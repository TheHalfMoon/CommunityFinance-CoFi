//! Strict original disbursement submission/terminal source event codecs.
//!
//! These are caller-supplied lifecycle facts, not authenticated provider
//! observations. All state transitions are delegated to DisbursementEngine.
//! No network, provider dispatch, balance posting or private-map hydration.

use cofi_community::CommunityRegistry;
use cofi_disbursements::{
    DisbursementEngine, DisbursementEventId, DisbursementId, DisbursementSubmission,
    DisbursementTerminalEvent, FailureCode, ProviderEventReference, ProviderRequestReference,
    ProviderSettlementReference, TerminalKind,
};
use cofi_governance::GovernanceEngine;
use cofi_ledger::Ledger;
use serde::{Deserialize, Serialize};

use crate::disbursement_creation::decode_disbursement_creation;
use crate::governance_fund_spend::decode_governance_fund_spend;
use crate::{CodecError, parse_i64_exact};

const VERSION: u64 = 1;
const SUBMISSION_KIND: &str = "disbursement.submission";
const TERMINAL_KIND: &str = "disbursement.terminal";
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
struct SubmissionRecord {
    source_event_id: String,
    disbursement_id: String,
    provider_request_reference: String,
    submitted_at_unix_ms: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum StoredTerminalKind {
    Settled { settlement_reference: String },
    Failed { failure_code: String },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalRecord {
    source_event_id: String,
    disbursement_id: String,
    provider_event_reference: String,
    kind: StoredTerminalKind,
    terminal_at_unix_ms: String,
}
fn invalid<E: std::fmt::Display>(e: E) -> CodecError {
    CodecError::InvalidDomain(e.to_string())
}
fn encode<T: Serialize>(kind: &str, payload: T) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: kind.to_owned(),
        payload,
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "disbursement fact exceeds 1MiB".into(),
        ));
    }
    Ok(bytes)
}
fn decode<'a, T: Deserialize<'a>>(kind: &str, bytes: &'a [u8]) -> Result<T, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "disbursement fact exceeds 1MiB".into(),
        ));
    }
    let record: Envelope<T> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if record.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(record.schema_version));
    }
    if record.record_type != kind {
        return Err(CodecError::UnsupportedRecordKind(record.record_type));
    }
    Ok(record.payload)
}
pub fn encode_disbursement_submission(
    submission: &DisbursementSubmission,
) -> Result<Vec<u8>, CodecError> {
    let bytes = encode(
        SUBMISSION_KIND,
        SubmissionRecord {
            source_event_id: submission.source_event_id().as_str().to_owned(),
            disbursement_id: submission.disbursement_id().as_str().to_owned(),
            provider_request_reference: submission.provider_request_reference().as_str().to_owned(),
            submitted_at_unix_ms: submission.submitted_at_unix_ms().to_string(),
        },
    )?;
    if decode_disbursement_submission(&bytes)? != *submission {
        return Err(CodecError::Replay("submission roundtrip mismatch".into()));
    }
    Ok(bytes)
}
pub fn decode_disbursement_submission(bytes: &[u8]) -> Result<DisbursementSubmission, CodecError> {
    let record: SubmissionRecord = decode(SUBMISSION_KIND, bytes)?;
    Ok(DisbursementSubmission::new(
        DisbursementEventId::new(record.source_event_id).map_err(invalid)?,
        DisbursementId::new(record.disbursement_id).map_err(invalid)?,
        ProviderRequestReference::new(record.provider_request_reference).map_err(invalid)?,
        parse_i64_exact(&record.submitted_at_unix_ms)?,
    ))
}
pub fn encode_disbursement_terminal(
    terminal: &DisbursementTerminalEvent,
) -> Result<Vec<u8>, CodecError> {
    let kind = match terminal.kind() {
        TerminalKind::Settled {
            settlement_reference,
        } => StoredTerminalKind::Settled {
            settlement_reference: settlement_reference.as_str().to_owned(),
        },
        TerminalKind::Failed { failure_code } => StoredTerminalKind::Failed {
            failure_code: failure_code.as_str().to_owned(),
        },
    };
    let bytes = encode(
        TERMINAL_KIND,
        TerminalRecord {
            source_event_id: terminal.source_event_id().as_str().to_owned(),
            disbursement_id: terminal.disbursement_id().as_str().to_owned(),
            provider_event_reference: terminal.provider_event_reference().as_str().to_owned(),
            kind,
            terminal_at_unix_ms: terminal.terminal_at_unix_ms().to_string(),
        },
    )?;
    if decode_disbursement_terminal(&bytes)? != *terminal {
        return Err(CodecError::Replay("terminal roundtrip mismatch".into()));
    }
    Ok(bytes)
}
pub fn decode_disbursement_terminal(bytes: &[u8]) -> Result<DisbursementTerminalEvent, CodecError> {
    let record: TerminalRecord = decode(TERMINAL_KIND, bytes)?;
    let source_event_id = DisbursementEventId::new(record.source_event_id).map_err(invalid)?;
    let disbursement_id = DisbursementId::new(record.disbursement_id).map_err(invalid)?;
    let provider_event_reference =
        ProviderEventReference::new(record.provider_event_reference).map_err(invalid)?;
    let at = parse_i64_exact(&record.terminal_at_unix_ms)?;
    Ok(match record.kind {
        StoredTerminalKind::Settled {
            settlement_reference,
        } => DisbursementTerminalEvent::settled(
            source_event_id,
            disbursement_id,
            provider_event_reference,
            ProviderSettlementReference::new(settlement_reference).map_err(invalid)?,
            at,
        ),
        StoredTerminalKind::Failed { failure_code } => DisbursementTerminalEvent::failed(
            source_event_id,
            disbursement_id,
            provider_event_reference,
            FailureCode::new(failure_code).map_err(invalid)?,
            at,
        ),
    })
}

/// Original accepted source fact sequence, *not* independently witnessed.
#[derive(Clone, Copy, Debug)]
pub enum DisbursementLifecycleFact<'a> {
    Creation { creation: &'a [u8], spend: &'a [u8] },
    Submission(&'a [u8]),
    Terminal(&'a [u8]),
}
/// Rebuild original registry transitions in the supplied causal event order.
/// Caller must independently authenticate provider receipt, source completeness
/// and tenant/history cutoff; this routine never contacts a provider.
pub fn replay_disbursement_lifecycle<'a>(
    facts: impl IntoIterator<Item = DisbursementLifecycleFact<'a>>,
    community: &CommunityRegistry,
    governance: &GovernanceEngine,
    ledger: &Ledger,
) -> Result<DisbursementEngine, CodecError> {
    let mut engine = DisbursementEngine::new();
    for fact in facts {
        match fact {
            DisbursementLifecycleFact::Creation { creation, spend } => {
                engine
                    .create(
                        community,
                        governance,
                        ledger,
                        &decode_governance_fund_spend(spend)?,
                        decode_disbursement_creation(creation)?,
                    )
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
            }
            DisbursementLifecycleFact::Submission(bytes) => {
                engine
                    .submit(decode_disbursement_submission(bytes)?)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
            }
            DisbursementLifecycleFact::Terminal(bytes) => {
                engine
                    .record_terminal(decode_disbursement_terminal(bytes)?)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
            }
        }
    }
    Ok(engine)
}
