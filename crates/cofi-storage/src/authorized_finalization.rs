//! P23 accepted authorized finalization replay: strictly through original
//! P22 accepted source evidence and original finalization authorization.
//!
//! This is NOT a live invoice finalization endpoint and does not apply a
//! billing event or journal entry to any database or financial account.

use std::collections::BTreeMap;

use cofi_billing::BillingEventId;
use cofi_finalization_authorization::{
    AuthorizedFinalization, AuthorizedFinalizationRegistry, AuthorizedFinalizationRequest,
};
use serde::{Deserialize, Serialize};

use crate::authorized_draft::decode_authorized_draft;
use crate::{CodecError, parse_i64_exact, parse_i128_exact};

const VERSION: u64 = 1;
const KIND: &str = "authorized.finalization";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FinalizationRecord {
    source_event_id: String,
    expected_source_event_id: String,
    authorized_draft: serde_json::Value,
    finalized_at_unix_ms: String,
    observed_at_unix_ms: String,
    expected_invoice_id: String,
    expected_organization_scope: String,
    expected_customer_id: String,
    expected_total_minor: String,
}

fn typed(bytes: &[u8]) -> Result<FinalizationRecord, CodecError> {
    let header: Envelope<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if header.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(header.schema_version));
    }
    if header.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(header.record_type));
    }
    let env: Envelope<FinalizationRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    Ok(env.payload)
}

fn checked_request(
    record: &FinalizationRecord,
) -> Result<AuthorizedFinalizationRequest, CodecError> {
    if record.source_event_id != record.expected_source_event_id {
        return Err(CodecError::Replay(
            "finalization source event identity changed".to_owned(),
        ));
    }
    let p22 = serde_json::to_vec(&record.authorized_draft)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let authorized_draft = decode_authorized_draft(&p22)?;
    Ok(AuthorizedFinalizationRequest::new(
        BillingEventId::new(&record.source_event_id)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        authorized_draft,
        parse_i64_exact(&record.finalized_at_unix_ms)?,
        parse_i64_exact(&record.observed_at_unix_ms)?,
    ))
}

fn validated(
    record: &FinalizationRecord,
) -> Result<(AuthorizedFinalizationRequest, AuthorizedFinalization), CodecError> {
    let request = checked_request(record)?;
    let finalized = AuthorizedFinalizationRegistry::new()
        .finalize(request.clone())
        .map_err(|e| CodecError::Replay(e.to_string()))?
        .authorization()
        .clone();
    let actual = finalized.finalized();
    if actual.draft().invoice_id().as_str() != record.expected_invoice_id
        || actual.draft().organization_scope().as_str() != record.expected_organization_scope
        || actual.draft().customer_id().as_str() != record.expected_customer_id
        || actual.draft().total_minor() != parse_i128_exact(&record.expected_total_minor)?
    {
        return Err(CodecError::Replay(
            "P23 finalization differs from original authorized draft lineage".to_owned(),
        ));
    }
    Ok((request, finalized))
}

/// Encode only a finalization that the original authorized finalization
/// registry can reproduce from the supplied P22 source evidence.
pub fn encode_authorized_finalization(
    request: &AuthorizedFinalizationRequest,
    p22_receipt: &[u8],
    accepted: &AuthorizedFinalization,
) -> Result<Vec<u8>, CodecError> {
    let source: serde_json::Value = serde_json::from_slice(p22_receipt)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let record = FinalizationRecord {
        source_event_id: request.source_event_id().as_str().to_owned(),
        expected_source_event_id: accepted.finalized().source_event_id().as_str().to_owned(),
        authorized_draft: source,
        finalized_at_unix_ms: request.finalized_at_unix_ms().to_string(),
        observed_at_unix_ms: request.observed_at_unix_ms().to_string(),
        expected_invoice_id: accepted
            .finalized()
            .draft()
            .invoice_id()
            .as_str()
            .to_owned(),
        expected_organization_scope: accepted
            .finalized()
            .draft()
            .organization_scope()
            .as_str()
            .to_owned(),
        expected_customer_id: accepted
            .finalized()
            .draft()
            .customer_id()
            .as_str()
            .to_owned(),
        expected_total_minor: accepted.finalized().draft().total_minor().to_string(),
    };
    let (original_request, result) = validated(&record)?;
    if original_request != *request || result != *accepted {
        return Err(CodecError::Replay(
            "P23 source and accepted authorization disagree".to_owned(),
        ));
    }
    serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: record,
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))
}

pub fn decode_authorized_finalization(bytes: &[u8]) -> Result<AuthorizedFinalization, CodecError> {
    Ok(validated(&typed(bytes)?)?.1)
}

/// Replay finalizations only through original P23 finalization authorization.
/// Never apply the projected billing event to a ledger during hydration.
pub fn replay_authorized_finalizations<'a>(
    facts: impl IntoIterator<Item = &'a [u8]>,
) -> Result<AuthorizedFinalizationRegistry, CodecError> {
    let mut registry = AuthorizedFinalizationRegistry::new();
    let mut history: BTreeMap<BillingEventId, Vec<u8>> = BTreeMap::new();
    for bytes in facts {
        let record = typed(bytes)?;
        let (request, expected) = validated(&record)?;
        let canonical =
            serde_json::to_vec(&record).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
        if let Some(first) = history.get(request.source_event_id()) {
            if first != &canonical {
                return Err(CodecError::Replay(
                    "P23 event reused with changed draft evidence".to_owned(),
                ));
            }
        }
        let outcome = registry
            .finalize(request.clone())
            .map_err(|e| CodecError::Replay(e.to_string()))?;
        if outcome.authorization() != &expected {
            return Err(CodecError::Replay(
                "P23 canonical finalization changed during replay".to_owned(),
            ));
        }
        history.insert(request.source_event_id().clone(), canonical);
    }
    Ok(registry)
}
