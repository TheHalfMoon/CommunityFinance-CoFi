//! Versioned P22 invoice-draft authorization source lineage replay.
//!
//! Every recovered AuthorizedRating must come from the verified P21 records,
//! never a serialized charge or unchecked authorization constructor.
//! The original AuthorizedDraftRegistry::assemble alone creates results.

use std::collections::BTreeMap;

use cofi_billing::{BillingEventId, BillingInvoiceId};
use cofi_invoice_authorization::{
    AuthorizedDraft, AuthorizedDraftRegistry, AuthorizedDraftRequest,
};
use cofi_rating_authorization::AuthorizedRating;
use serde::{Deserialize, Serialize};

use crate::authorized_rating::decode_authorized_rating;
use crate::{CodecError, parse_i64_exact, parse_i128_exact};

const VERSION: u64 = 1;
const KIND: &str = "authorized.draft";
const MAX_AUTHORIZED_FACT_BYTES: usize = 1024 * 1024;
const MAX_SOURCE_RECEIPT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftRecord {
    event_id: String,
    invoice_id: String,
    expected_invoice_id: String,
    period_start_unix_ms: String,
    period_end_unix_ms: String,
    observed_at_unix_ms: String,
    authorizations: Vec<serde_json::Value>,
    expected_scope: String,
    expected_customer_id: String,
    expected_total_minor: String,
    expected_charge_ids: Vec<String>,
}

fn typed(bytes: &[u8]) -> Result<DraftRecord, CodecError> {
    if bytes.len() > MAX_AUTHORIZED_FACT_BYTES {
        return Err(CodecError::InvalidPayload(
            "authorized evidence exceeds 1MiB".to_owned(),
        ));
    }
    let header: Envelope<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if header.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(header.schema_version));
    }
    if header.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(header.record_type));
    }
    let env: Envelope<DraftRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    Ok(env.payload)
}

fn checked_request(record: &DraftRecord) -> Result<AuthorizedDraftRequest, CodecError> {
    let mut ratings = Vec::<AuthorizedRating>::new();
    for evidence in &record.authorizations {
        let bytes =
            serde_json::to_vec(evidence).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
        ratings.push(decode_authorized_rating(&bytes)?);
    }
    AuthorizedDraftRequest::new(
        BillingEventId::new(&record.event_id)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        BillingInvoiceId::new(&record.invoice_id)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        parse_i64_exact(&record.period_start_unix_ms)?,
        parse_i64_exact(&record.period_end_unix_ms)?,
        parse_i64_exact(&record.observed_at_unix_ms)?,
        ratings,
    )
    .map_err(|e| CodecError::InvalidDomain(e.to_string()))
}

fn validated(
    record: &DraftRecord,
) -> Result<(AuthorizedDraftRequest, AuthorizedDraft), CodecError> {
    let request = checked_request(record)?;
    let draft = AuthorizedDraftRegistry::new()
        .assemble(request.clone())
        .map_err(|e| CodecError::Replay(e.to_string()))?
        .authorization()
        .clone();
    if draft.draft().invoice_id().as_str() != record.expected_invoice_id
        || draft.draft().organization_scope().as_str() != record.expected_scope
        || draft.draft().customer_id().as_str() != record.expected_customer_id
        || draft.draft().total_minor() != parse_i128_exact(&record.expected_total_minor)?
        || draft
            .draft()
            .lines()
            .iter()
            .map(|l| l.rated_charge_id().as_str().to_owned())
            .collect::<Vec<_>>()
            != record.expected_charge_ids
    {
        return Err(CodecError::Replay(
            "P22 accepted draft differs from original authorized rating lineage".to_owned(),
        ));
    }
    Ok((request, draft))
}

pub fn encode_authorized_draft(
    request: &AuthorizedDraftRequest,
    source_p21_receipts: &[Vec<u8>],
    accepted: &AuthorizedDraft,
) -> Result<Vec<u8>, CodecError> {
    let authorizations = source_p21_receipts
        .iter()
        .map(|b| {
            if b.len() > MAX_SOURCE_RECEIPT_BYTES {
                return Err(CodecError::InvalidPayload(
                    "P21 source receipt exceeds 1MiB".to_owned(),
                ));
            }
            serde_json::from_slice(b).map_err(|e| CodecError::InvalidPayload(e.to_string()))
        })
        .collect::<Result<Vec<serde_json::Value>, CodecError>>()?;
    let record = DraftRecord {
        event_id: request.event_id().as_str().to_owned(),
        invoice_id: request.invoice_id().as_str().to_owned(),
        expected_invoice_id: accepted.draft().invoice_id().as_str().to_owned(),
        period_start_unix_ms: request.period_start_unix_ms().to_string(),
        period_end_unix_ms: request.period_end_unix_ms().to_string(),
        observed_at_unix_ms: request.observed_at_unix_ms().to_string(),
        authorizations,
        expected_scope: accepted.draft().organization_scope().as_str().to_owned(),
        expected_customer_id: accepted.draft().customer_id().as_str().to_owned(),
        expected_total_minor: accepted.draft().total_minor().to_string(),
        expected_charge_ids: accepted
            .draft()
            .lines()
            .iter()
            .map(|l| l.rated_charge_id().as_str().to_owned())
            .collect(),
    };
    let (source_request, reconstructed) = validated(&record)?;
    if source_request != *request || reconstructed != *accepted {
        return Err(CodecError::Replay(
            "P22 encoding conflicts with checked original authorization".to_owned(),
        ));
    }
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: record,
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_AUTHORIZED_FACT_BYTES {
        return Err(CodecError::InvalidPayload(
            "authorized evidence exceeds 1MiB".to_owned(),
        ));
    }
    Ok(bytes)
}

pub fn decode_authorized_draft(bytes: &[u8]) -> Result<AuthorizedDraft, CodecError> {
    Ok(validated(&typed(bytes)?)?.1)
}

/// Rebuild all P22 source-event/draft/charge-binding maps from the canonical
/// authorized registry. An identical record is replayed; source-swapping
/// under one P22 event ID or double-use of charges across drafts is refused.
pub fn replay_authorized_drafts<'a>(
    facts: impl IntoIterator<Item = &'a [u8]>,
) -> Result<AuthorizedDraftRegistry, CodecError> {
    let mut accepted: BTreeMap<BillingEventId, Vec<u8>> = BTreeMap::new();
    let mut registry = AuthorizedDraftRegistry::new();
    for bytes in facts {
        let record = typed(bytes)?;
        let (request, expected) = validated(&record)?;
        let canonical =
            serde_json::to_vec(&record).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
        if let Some(original) = accepted.get(request.event_id()) {
            if original != &canonical {
                return Err(CodecError::Replay(
                    "P22 draft event identity reused with changed source authorization".to_owned(),
                ));
            }
        }
        let result = registry
            .assemble(request.clone())
            .map_err(|e| CodecError::Replay(e.to_string()))?;
        if result.authorization() != &expected {
            return Err(CodecError::Replay(
                "P22 draft differs under combined authorized replay".to_owned(),
            ));
        }
        accepted.insert(request.event_id().clone(), canonical);
    }
    Ok(registry)
}
