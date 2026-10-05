#![allow(clippy::unwrap_used)]
use cofi_audit::{
    ActorId, AppendOutcome, AuditAttribution, AuditDigest, AuditError, AuditEventId, AuditLog,
    AuditPayload, AuditPosition, AuditStreamId, AuditTiming, CausationId, CorrelationId,
    ReconciliationAuditOutcomeKind, reconciliation_audit_event,
};
use cofi_disbursements::{
    DisbursementEventId, DisbursementId, DisbursementStatus, DisbursementTerminalEvent,
    FailureCode, ProviderEventReference, ProviderRequestReference, ProviderSettlementReference,
};
use cofi_reconciliation::{
    DiscrepancyKind, ReconciliationCase, ReconciliationCaseId, ReconciliationOutcome,
};

fn case() -> ReconciliationCase {
    ReconciliationCase::new(
        ReconciliationCaseId::new("case-1").unwrap(),
        DisbursementId::new("disb-1").unwrap(),
        ProviderRequestReference::new("request-1").unwrap(),
        50,
    )
}

fn position(id: &str, sequence: u64, previous_digest: AuditDigest) -> AuditPosition {
    AuditPosition::new(
        AuditEventId::new(id).unwrap(),
        AuditStreamId::new("reconciliation-stream").unwrap(),
        sequence,
        previous_digest,
    )
}

fn attribution(actor: &str) -> AuditAttribution {
    AuditAttribution::new(
        ActorId::new(actor).unwrap(),
        Some(CorrelationId::new("corr-1").unwrap()),
        Some(CausationId::new("cause-1").unwrap()),
    )
}

fn pending(case: &ReconciliationCase) -> ReconciliationOutcome {
    ReconciliationOutcome::PendingAgreement {
        case_id: case.id().clone(),
    }
}

fn audit_event(
    id: &str,
    sequence: u64,
    previous_digest: AuditDigest,
    actor: &str,
    outcome: &ReconciliationOutcome,
) -> cofi_audit::AuditEvent {
    reconciliation_audit_event(
        position(id, sequence, previous_digest),
        attribution(actor),
        AuditTiming::new(50, 60).unwrap(),
        &case(),
        outcome,
    )
    .unwrap()
}

#[test]
fn identifiers_and_timing_fail_closed() {
    assert!(AuditEventId::new("  ").is_err());
    assert!(AuditStreamId::new("").is_err());
    assert!(ActorId::new(" ").is_err());
    assert_eq!(
        AuditTiming::new(-1, 0).unwrap_err(),
        AuditError::InvalidTimestamp
    );
    assert_eq!(
        AuditTiming::new(10, 9).unwrap_err(),
        AuditError::RecordedBeforeEffective
    );
}

#[test]
fn identical_events_have_identical_digest_and_field_boundaries_are_unambiguous() {
    let c = case();
    let outcome = pending(&c);
    let first = audit_event("event-1", 1, AuditDigest::GENESIS, "actor-a", &outcome);
    let second = audit_event("event-1", 1, AuditDigest::GENESIS, "actor-a", &outcome);
    let changed = audit_event("event-1", 1, AuditDigest::GENESIS, "actor-aa", &outcome);
    let boundary_left = reconciliation_audit_event(
        position("event-1", 1, AuditDigest::GENESIS),
        AuditAttribution::new(
            ActorId::new("ab").unwrap(),
            Some(CorrelationId::new("c").unwrap()),
            None,
        ),
        AuditTiming::new(50, 60).unwrap(),
        &c,
        &outcome,
    )
    .unwrap();
    let boundary_right = reconciliation_audit_event(
        position("event-1", 1, AuditDigest::GENESIS),
        AuditAttribution::new(
            ActorId::new("a").unwrap(),
            Some(CorrelationId::new("bc").unwrap()),
            None,
        ),
        AuditTiming::new(50, 60).unwrap(),
        &c,
        &outcome,
    )
    .unwrap();

    assert_eq!(first.digest(), second.digest());
    assert_ne!(first.digest(), changed.digest());
    assert_ne!(boundary_left.digest(), boundary_right.digest());
    assert!(first.verify_digest());
    assert_eq!(first.digest().to_hex().len(), 64);
}

#[test]
fn append_chain_verifies_and_exact_replay_has_zero_duplicate_effect() {
    let c = case();
    let mut log = AuditLog::new();
    let first = audit_event("event-1", 1, AuditDigest::GENESIS, "actor-a", &pending(&c));
    assert_eq!(log.append(first.clone()).unwrap(), AppendOutcome::Appended);
    assert_eq!(log.append(first).unwrap(), AppendOutcome::Replayed);
    assert_eq!(log.event_count(), 1);
    assert_eq!(log.stream_count(), 1);

    let second = audit_event(
        "event-2",
        2,
        log.tail_digest(&AuditStreamId::new("reconciliation-stream").unwrap()),
        "actor-a",
        &pending(&c),
    );
    assert_eq!(log.append(second).unwrap(), AppendOutcome::Appended);
    assert_eq!(log.event_count(), 2);
    log.verify_stream(&AuditStreamId::new("reconciliation-stream").unwrap())
        .unwrap();
}

#[test]
fn conflicting_event_id_fails_closed_without_advancing_chain() {
    let c = case();
    let mut log = AuditLog::new();
    let first = audit_event("event-1", 1, AuditDigest::GENESIS, "actor-a", &pending(&c));
    log.append(first).unwrap();
    let tail = log.tail_digest(&AuditStreamId::new("reconciliation-stream").unwrap());
    let conflict = audit_event("event-1", 1, AuditDigest::GENESIS, "actor-b", &pending(&c));

    assert!(matches!(
        log.append(conflict),
        Err(AuditError::EventIdConflict(_))
    ));
    assert_eq!(log.event_count(), 1);
    assert_eq!(
        log.tail_digest(&AuditStreamId::new("reconciliation-stream").unwrap()),
        tail
    );
}

