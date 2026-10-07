//! Versioned, checked audit-event records.
//!
//! The codec reconstructs the ORIGINAL audit event via AuditEvent::new and
//! compares the canonical digest. AuditLog::append subsequently checks
//! per-stream sequence and previous-digest ancestry. Hashes alone do not
//! authenticate the source or prove that the complete stream was supplied.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_disbursements::{DisbursementId, DisbursementStatus, ProviderRequestReference};
use cofi_reconciliation::{DiscrepancyKind, ReconciliationCaseId};
use serde::{Deserialize, Serialize};

use super::{
    ActorId, AuditAction, AuditAttribution, AuditDigest, AuditError, AuditEvent, AuditEventId,
    AuditLog, AuditPayload, AuditPosition, AuditResource, AuditStreamId, AuditTiming, CausationId,
    CorrelationId, ReconciliationAuditOutcomeKind, ReconciliationAuditPayload,
};

const SCHEMA_VERSION: u64 = 1;
const RECORD_TYPE: &str = "audit.reconciliation";
// Record-intake bound, not a trusted total replay-stream limit.
const MAX_RECORD_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub enum AuditCodecError {
    InvalidRecord(String),
    UnsupportedVersion(u64),
    Domain(AuditError),
    DigestMismatch,
}

impl Display for AuditCodecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRecord(s) => write!(f, "invalid audit record: {s}"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported audit schema version: {v}"),
            Self::Domain(e) => write!(f, "audit domain rejected record: {e}"),
            Self::DigestMismatch => write!(f, "audit digest does not match canonical event"),
        }
    }
}
impl Error for AuditCodecError {}
impl From<AuditError> for AuditCodecError {
    fn from(error: AuditError) -> Self {
        Self::Domain(error)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OutcomeKind {
    PendingAgreement,
    ProviderAhead,
    TerminalAgreement,
    Discrepancy,
}
impl From<ReconciliationAuditOutcomeKind> for OutcomeKind {
    fn from(value: ReconciliationAuditOutcomeKind) -> Self {
        match value {
            ReconciliationAuditOutcomeKind::PendingAgreement => Self::PendingAgreement,
            ReconciliationAuditOutcomeKind::ProviderAhead => Self::ProviderAhead,
            ReconciliationAuditOutcomeKind::TerminalAgreement => Self::TerminalAgreement,
            ReconciliationAuditOutcomeKind::Discrepancy => Self::Discrepancy,
        }
    }
}
impl From<OutcomeKind> for ReconciliationAuditOutcomeKind {
    fn from(value: OutcomeKind) -> Self {
        match value {
            OutcomeKind::PendingAgreement => Self::PendingAgreement,
            OutcomeKind::ProviderAhead => Self::ProviderAhead,
            OutcomeKind::TerminalAgreement => Self::TerminalAgreement,
            OutcomeKind::Discrepancy => Self::Discrepancy,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredStatus {
    Ready,
    Submitted,
    Settled,
    Failed,
}
impl From<DisbursementStatus> for StoredStatus {
    fn from(value: DisbursementStatus) -> Self {
        match value {
            DisbursementStatus::Ready => Self::Ready,
            DisbursementStatus::Submitted => Self::Submitted,
            DisbursementStatus::Settled => Self::Settled,
            DisbursementStatus::Failed => Self::Failed,
        }
    }
}
impl From<StoredStatus> for DisbursementStatus {
    fn from(value: StoredStatus) -> Self {
        match value {
            StoredStatus::Ready => Self::Ready,
            StoredStatus::Submitted => Self::Submitted,
            StoredStatus::Settled => Self::Settled,
            StoredStatus::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredDiscrepancy {
    AcceptedAfterTerminal,
    TerminalStatusMismatch,
    ProviderEventReferenceMismatch,
    SettlementReferenceMismatch,
    FailureCodeMismatch,
    TerminalTimestampMismatch,
}
impl From<DiscrepancyKind> for StoredDiscrepancy {
    fn from(value: DiscrepancyKind) -> Self {
        match value {
            DiscrepancyKind::AcceptedAfterTerminal => Self::AcceptedAfterTerminal,
            DiscrepancyKind::TerminalStatusMismatch => Self::TerminalStatusMismatch,
            DiscrepancyKind::ProviderEventReferenceMismatch => Self::ProviderEventReferenceMismatch,
            DiscrepancyKind::SettlementReferenceMismatch => Self::SettlementReferenceMismatch,
            DiscrepancyKind::FailureCodeMismatch => Self::FailureCodeMismatch,
            DiscrepancyKind::TerminalTimestampMismatch => Self::TerminalTimestampMismatch,
        }
    }
}
impl From<StoredDiscrepancy> for DiscrepancyKind {
    fn from(value: StoredDiscrepancy) -> Self {
        match value {
            StoredDiscrepancy::AcceptedAfterTerminal => Self::AcceptedAfterTerminal,
            StoredDiscrepancy::TerminalStatusMismatch => Self::TerminalStatusMismatch,
            StoredDiscrepancy::ProviderEventReferenceMismatch => {
                Self::ProviderEventReferenceMismatch
            }
            StoredDiscrepancy::SettlementReferenceMismatch => Self::SettlementReferenceMismatch,
            StoredDiscrepancy::FailureCodeMismatch => Self::FailureCodeMismatch,
            StoredDiscrepancy::TerminalTimestampMismatch => Self::TerminalTimestampMismatch,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconciliationRecord {
    case_id: String,
    disbursement_id: String,
    provider_request_reference: String,
    outcome_kind: OutcomeKind,
    terminal_status: Option<StoredStatus>,
    discrepancy_kind: Option<StoredDiscrepancy>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditRecord {
    event_id: String,
    stream_id: String,
    sequence: String,
    previous_digest: String,
    actor_id: String,
    correlation_id: Option<String>,
    causation_id: Option<String>,
    action: String,
    resource: String,
    effective_at_unix_ms: String,
    recorded_at_unix_ms: String,
    reconciliation: ReconciliationRecord,
    digest: String,
}

fn number_u64(s: &str) -> Result<u64, AuditCodecError> {
    if s.is_empty() || (s.len() > 1 && s.starts_with('0')) || !s.bytes().all(|c| c.is_ascii_digit())
    {
        return Err(AuditCodecError::InvalidRecord(
            "noncanonical unsigned integer".to_owned(),
        ));
    }
    s.parse()
        .map_err(|_| AuditCodecError::InvalidRecord("unsigned integer overflow".to_owned()))
}
fn number_i64(s: &str) -> Result<i64, AuditCodecError> {
    if s.is_empty()
        || (s.len() > 1 && s.starts_with('0'))
        || s.starts_with('+')
        || s.starts_with("-0")
        || s == "-"
        || !s
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_digit() || (i == 0 && b == b'-'))
    {
        return Err(AuditCodecError::InvalidRecord(
            "noncanonical timestamp".to_owned(),
        ));
    }
    s.parse()
        .map_err(|_| AuditCodecError::InvalidRecord("timestamp overflow".to_owned()))
}
fn digest_from_hex(hex: &str) -> Result<AuditDigest, AuditCodecError> {
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(AuditCodecError::InvalidRecord(
            "digest must be 64 lowercase hex digits".to_owned(),
        ));
    }
    let mut result = [0u8; 32];
    for (i, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| AuditCodecError::InvalidRecord("invalid hex digest".to_owned()))?;
    }
    Ok(AuditDigest(result))
}

fn checked_payload(
    value: ReconciliationRecord,
) -> Result<ReconciliationAuditPayload, AuditCodecError> {
    let valid = matches!(
        (
            value.outcome_kind,
            value.terminal_status,
            value.discrepancy_kind
        ),
        (OutcomeKind::PendingAgreement, None, None)
            | (OutcomeKind::Discrepancy, None, Some(_))
            | (
                OutcomeKind::ProviderAhead,
                Some(StoredStatus::Settled | StoredStatus::Failed),
                None
            )
            | (
                OutcomeKind::TerminalAgreement,
                Some(StoredStatus::Settled | StoredStatus::Failed),
                None
            )
    );
    if !valid {
        return Err(AuditCodecError::InvalidRecord(
            "inconsistent reconciliation outcome shape".to_owned(),
        ));
    }
    Ok(ReconciliationAuditPayload {
        case_id: ReconciliationCaseId::new(value.case_id)
            .map_err(|e| AuditCodecError::InvalidRecord(e.to_string()))?,
        disbursement_id: DisbursementId::new(value.disbursement_id)
            .map_err(|e| AuditCodecError::InvalidRecord(e.to_string()))?,
        provider_request_reference: ProviderRequestReference::new(value.provider_request_reference)
            .map_err(|e| AuditCodecError::InvalidRecord(e.to_string()))?,
        outcome_kind: value.outcome_kind.into(),
        terminal_status: value.terminal_status.map(Into::into),
        discrepancy_kind: value.discrepancy_kind.map(Into::into),
    })
}

fn record_for(event: &AuditEvent) -> Result<AuditRecord, AuditCodecError> {
    if !event.verify_digest() {
        return Err(AuditCodecError::DigestMismatch);
    }
    let AuditPayload::Reconciliation(ref p) = event.payload;
    let record = AuditRecord {
        event_id: event.position.event_id.as_str().to_owned(),
        stream_id: event.position.stream_id.as_str().to_owned(),
        sequence: event.position.sequence.to_string(),
        previous_digest: event.position.previous_digest.to_hex(),
        actor_id: event.attribution.actor_id.as_str().to_owned(),
        correlation_id: event
            .attribution
            .correlation_id
            .as_ref()
            .map(|x| x.as_str().to_owned()),
        causation_id: event
            .attribution
            .causation_id
            .as_ref()
            .map(|x| x.as_str().to_owned()),
        action: event.action.as_str().to_owned(),
        resource: event.resource.as_str().to_owned(),
        effective_at_unix_ms: event.timing.effective_at_unix_ms.to_string(),
        recorded_at_unix_ms: event.timing.recorded_at_unix_ms.to_string(),
        reconciliation: ReconciliationRecord {
            case_id: p.case_id.as_str().to_owned(),
            disbursement_id: p.disbursement_id.as_str().to_owned(),
            provider_request_reference: p.provider_request_reference.as_str().to_owned(),
            outcome_kind: p.outcome_kind.into(),
            terminal_status: p.terminal_status.map(Into::into),
            discrepancy_kind: p.discrepancy_kind.map(Into::into),
        },
        digest: event.digest.to_hex(),
    };
    Ok(record)
}

pub fn encode_audit_event(event: &AuditEvent) -> Result<Vec<u8>, AuditCodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: SCHEMA_VERSION,
        record_type: RECORD_TYPE.to_owned(),
        payload: record_for(event)?,
    })
    .map_err(|e| AuditCodecError::InvalidRecord(e.to_string()))?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(AuditCodecError::InvalidRecord(
            "audit record exceeds size limit".to_owned(),
        ));
    }
    if decode_audit_event(&bytes)? != *event {
        return Err(AuditCodecError::InvalidRecord(
            "event cannot be faithfully reconstituted".to_owned(),
        ));
    }
    Ok(bytes)
}

pub fn decode_audit_event(bytes: &[u8]) -> Result<AuditEvent, AuditCodecError> {
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(AuditCodecError::InvalidRecord(
            "audit record exceeds size limit".to_owned(),
        ));
    }
    let envelope: Envelope<AuditRecord> =
        serde_json::from_slice(bytes).map_err(|e| AuditCodecError::InvalidRecord(e.to_string()))?;
    if envelope.schema_version != SCHEMA_VERSION {
        return Err(AuditCodecError::UnsupportedVersion(envelope.schema_version));
    }
    if envelope.record_type != RECORD_TYPE {
        return Err(AuditCodecError::InvalidRecord(
            "unsupported audit record type".to_owned(),
        ));
    }
    let r = envelope.payload;
    let payload = checked_payload(r.reconciliation)?;
    // Reconciliation events originate from the canonical domain adapter.
    if r.action != "reconciliation.observed"
        || r.resource != format!("reconciliation:{}", payload.case_id.as_str())
    {
        return Err(AuditCodecError::InvalidRecord(
            "noncanonical reconciliation action/resource".to_owned(),
        ));
    }
    let original_digest = digest_from_hex(&r.digest)?;
    let event = AuditEvent::new(
        AuditPosition::new(
            AuditEventId::new(r.event_id)?,
            AuditStreamId::new(r.stream_id)?,
            number_u64(&r.sequence)?,
            digest_from_hex(&r.previous_digest)?,
        ),
        AuditAttribution::new(
            ActorId::new(r.actor_id)?,
            r.correlation_id.map(CorrelationId::new).transpose()?,
            r.causation_id.map(CausationId::new).transpose()?,
        ),
        AuditAction::new(r.action)?,
        AuditResource::new(r.resource)?,
        AuditTiming::new(
            number_i64(&r.effective_at_unix_ms)?,
            number_i64(&r.recorded_at_unix_ms)?,
        )?,
        AuditPayload::Reconciliation(payload),
    )?;
    if event.digest() != original_digest {
        return Err(AuditCodecError::DigestMismatch);
    }
    Ok(event)
}

/// All accepted records are processed in their original stream order.
/// This ensures hash/digest/sequence parity but does NOT authenticate
/// the external event stream or guarantee completeness against omission.
pub fn replay_audit_events<'a>(
    facts: impl IntoIterator<Item = &'a [u8]>,
) -> Result<AuditLog, AuditCodecError> {
    let mut log = AuditLog::new();
    let mut streams = BTreeSet::new();
    for fact in facts {
        let event = decode_audit_event(fact)?;
        streams.insert(event.stream_id().clone());
        log.append(event)?;
    }
    for stream in streams {
        log.verify_stream(&stream)?;
    }
    Ok(log)
}

/// A final digest and sequence that the caller obtained through an
/// **independent, trusted** authority. This type does not authenticate the
/// authority or store/sign the anchor: the caller must establish that trust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditStreamAnchor {
    stream_id: AuditStreamId,
    final_sequence: u64,
    final_digest: AuditDigest,
}
impl AuditStreamAnchor {
    pub fn new(
        stream_id: AuditStreamId,
        final_sequence: u64,
        final_digest: AuditDigest,
    ) -> Result<Self, AuditCodecError> {
        if final_sequence == 0 || final_digest == AuditDigest::GENESIS {
            return Err(AuditCodecError::InvalidRecord(
                "a nonempty audit anchor requires a nonzero sequence and digest".to_owned(),
            ));
        }
        Ok(Self {
            stream_id,
            final_sequence,
            final_digest,
        })
    }

    #[must_use]
    pub const fn stream_id(&self) -> &AuditStreamId {
        &self.stream_id
    }
    #[must_use]
    pub const fn final_sequence(&self) -> u64 {
        self.final_sequence
    }
    #[must_use]
    pub const fn final_digest(&self) -> AuditDigest {
        self.final_digest
    }
}

/// Require *all and only* the independently anchored nonempty streams,
/// checking both the sequence and terminal digest. An internally consistent
/// rewritten or truncated stream cannot match an unchanged trusted anchor.
///
/// WARNING: A malicious/untrusted caller can simply supply an attacker-owned
/// matching anchor. Upstream root anchoring, tenant binding, signature checks
/// and durable atomic association are future requirements, not provided here.
pub fn replay_audit_events_anchored<'a>(
    facts: impl IntoIterator<Item = &'a [u8]>,
    anchors: &[AuditStreamAnchor],
) -> Result<AuditLog, AuditCodecError> {
    if anchors.is_empty() {
        return Err(AuditCodecError::InvalidRecord(
            "trusted audit stream anchors cannot be empty".to_owned(),
        ));
    }
    let log = replay_audit_events(facts)?;
    if log.stream_count() != anchors.len() {
        return Err(AuditCodecError::InvalidRecord(
            "provided stream count differs from trusted anchor set".to_owned(),
        ));
    }
    let mut seen = BTreeSet::new();
    for anchor in anchors {
        if !seen.insert(anchor.stream_id.clone()) {
            return Err(AuditCodecError::InvalidRecord(
                "duplicated trusted stream anchor".to_owned(),
            ));
        }
        let Some(stream) = log.stream(&anchor.stream_id) else {
            return Err(AuditCodecError::InvalidRecord(
                "trusted audit stream is missing".to_owned(),
            ));
        };
        let Some(last) = stream.last() else {
            return Err(AuditCodecError::InvalidRecord(
                "trusted audit stream is empty".to_owned(),
            ));
        };
        if last.sequence() != anchor.final_sequence || last.digest() != anchor.final_digest {
            return Err(AuditCodecError::InvalidRecord(
                "audit stream differs from independently trusted anchor".to_owned(),
            ));
        }
    }
    Ok(log)
}
