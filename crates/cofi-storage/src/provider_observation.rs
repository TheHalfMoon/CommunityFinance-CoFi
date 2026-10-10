//! Bounded exact transport of a caller-provided original ProviderObservation.
//!
//! IMPORTANT: Decoding does NOT authenticate provider origin, signature,
//! accepted source root, full tenant cutoff or a real payment. Bind and
//! replay only after independent provenance and canonical Disbursement check.

use cofi_disbursements::{
    DisbursementEventId, DisbursementId, FailureCode, ProviderEventReference,
    ProviderRequestReference, ProviderSettlementReference,
};
use cofi_provider_contract::{ProviderObservation, ProviderObservationKind};
use cofi_reconciliation::ReconciliationCase;
use serde::{Deserialize, Serialize};

use crate::reconciliation_case::decode_reconciliation_case;
use crate::{CodecError, parse_i64_exact};

const VERSION: u64 = 1;
const KIND: &str = "provider.observation";
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
struct ObservationRecord {
    disbursement_id: String,
    provider_request_reference: String,
    kind: ObservationKindRecord,
    occurred_at_unix_ms: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ObservationKindRecord {
    Accepted {
        provider_event_reference: String,
    },
    Settled {
        lifecycle_event_id: String,
        provider_event_reference: String,
        settlement_reference: String,
    },
    Failed {
        lifecycle_event_id: String,
        provider_event_reference: String,
        failure_code: String,
    },
}

fn invalid<E: std::fmt::Display>(e: E) -> CodecError {
    CodecError::InvalidDomain(e.to_string())
}

impl ObservationRecord {
    fn from_original(original: &ProviderObservation) -> Self {
        let kind = match original.kind() {
            ProviderObservationKind::Accepted {
                provider_event_reference,
            } => ObservationKindRecord::Accepted {
                provider_event_reference: provider_event_reference.as_str().to_owned(),
            },
            ProviderObservationKind::Settled {
                lifecycle_event_id,
                provider_event_reference,
                settlement_reference,
            } => ObservationKindRecord::Settled {
                lifecycle_event_id: lifecycle_event_id.as_str().to_owned(),
                provider_event_reference: provider_event_reference.as_str().to_owned(),
                settlement_reference: settlement_reference.as_str().to_owned(),
            },
            ProviderObservationKind::Failed {
                lifecycle_event_id,
                provider_event_reference,
                failure_code,
            } => ObservationKindRecord::Failed {
                lifecycle_event_id: lifecycle_event_id.as_str().to_owned(),
                provider_event_reference: provider_event_reference.as_str().to_owned(),
                failure_code: failure_code.as_str().to_owned(),
            },
        };
        Self {
            disbursement_id: original.disbursement_id().as_str().to_owned(),
            provider_request_reference: original.provider_request_reference().as_str().to_owned(),
            kind,
            occurred_at_unix_ms: original.occurred_at_unix_ms().to_string(),
        }
    }

    fn checked(self) -> Result<ProviderObservation, CodecError> {
        let disbursement_id = DisbursementId::new(self.disbursement_id).map_err(invalid)?;
        let request =
            ProviderRequestReference::new(self.provider_request_reference).map_err(invalid)?;
        let at = parse_i64_exact(&self.occurred_at_unix_ms)?;
        Ok(match self.kind {
            ObservationKindRecord::Accepted {
                provider_event_reference,
            } => ProviderObservation::accepted(
                disbursement_id,
                request,
                ProviderEventReference::new(provider_event_reference).map_err(invalid)?,
                at,
            ),
            ObservationKindRecord::Settled {
                lifecycle_event_id,
                provider_event_reference,
                settlement_reference,
            } => ProviderObservation::settled(
                disbursement_id,
                request,
                DisbursementEventId::new(lifecycle_event_id).map_err(invalid)?,
                ProviderEventReference::new(provider_event_reference).map_err(invalid)?,
                ProviderSettlementReference::new(settlement_reference).map_err(invalid)?,
                at,
            ),
            ObservationKindRecord::Failed {
                lifecycle_event_id,
                provider_event_reference,
                failure_code,
            } => ProviderObservation::failed(
                disbursement_id,
                request,
                DisbursementEventId::new(lifecycle_event_id).map_err(invalid)?,
                ProviderEventReference::new(provider_event_reference).map_err(invalid)?,
                FailureCode::new(failure_code).map_err(invalid)?,
                at,
            ),
        })
    }
}

/// Encode an original domain observation, not an authenticated provider receipt.
pub fn encode_provider_observation(original: &ProviderObservation) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: ObservationRecord::from_original(original),
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "provider observation exceeds 1MiB".into(),
        ));
    }
    if decode_provider_observation(&bytes)? != *original {
        return Err(CodecError::Replay(
            "original provider observation roundtrip differs".into(),
        ));
    }
    Ok(bytes)
}

/// Decode checked ORIGINAL transport variant; external signature and event
/// acceptance remain completely unverified at this boundary.
pub fn decode_provider_observation(bytes: &[u8]) -> Result<ProviderObservation, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "provider observation exceeds 1MiB".into(),
        ));
    }
    let wire: Envelope<ObservationRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if wire.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(wire.schema_version));
    }
    if wire.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(wire.record_type));
    }
    wire.payload.checked()
}

/// Compare a case and observation supplied by an **untrusted caller**.
/// Cannot prove valid canonical Disbursement status, original observation,
/// accepted provider event, or any authoritative reconciliation outcome.
pub fn check_untrusted_case_observation_pair(
    case_bytes: &[u8],
    observation_bytes: &[u8],
) -> Result<(ReconciliationCase, ProviderObservation), CodecError> {
    let case = decode_reconciliation_case(case_bytes)?;
    let observation = decode_provider_observation(observation_bytes)?;
    if case.disbursement_id() != observation.disbursement_id()
        || case.provider_request_reference() != observation.provider_request_reference()
        || case.reconciled_at_unix_ms() < observation.occurred_at_unix_ms()
    {
        return Err(CodecError::Replay(
            "untrusted case/observation binding or chronology mismatch".into(),
        ));
    }
    Ok((case, observation))
}
