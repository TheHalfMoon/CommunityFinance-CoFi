//! Exact codec for an original *request/context* reconciliation case only.
//!
//! This does NOT store or attest an accepted reconciliation outcome, signed
//! provider observation, final disbursement state or trusted source cutoff.
//! ReconciliationEngine::reconcile must be reapplied to independently verified
//! canonical Disbursement and ProviderObservation objects at replay time.

use cofi_disbursements::{DisbursementId, ProviderRequestReference};
use cofi_reconciliation::{ReconciliationCase, ReconciliationCaseId};
use serde::{Deserialize, Serialize};

use crate::{CodecError, parse_i64_exact};

const VERSION: u64 = 1;
const KIND: &str = "reconciliation.case";
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseRecord {
    id: String,
    disbursement_id: String,
    provider_request_reference: String,
    reconciled_at_unix_ms: String,
}

fn invalid<E: std::fmt::Display>(e: E) -> CodecError {
    CodecError::InvalidDomain(e.to_string())
}

impl CaseRecord {
    fn from_original(case: &ReconciliationCase) -> Self {
        Self {
            id: case.id().as_str().to_owned(),
            disbursement_id: case.disbursement_id().as_str().to_owned(),
            provider_request_reference: case.provider_request_reference().as_str().to_owned(),
            reconciled_at_unix_ms: case.reconciled_at_unix_ms().to_string(),
        }
    }

    fn checked(self) -> Result<ReconciliationCase, CodecError> {
        Ok(ReconciliationCase::new(
            ReconciliationCaseId::new(self.id).map_err(invalid)?,
            DisbursementId::new(self.disbursement_id).map_err(invalid)?,
            ProviderRequestReference::new(self.provider_request_reference).map_err(invalid)?,
            parse_i64_exact(&self.reconciled_at_unix_ms)?,
        ))
    }
}

/// Serialize only original immutable reconciliation case context.
/// An encoded case is NOT a verdict or provider observation.
pub fn encode_reconciliation_case(case: &ReconciliationCase) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: CaseRecord::from_original(case),
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "reconciliation case exceeds 1MiB".into(),
        ));
    }
    if decode_reconciliation_case(&bytes)? != *case {
        return Err(CodecError::Replay(
            "original reconciliation case roundtrip differs".into(),
        ));
    }
    Ok(bytes)
}

/// Check version, type, exact decimal timestamp and ORIGINAL domain IDs.
/// No accepted outcome, provider authority or unique-case index is implied.
pub fn decode_reconciliation_case(bytes: &[u8]) -> Result<ReconciliationCase, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "reconciliation case exceeds 1MiB".into(),
        ));
    }
    let wire: Envelope<CaseRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if wire.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(wire.schema_version));
    }
    if wire.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(wire.record_type));
    }
    wire.payload.checked()
}