#[test]
fn sequence_gap_and_previous_digest_mismatch_do_not_mutate() {
    let c = case();
    let mut log = AuditLog::new();
    let gap = audit_event(
        "event-gap",
        2,
        AuditDigest::GENESIS,
        "actor-a",
        &pending(&c),
    );
    assert_eq!(
        log.append(gap).unwrap_err(),
        AuditError::SequenceMismatch {
            expected: 1,
            actual: 2
        }
    );
    assert_eq!(log.event_count(), 0);

    let wrong_previous = AuditDigest::GENESIS;
    let first = audit_event("event-1", 1, wrong_previous, "actor-a", &pending(&c));
    log.append(first).unwrap();
    let bad_second = audit_event("event-2", 2, AuditDigest::GENESIS, "actor-a", &pending(&c));
    assert_eq!(
        log.append(bad_second).unwrap_err(),
        AuditError::PreviousDigestMismatch
    );
    assert_eq!(log.event_count(), 1);
}

#[test]
fn reconciliation_adapter_maps_all_outcome_kinds() {
    let c = case();
    let provider_ahead = ReconciliationOutcome::ProviderAhead {
        case_id: c.id().clone(),
        proposed_terminal_event: DisbursementTerminalEvent::settled(
            DisbursementEventId::new("terminal-event").unwrap(),
            c.disbursement_id().clone(),
            ProviderEventReference::new("provider-event").unwrap(),
            ProviderSettlementReference::new("settlement").unwrap(),
            40,
        ),
    };
    let terminal = ReconciliationOutcome::TerminalAgreement {
        case_id: c.id().clone(),
        status: DisbursementStatus::Settled,
    };
    let discrepancy = ReconciliationOutcome::Discrepancy {
        case_id: c.id().clone(),
        kind: DiscrepancyKind::SettlementReferenceMismatch,
    };
    let outcomes = [
        (
            pending(&c),
            ReconciliationAuditOutcomeKind::PendingAgreement,
            None,
            None,
        ),
        (
            provider_ahead,
            ReconciliationAuditOutcomeKind::ProviderAhead,
            Some(DisbursementStatus::Settled),
            None,
        ),
        (
            terminal,
            ReconciliationAuditOutcomeKind::TerminalAgreement,
            Some(DisbursementStatus::Settled),
            None,
        ),
        (
            discrepancy,
            ReconciliationAuditOutcomeKind::Discrepancy,
            None,
            Some(DiscrepancyKind::SettlementReferenceMismatch),
        ),
    ];

    for (index, (outcome, expected_kind, expected_status, expected_discrepancy)) in
        outcomes.into_iter().enumerate()
    {
        let event = audit_event(
            &format!("event-{index}"),
            1,
            AuditDigest::GENESIS,
            "actor-a",
            &outcome,
        );
        let AuditPayload::Reconciliation(payload) = event.payload();
        assert_eq!(payload.case_id(), c.id());
        assert_eq!(payload.disbursement_id(), c.disbursement_id());
        assert_eq!(
            payload.provider_request_reference(),
            c.provider_request_reference()
        );
        assert_eq!(payload.outcome_kind(), expected_kind);
        assert_eq!(payload.terminal_status(), expected_status);
        assert_eq!(payload.discrepancy_kind(), expected_discrepancy);
    }
}

#[test]
fn provider_ahead_terminal_direction_changes_audit_digest() {
    let c = case();
    let settled = ReconciliationOutcome::ProviderAhead {
        case_id: c.id().clone(),
        proposed_terminal_event: DisbursementTerminalEvent::settled(
            DisbursementEventId::new("settled-event").unwrap(),
            c.disbursement_id().clone(),
            ProviderEventReference::new("provider-settled").unwrap(),
            ProviderSettlementReference::new("settlement-1").unwrap(),
            40,
        ),
    };
    let failed = ReconciliationOutcome::ProviderAhead {
        case_id: c.id().clone(),
        proposed_terminal_event: DisbursementTerminalEvent::failed(
            DisbursementEventId::new("failed-event").unwrap(),
            c.disbursement_id().clone(),
            ProviderEventReference::new("provider-failed").unwrap(),
            FailureCode::new("DECLINED").unwrap(),
            40,
        ),
    };
    let settled_event = audit_event("event-1", 1, AuditDigest::GENESIS, "actor-a", &settled);
    let failed_event = audit_event("event-1", 1, AuditDigest::GENESIS, "actor-a", &failed);
    assert_ne!(settled_event.digest(), failed_event.digest());
}

#[test]
fn reconciliation_case_mismatch_fails_closed() {
    let c = case();
    let outcome = ReconciliationOutcome::PendingAgreement {
        case_id: ReconciliationCaseId::new("different-case").unwrap(),
    };
    let result = reconciliation_audit_event(
        position("event-1", 1, AuditDigest::GENESIS),
        attribution("actor-a"),
        AuditTiming::new(50, 60).unwrap(),
        &c,
        &outcome,
    );
    assert_eq!(result.unwrap_err(), AuditError::ReconciliationCaseMismatch);
}

#[test]
fn unknown_stream_verification_fails_closed() {
    let log = AuditLog::new();
    let stream = AuditStreamId::new("unknown").unwrap();
    assert_eq!(
        log.verify_stream(&stream).unwrap_err(),
        AuditError::UnknownStream(stream)
    );
}
