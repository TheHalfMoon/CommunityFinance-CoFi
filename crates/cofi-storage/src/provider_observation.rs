//! Bounded exact transport of a caller-provided original ProviderObservation.
//!
//! IMPORTANT: Decoding does NOT authenticate provider origin, signature,
//! accepted source root, full tenant cutoff or a real payment. Bind and
//! replay only after independent provenance and canonical Disbursement check.

use std::collections::BTreeSet;

use cofi_disbursements::{
    Disbursement, DisbursementEventId, DisbursementId, FailureCode, ProviderEventReference,
    ProviderRequestReference, ProviderSettlementReference,
};
use cofi_provider_contract::{ProviderObservation, ProviderObservationKind};
use cofi_reconciliation::{ReconciliationCase, ReconciliationEngine, ReconciliationOutcome};
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

/// A diagnostic-only projection of a caller-supplied reconciliation result.
/// In particular, ProviderAhead never exposes its proposed terminal event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UntrustedReconciliationDiagnostic {
    PendingAgreement,
    ProviderAhead,
    TerminalAgreement,
    Discrepancy,
}

/// Deterministic original-domain calculation over **caller-supplied** records.
/// The private domain result is never exposed as a clonable terminal command.
///
/// Only non-command diagnostics may be inspected:
///
/// ```
/// use cofi_storage::provider_observation::{UnauthenticatedReconciliationOutcome, UntrustedReconciliationDiagnostic};
/// fn inspect(candidate: &UnauthenticatedReconciliationOutcome) -> UntrustedReconciliationDiagnostic {
///     candidate.diagnostic()
/// }
/// ```
///
/// The old public accessor must not compile, because it could expose a
/// clonable `ReconciliationOutcome::ProviderAhead.proposed_terminal_event`:
///
/// ```compile_fail
/// use cofi_storage::provider_observation::UnauthenticatedReconciliationOutcome;
/// fn extract_terminal_event(candidate: &UnauthenticatedReconciliationOutcome) {
///     let _ = candidate.outcome();
/// }
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct UnauthenticatedReconciliationOutcome {
    outcome: ReconciliationOutcome,
}

// Never derive Debug for this wrapper. The original ProviderAhead outcome
// contains a proposed terminal event whose identifiers must remain opaque to
// untrusted callers, including logging and diagnostic serialization.
impl std::fmt::Debug for UnauthenticatedReconciliationOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UnauthenticatedReconciliationOutcome")
            .field("diagnostic", &self.diagnostic())
            .finish()
    }
}

impl UnauthenticatedReconciliationOutcome {
    /// Only a diagnostic projection is visible to untrusted callers.
    #[must_use]
    pub const fn diagnostic(&self) -> UntrustedReconciliationDiagnostic {
        match self.outcome {
            ReconciliationOutcome::PendingAgreement { .. } => {
                UntrustedReconciliationDiagnostic::PendingAgreement
            }
            ReconciliationOutcome::ProviderAhead { .. } => {
                UntrustedReconciliationDiagnostic::ProviderAhead
            }
            ReconciliationOutcome::TerminalAgreement { .. } => {
                UntrustedReconciliationDiagnostic::TerminalAgreement
            }
            ReconciliationOutcome::Discrepancy { .. } => {
                UntrustedReconciliationDiagnostic::Discrepancy
            }
        }
    }

    /// No local consistency or synthetic case can prove external source custody.
    /// An independently authenticated replay protocol must make that decision.
    pub fn require_independent_source_authentication(
        self,
    ) -> Result<ReconciliationOutcome, CodecError> {
        Err(CodecError::Replay(
            "UNAUTHENTICATED: original provider and disbursement custody unverified".into(),
        ))
    }
}

/// Recompute by delegating to the real original ReconciliationEngine, rather
/// than decoding/trusting a stored result label or bypassing the provider contract.
/// A supplied Disbursement is NOT, by this function alone, a canonical one.
pub fn recompute_untrusted_reconciliation(
    case_bytes: &[u8],
    observation_bytes: &[u8],
    disbursement: &Disbursement,
) -> Result<UnauthenticatedReconciliationOutcome, CodecError> {
    let (case, observation) = check_untrusted_case_observation_pair(case_bytes, observation_bytes)?;
    let outcome = ReconciliationEngine::new()
        .reconcile(&case, disbursement, &observation)
        .map_err(|error| {
            CodecError::Replay(format!(
                "original domain reconciliation rejected untrusted inputs: {error}"
            ))
        })?;
    Ok(UnauthenticatedReconciliationOutcome { outcome })
}

/// One **caller-supplied** case, provider observation and disbursement from a
/// proposed first-time source batch; none has authenticated source custody.
#[derive(Clone, Copy)]
pub struct UntrustedReconciliationRecord<'a> {
    pub case_bytes: &'a [u8],
    pub observation_bytes: &'a [u8],
    pub disbursement: &'a Disbursement,
}

