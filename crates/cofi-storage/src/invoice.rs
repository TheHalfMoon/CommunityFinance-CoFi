//! Immutable draft-invoice evidence codec.
//!
//! Every rated line is reconstructed by the canonical rating engine from a
//! frozen rating acceptance receipt, not from unchecked RatedCharge DTOs.
//! This is not proof of completeness or authenticity of the external event log.

use std::collections::BTreeSet;

use cofi_billing::{BillingCustomerId, BillingEventId, BillingInvoiceId};
use cofi_invoicing::{DraftInvoiceRegistry, DraftInvoiceRequest};
use cofi_ledger::{Currency, LedgerScopeId};
use cofi_rating::RatingEventId;
use serde::{Deserialize, Serialize};

use crate::rating::replay_rating_acceptances;
use crate::{CodecError, parse_i64_exact, parse_i128_exact};

const VERSION: u64 = 1;
const DRAFT_KIND: &str = "invoice.draft";
const MAX_DRAFT_FACT_BYTES: usize = 1024 * 1024;
const MAX_RATING_RECEIPT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenRatedLine {
    rating_event_id: String,
    evidence: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftInvoiceRecord {
    source_event_id: String,
    invoice_id: String,
    customer_id: String,
    organization_scope: String,
    period_start_unix_ms: String,
    period_end_unix_ms: String,
    observed_at_unix_ms: String,
    expected_currency: String,
    expected_total_minor: String,
    ratings: Vec<FrozenRatedLine>,
}

/// Keep the externally accepted original rating receipts next to the draft.
/// The caller must already have immutable, original accepted source receipts.
pub fn encode_draft_invoice(
    request: &DraftInvoiceRequest,
    rating_receipts: &[Vec<u8>],
) -> Result<Vec<u8>, CodecError> {
    let ratings = rating_receipts
        .iter()
        .map(|bytes| {
            if bytes.len() > MAX_RATING_RECEIPT_BYTES {
                return Err(CodecError::InvalidPayload(
                    "embedded rating receipt exceeds 1MiB".to_owned(),
                ));
            }
            let evidence: serde_json::Value = serde_json::from_slice(bytes)
                .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
            let event_id = evidence
                .pointer("/payload/rating_event_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    CodecError::InvalidPayload("rating evidence is missing event ID".to_owned())
                })?;
            Ok(FrozenRatedLine {
                rating_event_id: event_id.to_owned(),
                evidence,
            })
        })
        .collect::<Result<Vec<_>, CodecError>>()?;
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: DRAFT_KIND.to_owned(),
        payload: DraftInvoiceRecord {
            source_event_id: request.source_event_id().as_str().to_owned(),
            invoice_id: request.invoice_id().as_str().to_owned(),
            customer_id: request.customer_id().as_str().to_owned(),
            organization_scope: request.organization_scope().as_str().to_owned(),
            period_start_unix_ms: request.period_start_unix_ms().to_string(),
            period_end_unix_ms: request.period_end_unix_ms().to_string(),
            observed_at_unix_ms: request.observed_at_unix_ms().to_string(),
            expected_currency: request.currency().code().to_owned(),
            expected_total_minor: request.total_minor().to_string(),
            ratings,
        },
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_DRAFT_FACT_BYTES {
        return Err(CodecError::InvalidPayload(
            "draft invoice fact exceeds 1MiB".to_owned(),
        ));
    }
    if decode_draft_invoice(&bytes)? != *request {
        return Err(CodecError::Replay(
            "draft request differs from recomputed original rating source evidence".to_owned(),
        ));
    }
    Ok(bytes)
}

/// Recompute source charges and invoice totals through checked domain engines.
/// Duplicate/missing/modified rated-charge identity and lineage fail closed.
pub fn decode_draft_invoice(bytes: &[u8]) -> Result<DraftInvoiceRequest, CodecError> {
    if bytes.len() > MAX_DRAFT_FACT_BYTES {
        return Err(CodecError::InvalidPayload(
            "draft invoice fact exceeds 1MiB".to_owned(),
        ));
    }
    let header: Envelope<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if header.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(header.schema_version));
    }
    if header.record_type != DRAFT_KIND {
        return Err(CodecError::UnsupportedRecordKind(header.record_type));
    }
    let decoded: Envelope<DraftInvoiceRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let record = decoded.payload;

    let mut ids = BTreeSet::new();
    let mut charges = Vec::with_capacity(record.ratings.len());
    for line in record.ratings {
        let id = RatingEventId::new(line.rating_event_id)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?;
        if !ids.insert(id.as_str().to_owned()) {
            return Err(CodecError::Replay(
                "duplicate rated charge source event in draft".to_owned(),
            ));
        }
        let receipt = serde_json::to_vec(&line.evidence)
            .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
        let replayed = replay_rating_acceptances([receipt.as_slice()])?;
        let charge = replayed.charge_for_event(&id).ok_or_else(|| {
            CodecError::Replay(
                "rating receipt does not match the referenced source event".to_owned(),
            )
        })?;
        charges.push(charge.clone());
    }
    let request = DraftInvoiceRequest::new(
        BillingEventId::new(record.source_event_id)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        BillingInvoiceId::new(record.invoice_id)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        BillingCustomerId::new(record.customer_id)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        LedgerScopeId::new(record.organization_scope)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        parse_i64_exact(&record.period_start_unix_ms)?,
        parse_i64_exact(&record.period_end_unix_ms)?,
        parse_i64_exact(&record.observed_at_unix_ms)?,
        charges,
    )
    .map_err(|e| CodecError::InvalidDomain(e.to_string()))?;
    if request.currency()
        != Currency::new(&record.expected_currency)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?
        || request.total_minor() != parse_i128_exact(&record.expected_total_minor)?
    {
        return Err(CodecError::Replay(
            "draft total/currency mismatch with rated charge evidence".to_owned(),
        ));
    }
    Ok(request)
}

/// Persisted snapshots are replayed as source facts via the canonical
/// domain assembly entrypoint to rebuild invoice and charge-binding indexes.
pub fn replay_draft_invoices<'a>(
    facts: impl IntoIterator<Item = &'a [u8]>,
) -> Result<DraftInvoiceRegistry, CodecError> {
    let mut registry = DraftInvoiceRegistry::new();
    for fact in facts {
        registry
            .assemble(decode_draft_invoice(fact)?)
            .map_err(|e| CodecError::Replay(e.to_string()))?;
    }
    Ok(registry)
}
