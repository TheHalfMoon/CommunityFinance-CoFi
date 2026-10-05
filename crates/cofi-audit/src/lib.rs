use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_disbursements::{
    DisbursementId, DisbursementStatus, ProviderRequestReference, TerminalKind,
};
use cofi_reconciliation::{
    DiscrepancyKind, ReconciliationCase, ReconciliationCaseId, ReconciliationOutcome,
};
use sha2::{Digest, Sha256};

macro_rules! audit_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, AuditError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(AuditError::EmptyIdentifier($label));
                }
                Ok(Self(value))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}
audit_id!(AuditEventId, "audit_event_id");
audit_id!(AuditStreamId, "audit_stream_id");
audit_id!(ActorId, "actor_id");
audit_id!(CorrelationId, "correlation_id");
audit_id!(CausationId, "causation_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuditDigest([u8; 32]);

impl AuditDigest {
    pub const GENESIS: Self = Self([0; 32]);

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut value = String::with_capacity(64);
        for byte in self.0 {
            use std::fmt::Write as _;
            let _ = write!(&mut value, "{byte:02x}");
        }
        value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditAction(String);
impl AuditAction {
    pub fn new(value: impl Into<String>) -> Result<Self, AuditError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AuditError::EmptyIdentifier("audit_action"));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditResource(String);

impl AuditResource {
    pub fn new(value: impl Into<String>) -> Result<Self, AuditError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AuditError::EmptyIdentifier("audit_resource"));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditPosition {
    event_id: AuditEventId,
    stream_id: AuditStreamId,
    sequence: u64,
    previous_digest: AuditDigest,
}

impl AuditPosition {
    #[must_use]
    pub const fn new(
        event_id: AuditEventId,
        stream_id: AuditStreamId,
        sequence: u64,
        previous_digest: AuditDigest,
    ) -> Self {
        Self {
            event_id,
            stream_id,
            sequence,
            previous_digest,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditAttribution {
    actor_id: ActorId,
    correlation_id: Option<CorrelationId>,
    causation_id: Option<CausationId>,
}
impl AuditAttribution {
    #[must_use]
    pub const fn new(
        actor_id: ActorId,
        correlation_id: Option<CorrelationId>,
        causation_id: Option<CausationId>,
    ) -> Self {
        Self {
            actor_id,
            correlation_id,
            causation_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditTiming {
    effective_at_unix_ms: i64,
    recorded_at_unix_ms: i64,
}

impl AuditTiming {
    pub fn new(effective_at_unix_ms: i64, recorded_at_unix_ms: i64) -> Result<Self, AuditError> {
        if effective_at_unix_ms < 0 || recorded_at_unix_ms < 0 {
            return Err(AuditError::InvalidTimestamp);
        }
        if effective_at_unix_ms > recorded_at_unix_ms {
            return Err(AuditError::RecordedBeforeEffective);
        }
        Ok(Self {
            effective_at_unix_ms,
            recorded_at_unix_ms,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationAuditOutcomeKind {
    PendingAgreement,
    ProviderAhead,
    TerminalAgreement,
    Discrepancy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationAuditPayload {
    case_id: ReconciliationCaseId,
    disbursement_id: DisbursementId,
    provider_request_reference: ProviderRequestReference,
    outcome_kind: ReconciliationAuditOutcomeKind,
    terminal_status: Option<DisbursementStatus>,
    discrepancy_kind: Option<DiscrepancyKind>,
}

impl ReconciliationAuditPayload {
    #[must_use]
    pub const fn case_id(&self) -> &ReconciliationCaseId {
        &self.case_id
    }
    #[must_use]
    pub const fn disbursement_id(&self) -> &DisbursementId {
        &self.disbursement_id
    }
    #[must_use]
    pub const fn provider_request_reference(&self) -> &ProviderRequestReference {
        &self.provider_request_reference
    }
    #[must_use]
    pub const fn outcome_kind(&self) -> ReconciliationAuditOutcomeKind {
        self.outcome_kind
    }
    #[must_use]
    pub const fn terminal_status(&self) -> Option<DisbursementStatus> {
        self.terminal_status
    }
    #[must_use]
    pub const fn discrepancy_kind(&self) -> Option<DiscrepancyKind> {
        self.discrepancy_kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditPayload {
    Reconciliation(ReconciliationAuditPayload),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    position: AuditPosition,
    attribution: AuditAttribution,
    action: AuditAction,
    resource: AuditResource,
    timing: AuditTiming,
    payload: AuditPayload,
    digest: AuditDigest,
}

impl AuditEvent {
    pub fn new(
        position: AuditPosition,
        attribution: AuditAttribution,
        action: AuditAction,
        resource: AuditResource,
        timing: AuditTiming,
        payload: AuditPayload,
    ) -> Result<Self, AuditError> {
        if position.sequence == 0 {
            return Err(AuditError::InvalidSequenceZero);
        }
        let mut event = Self {
            position,
            attribution,
            action,
            resource,
            timing,
            payload,
            digest: AuditDigest::GENESIS,
        };
        event.digest = event.compute_digest();
        Ok(event)
    }

    #[must_use]
    pub const fn id(&self) -> &AuditEventId {
        &self.position.event_id
    }
    #[must_use]
    pub const fn stream_id(&self) -> &AuditStreamId {
        &self.position.stream_id
    }
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.position.sequence
    }
    #[must_use]
    pub const fn previous_digest(&self) -> AuditDigest {
        self.position.previous_digest
    }
    #[must_use]
    pub const fn digest(&self) -> AuditDigest {
        self.digest
    }
    #[must_use]
    pub const fn payload(&self) -> &AuditPayload {
        &self.payload
    }

    #[must_use]
    pub fn verify_digest(&self) -> bool {
        self.digest == self.compute_digest()
    }

    fn compute_digest(&self) -> AuditDigest {
        let mut canonical = Vec::new();
        push_field(&mut canonical, b"schema", b"cofi-audit-v1");
        push_field(
            &mut canonical,
            b"event_id",
            self.position.event_id.as_str().as_bytes(),
        );
        push_field(
            &mut canonical,
            b"stream_id",
            self.position.stream_id.as_str().as_bytes(),
        );
        push_field(
            &mut canonical,
            b"sequence",
            &self.position.sequence.to_be_bytes(),
        );
        push_field(
            &mut canonical,
            b"previous_digest",
            self.position.previous_digest.as_bytes(),
        );
        push_field(
            &mut canonical,
            b"actor_id",
            self.attribution.actor_id.as_str().as_bytes(),
        );
        push_optional_id(
            &mut canonical,
            b"correlation_id",
            self.attribution
                .correlation_id
                .as_ref()
                .map(CorrelationId::as_str),
        );
        push_optional_id(
            &mut canonical,
            b"causation_id",
            self.attribution
                .causation_id
                .as_ref()
                .map(CausationId::as_str),
        );
        push_field(&mut canonical, b"action", self.action.as_str().as_bytes());
        push_field(
            &mut canonical,
            b"resource",
            self.resource.as_str().as_bytes(),
        );
        push_field(
            &mut canonical,
            b"effective_at",
            &self.timing.effective_at_unix_ms.to_be_bytes(),
        );
        push_field(
            &mut canonical,
            b"recorded_at",
            &self.timing.recorded_at_unix_ms.to_be_bytes(),
        );
        encode_payload(&mut canonical, &self.payload);
        let digest: [u8; 32] = Sha256::digest(canonical).into();
        AuditDigest(digest)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendOutcome {
    Appended,
    Replayed,
}

#[derive(Debug, Clone, Default)]
pub struct AuditLog {
    streams: BTreeMap<AuditStreamId, Vec<AuditEvent>>,
    events: BTreeMap<AuditEventId, AuditEvent>,
}

impl AuditLog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn event(&self, id: &AuditEventId) -> Option<&AuditEvent> {
        self.events.get(id)
    }

    #[must_use]
    pub fn stream(&self, id: &AuditStreamId) -> Option<&[AuditEvent]> {
        self.streams.get(id).map(Vec::as_slice)
    }

    #[must_use]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }

    #[must_use]
    pub fn tail_digest(&self, stream_id: &AuditStreamId) -> AuditDigest {
        self.streams
            .get(stream_id)
            .and_then(|events| events.last())
            .map_or(AuditDigest::GENESIS, AuditEvent::digest)
    }

    pub fn append(&mut self, event: AuditEvent) -> Result<AppendOutcome, AuditError> {
        if !event.verify_digest() {
            return Err(AuditError::DigestMismatch);
        }
        if let Some(existing) = self.events.get(event.id()) {
            return if existing == &event {
                Ok(AppendOutcome::Replayed)
            } else {
                Err(AuditError::EventIdConflict(event.id().clone()))
            };
        }

        let stream = self.streams.get(event.stream_id());
        let expected_sequence = stream.map_or(1_u64, |events| events.len() as u64 + 1);
        if event.sequence() != expected_sequence {
            return Err(AuditError::SequenceMismatch {
                expected: expected_sequence,
                actual: event.sequence(),
            });
        }
        let expected_previous = stream
            .and_then(|events| events.last())
            .map_or(AuditDigest::GENESIS, AuditEvent::digest);
        if event.previous_digest() != expected_previous {
            return Err(AuditError::PreviousDigestMismatch);
        }

        self.events.insert(event.id().clone(), event.clone());
        self.streams
            .entry(event.stream_id().clone())
            .or_default()
            .push(event);
        Ok(AppendOutcome::Appended)
    }

    pub fn verify_stream(&self, stream_id: &AuditStreamId) -> Result<(), AuditError> {
        let events = self
            .streams
            .get(stream_id)
            .ok_or_else(|| AuditError::UnknownStream(stream_id.clone()))?;
        let mut previous = AuditDigest::GENESIS;
        for (index, event) in events.iter().enumerate() {
            let expected = index as u64 + 1;
            if event.sequence() != expected {
                return Err(AuditError::SequenceMismatch {
                    expected,
                    actual: event.sequence(),
                });
            }
            if event.previous_digest() != previous {
                return Err(AuditError::PreviousDigestMismatch);
            }
            if !event.verify_digest() {
                return Err(AuditError::DigestMismatch);
            }
            previous = event.digest();
        }
        Ok(())
    }
}
pub fn reconciliation_audit_event(
    position: AuditPosition,
    attribution: AuditAttribution,
    timing: AuditTiming,
    case: &ReconciliationCase,
    outcome: &ReconciliationOutcome,
) -> Result<AuditEvent, AuditError> {
    let (outcome_case_id, outcome_kind, terminal_status, discrepancy_kind) = match outcome {
        ReconciliationOutcome::PendingAgreement { case_id } => (
            case_id,
            ReconciliationAuditOutcomeKind::PendingAgreement,
            None,
            None,
        ),
        ReconciliationOutcome::ProviderAhead {
            case_id,
            proposed_terminal_event,
        } => {
            let terminal_status = match proposed_terminal_event.kind() {
                TerminalKind::Settled { .. } => DisbursementStatus::Settled,
                TerminalKind::Failed { .. } => DisbursementStatus::Failed,
            };
            (
                case_id,
                ReconciliationAuditOutcomeKind::ProviderAhead,
                Some(terminal_status),
                None,
            )
        }
        ReconciliationOutcome::TerminalAgreement { case_id, status } => (
            case_id,
            ReconciliationAuditOutcomeKind::TerminalAgreement,
            Some(*status),
            None,
        ),
        ReconciliationOutcome::Discrepancy { case_id, kind } => (
            case_id,
            ReconciliationAuditOutcomeKind::Discrepancy,
            None,
            Some(*kind),
        ),
    };
    if outcome_case_id != case.id() {
        return Err(AuditError::ReconciliationCaseMismatch);
    }

    let payload = ReconciliationAuditPayload {
        case_id: case.id().clone(),
        disbursement_id: case.disbursement_id().clone(),
        provider_request_reference: case.provider_request_reference().clone(),
        outcome_kind,
        terminal_status,
        discrepancy_kind,
    };
    let action = AuditAction::new("reconciliation.observed")?;
    let resource = AuditResource::new(format!("reconciliation:{}", case.id().as_str()))?;
    AuditEvent::new(
        position,
        attribution,
        action,
        resource,
        timing,
        AuditPayload::Reconciliation(payload),
    )
}

fn push_field(target: &mut Vec<u8>, name: &[u8], value: &[u8]) {
    target.extend_from_slice(&(name.len() as u32).to_be_bytes());
    target.extend_from_slice(name);
    target.extend_from_slice(&(value.len() as u64).to_be_bytes());
    target.extend_from_slice(value);
}

fn push_optional_id(target: &mut Vec<u8>, name: &[u8], value: Option<&str>) {
    match value {
        Some(value) => {
            push_field(target, name, b"present");
            push_field(target, b"optional_value", value.as_bytes());
        }
        None => push_field(target, name, b"absent"),
    }
}

fn encode_payload(target: &mut Vec<u8>, payload: &AuditPayload) {
    match payload {
        AuditPayload::Reconciliation(payload) => {
            push_field(target, b"payload_kind", b"reconciliation");
            push_field(target, b"case_id", payload.case_id.as_str().as_bytes());
            push_field(
                target,
                b"disbursement_id",
                payload.disbursement_id.as_str().as_bytes(),
            );
            push_field(
                target,
                b"provider_request_reference",
                payload.provider_request_reference.as_str().as_bytes(),
            );
            push_field(
                target,
                b"outcome_kind",
                reconciliation_outcome_code(payload.outcome_kind),
            );
            if let Some(status) = payload.terminal_status {
                push_field(target, b"terminal_status", disbursement_status_code(status));
            } else {
                push_field(target, b"terminal_status", b"none");
            }
            if let Some(kind) = payload.discrepancy_kind {
                push_field(target, b"discrepancy_kind", discrepancy_code(kind));
            } else {
                push_field(target, b"discrepancy_kind", b"none");
            }
        }
    }
}

const fn reconciliation_outcome_code(kind: ReconciliationAuditOutcomeKind) -> &'static [u8] {
    match kind {
        ReconciliationAuditOutcomeKind::PendingAgreement => b"pending_agreement",
        ReconciliationAuditOutcomeKind::ProviderAhead => b"provider_ahead",
        ReconciliationAuditOutcomeKind::TerminalAgreement => b"terminal_agreement",
        ReconciliationAuditOutcomeKind::Discrepancy => b"discrepancy",
    }
}

const fn disbursement_status_code(status: DisbursementStatus) -> &'static [u8] {
    match status {
        DisbursementStatus::Ready => b"ready",
        DisbursementStatus::Submitted => b"submitted",
        DisbursementStatus::Settled => b"settled",
        DisbursementStatus::Failed => b"failed",
    }
}

const fn discrepancy_code(kind: DiscrepancyKind) -> &'static [u8] {
    match kind {
        DiscrepancyKind::AcceptedAfterTerminal => b"accepted_after_terminal",
        DiscrepancyKind::TerminalStatusMismatch => b"terminal_status_mismatch",
        DiscrepancyKind::ProviderEventReferenceMismatch => b"provider_event_reference_mismatch",
        DiscrepancyKind::SettlementReferenceMismatch => b"settlement_reference_mismatch",
        DiscrepancyKind::FailureCodeMismatch => b"failure_code_mismatch",
        DiscrepancyKind::TerminalTimestampMismatch => b"terminal_timestamp_mismatch",
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditError {
    EmptyIdentifier(&'static str),
    InvalidTimestamp,
    RecordedBeforeEffective,
    InvalidSequenceZero,
    EventIdConflict(AuditEventId),
    SequenceMismatch { expected: u64, actual: u64 },
    PreviousDigestMismatch,
    DigestMismatch,
    UnknownStream(AuditStreamId),
    ReconciliationCaseMismatch,
}

impl Display for AuditError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(field) => write!(formatter, "{field} must not be empty"),
            Self::InvalidTimestamp => write!(formatter, "audit timestamps must be non-negative"),
            Self::RecordedBeforeEffective => {
                write!(
                    formatter,
                    "recorded timestamp cannot precede effective timestamp"
                )
            }
            Self::InvalidSequenceZero => write!(formatter, "audit sequence must start at one"),
            Self::EventIdConflict(id) => {
                write!(formatter, "audit event id conflict: {}", id.as_str())
            }
            Self::SequenceMismatch { expected, actual } => {
                write!(
                    formatter,
                    "audit sequence mismatch: expected {expected}, got {actual}"
                )
            }
            Self::PreviousDigestMismatch => write!(formatter, "audit previous digest mismatch"),
            Self::DigestMismatch => write!(formatter, "audit event digest mismatch"),
            Self::UnknownStream(id) => write!(formatter, "unknown audit stream: {}", id.as_str()),
            Self::ReconciliationCaseMismatch => {
                write!(
                    formatter,
                    "reconciliation outcome is bound to a different case"
                )
            }
        }
    }
}

impl Error for AuditError {}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod internal_tests {
    use super::*;

    #[test]
    fn verifier_detects_tampered_sequence_previous_digest_and_digest() {
        let mut canonical = AuditLog::new();
        canonical
            .append(test_event(1, AuditDigest::GENESIS))
            .unwrap();
        let stream_id = AuditStreamId::new("stream-1").unwrap();

        let mut sequence_tampered = canonical.clone();
        sequence_tampered.streams.get_mut(&stream_id).unwrap()[0]
            .position
            .sequence = 2;
        assert!(matches!(
            sequence_tampered.verify_stream(&stream_id),
            Err(AuditError::SequenceMismatch {
                expected: 1,
                actual: 2
            })
        ));

        let mut previous_tampered = canonical.clone();
        previous_tampered.streams.get_mut(&stream_id).unwrap()[0]
            .position
            .previous_digest = AuditDigest([1; 32]);
        assert_eq!(
            previous_tampered.verify_stream(&stream_id).unwrap_err(),
            AuditError::PreviousDigestMismatch
        );

        let mut digest_tampered = canonical;
        digest_tampered.streams.get_mut(&stream_id).unwrap()[0].digest = AuditDigest([2; 32]);
        assert_eq!(
            digest_tampered.verify_stream(&stream_id).unwrap_err(),
            AuditError::DigestMismatch
        );
    }

    fn test_event(sequence: u64, previous_digest: AuditDigest) -> AuditEvent {
        AuditEvent::new(
            AuditPosition::new(
                AuditEventId::new(format!("event-{sequence}")).unwrap(),
                AuditStreamId::new("stream-1").unwrap(),
                sequence,
                previous_digest,
            ),
            AuditAttribution::new(ActorId::new("tester").unwrap(), None, None),
            AuditAction::new("test.action").unwrap(),
            AuditResource::new("test:resource").unwrap(),
            AuditTiming::new(10, 10).unwrap(),
            AuditPayload::Reconciliation(ReconciliationAuditPayload {
                case_id: ReconciliationCaseId::new("case-1").unwrap(),
                disbursement_id: DisbursementId::new("disb-1").unwrap(),
                provider_request_reference: ProviderRequestReference::new("req-1").unwrap(),
                outcome_kind: ReconciliationAuditOutcomeKind::PendingAgreement,
                terminal_status: None,
                discrepancy_kind: None,
            }),
        )
        .unwrap()
    }
}