/// A bounded, deterministic list of original-domain candidate calculations.
/// This has NO authority to admit any event as accepted or move real funds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnauthenticatedReconciliationBatch {
    outcomes: Vec<UnauthenticatedReconciliationOutcome>,
}

impl UnauthenticatedReconciliationBatch {
    #[must_use]
    pub fn outcomes(&self) -> &[UnauthenticatedReconciliationOutcome] {
        &self.outcomes
    }

    /// Reconstructed consistency is not a substitute for externally verified
    /// provider source, complete tenant stream or accepted ledger custody.
    pub fn require_independent_source_authentication(
        self,
    ) -> Result<Vec<ReconciliationOutcome>, CodecError> {
        Err(CodecError::Replay(
            "UNAUTHENTICATED: first-time reconciliation source custody unverified".into(),
        ))
    }
}

/// Recalculate a single **caller-declared, first-time** set of unique original
/// reconciliation cases, disbursements and provider events with the original engine. This
/// checks only internal duplicates inside *this input set*: a later provider
/// recheck with a previously observed event is intentionally not covered by
/// this first-time-only API. No tenant, provider namespace or complete source
/// boundary can be independently authenticated using these DTOs alone.
///
/// Fail closed at 4096 records and 16 MiB aggregate serialized input, before
/// decoding any original domain object or calculating a potential result.
pub fn recompute_untrusted_first_time_reconciliation_batch(
    records: &[UntrustedReconciliationRecord<'_>],
) -> Result<UnauthenticatedReconciliationBatch, CodecError> {
    const MAX_RECORDS: usize = 4096;
    const MAX_BYTES: usize = 16 * 1024 * 1024;
    if records.is_empty() || records.len() > MAX_RECORDS {
        return Err(CodecError::Replay(
            "invalid first-time reconciliation batch record count".into(),
        ));
    }
    let mut total_bytes = 0usize;
    for record in records {
        total_bytes = total_bytes
            .checked_add(record.case_bytes.len())
            .and_then(|n| n.checked_add(record.observation_bytes.len()))
            .ok_or_else(|| {
                CodecError::Replay("reconciliation batch input byte count overflow".into())
            })?;
        if total_bytes > MAX_BYTES {
            return Err(CodecError::Replay(
                "reconciliation batch input exceeds 16 MiB".into(),
            ));
        }
    }
    let mut cases = BTreeSet::new();
    let mut disbursements = BTreeSet::new();
    let mut provider_events = BTreeSet::new();
    let mut terminal_events = BTreeSet::new();
    let mut settlement_receipts = BTreeSet::new();
    let mut outcomes = Vec::with_capacity(records.len());
    for record in records {
        let case = decode_reconciliation_case(record.case_bytes)?;
        let observation = decode_provider_observation(record.observation_bytes)?;
        if !cases.insert(case.id().as_str().to_owned()) {
            return Err(CodecError::Replay(
                "duplicate original reconciliation case in declared first-time batch".into(),
            ));
        }
        // Two distinct case and provider-event identifiers may still refer to
        // one disbursement. A first-time snapshot must not count that as two
        // independently admissible records, even if each case is domain-valid.
        if !disbursements.insert(record.disbursement.id().as_str().to_owned()) {
            return Err(CodecError::Replay(
                "duplicate original disbursement in declared first-time batch".into(),
            ));
        }
        let (event, terminal, settlement) = match observation.kind() {
            ProviderObservationKind::Accepted {
                provider_event_reference,
            } => (provider_event_reference, None, None),
            ProviderObservationKind::Settled {
                provider_event_reference,
                lifecycle_event_id,
                settlement_reference,
            } => (
                provider_event_reference,
                Some(lifecycle_event_id),
                Some(settlement_reference),
            ),
            ProviderObservationKind::Failed {
                provider_event_reference,
                lifecycle_event_id,
                ..
            } => (provider_event_reference, Some(lifecycle_event_id), None),
        };
        if !provider_events.insert(event.as_str().to_owned()) {
            return Err(CodecError::Replay(
                "reused original provider event reference in first-time batch".into(),
            ));
        }
        if let Some(event) = terminal {
            if !terminal_events.insert(event.as_str().to_owned()) {
                return Err(CodecError::Replay(
                    "reused original terminal event identity in first-time batch".into(),
                ));
            }
        }
        if let Some(reference) = settlement {
            if !settlement_receipts.insert(reference.as_str().to_owned()) {
                return Err(CodecError::Replay(
                    "reused original provider settlement in first-time batch".into(),
                ));
            }
        }
        // The original ReconciliationEngine, not a hand-written status
        // approximation or serialized outcome label, calculates every result.
        // Its domain status/ID/request/timestamp guards remain mandatory.
        let candidate = ReconciliationEngine::new()
            .reconcile(&case, record.disbursement, &observation)
            .map_err(|error| {
                CodecError::Replay(format!(
                    "original engine rejected first-time reconciliation candidate: {error}"
                ))
            })?;
        outcomes.push(UnauthenticatedReconciliationOutcome { outcome: candidate });
    }
    Ok(UnauthenticatedReconciliationBatch { outcomes })
}
