#![allow(clippy::unwrap_used)]

use cofi_audit::codec::{decode_audit_event, encode_audit_event, replay_audit_events};
use cofi_audit::{
    ActorId, AuditAttribution, AuditDigest, AuditEvent, AuditEventId, AuditLog, AuditPayload,
    AuditPosition, AuditStreamId, AuditTiming, CausationId, CorrelationId,
    ReconciliationAuditOutcomeKind, reconciliation_audit_event,
};
use cofi_disbursements::{
    DisbursementEventId, DisbursementId, DisbursementStatus, DisbursementTerminalEvent,
    ProviderEventReference, ProviderRequestReference, ProviderSettlementReference,
};
use cofi_reconciliation::{
    DiscrepancyKind, ReconciliationCase, ReconciliationCaseId, ReconciliationOutcome,
};

fn case(id: &str) -> ReconciliationCase {
    ReconciliationCase::new(
        ReconciliationCaseId::new(id).unwrap(),
        DisbursementId::new("disb-1").unwrap(),
        ProviderRequestReference::new("req-1").unwrap(),
        55,
    )
}
fn event(
    id: &str,
    stream: &str,
    sequence: u64,
    previous: AuditDigest,
    actor: &str,
    outcome: ReconciliationOutcome,
) -> AuditEvent {
    let c = case("case-1");
    reconciliation_audit_event(
        AuditPosition::new(
            AuditEventId::new(id).unwrap(),
            AuditStreamId::new(stream).unwrap(),
            sequence,
            previous,
        ),
        AuditAttribution::new(
            ActorId::new(actor).unwrap(),
            Some(CorrelationId::new("corr-1").unwrap()),
            Some(CausationId::new("cause-1").unwrap()),
        ),
        AuditTiming::new(55, 60).unwrap(),
        &c,
        &outcome,
    )
    .unwrap()
}
fn pending() -> ReconciliationOutcome {
    ReconciliationOutcome::PendingAgreement {
        case_id: case("case-1").id().clone(),
    }
}

#[test]
fn exact_digest_event_and_multi_stream_replay_parity() {
    let first = event("e1", "s1", 1, AuditDigest::GENESIS, "actor-one", pending());
    let other = event("e2", "s2", 1, AuditDigest::GENESIS, "actor-two", pending());
    let second = event("e3", "s1", 2, first.digest(), "actor-one", pending());
    for item in [&first, &other, &second] {
        assert_eq!(
            decode_audit_event(&encode_audit_event(item).unwrap()).unwrap(),
            *item
        );
    }
    let records = [
        encode_audit_event(&first).unwrap(),
        encode_audit_event(&other).unwrap(),
        encode_audit_event(&second).unwrap(),
    ];
    let mut reference = AuditLog::new();
    reference.append(first.clone()).unwrap();
    reference.append(other.clone()).unwrap();
    reference.append(second.clone()).unwrap();
    let mut restored = replay_audit_events(records.iter().map(Vec::as_slice)).unwrap();
    assert_eq!(restored.event_count(), reference.event_count());
    assert_eq!(restored.stream_count(), reference.stream_count());
    for item in [&first, &other, &second] {
        assert_eq!(restored.event(item.id()), reference.event(item.id()));
    }
    for name in ["s1", "s2"] {
        let stream = AuditStreamId::new(name).unwrap();
        restored.verify_stream(&stream).unwrap();
        assert_eq!(
            restored.tail_digest(&stream),
            reference.tail_digest(&stream)
        );
    }
    // Same immutable event is a no-op on an already reconstructed chain.
    restored.append(first).unwrap();
    assert_eq!(restored.event_count(), 3);
}

#[test]
fn wrong_version_digest_actor_time_and_unknown_fields_fail_closed() {
    let original = event("e1", "s", 1, AuditDigest::GENESIS, "actor", pending());
    let bytes = encode_audit_event(&original).unwrap();
    let baseline: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (path, value) in [
        ("/schema_version", serde_json::json!(2)),
        (
            "/payload/digest",
            serde_json::json!("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"),
        ),
        ("/payload/actor_id", serde_json::json!("changed")),
        ("/payload/effective_at_unix_ms", serde_json::json!("9999")),
        ("/payload/recorded_at_unix_ms", serde_json::json!("-1")),
        ("/payload/previous_digest", serde_json::json!("xyz")),
        ("/payload/action", serde_json::json!("unexpected.action")),
        (
            "/payload/resource",
            serde_json::json!("unexpected:resource"),
        ),
        ("/payload/sequence", serde_json::json!("0")),
    ] {
        let mut edited = baseline.clone();
        *edited.pointer_mut(path).unwrap() = value;
        assert!(
            decode_audit_event(&serde_json::to_vec(&edited).unwrap()).is_err(),
            "{path}"
        );
    }
    let mut extra = baseline.clone();
    extra["payload"]["surprise"] = serde_json::json!(1);
    assert!(decode_audit_event(&serde_json::to_vec(&extra).unwrap()).is_err());
    extra = baseline;
    extra["payload"]["sequence"] = serde_json::json!("1e3");
    assert!(decode_audit_event(&serde_json::to_vec(&extra).unwrap()).is_err());
}

#[test]
fn missing_parent_and_changed_identity_are_rejected() {
    let first = event("e1", "s", 1, AuditDigest::GENESIS, "actor", pending());
    let second = event("e2", "s", 2, first.digest(), "actor", pending());
    let first_bytes = encode_audit_event(&first).unwrap();
    let second_bytes = encode_audit_event(&second).unwrap();
    assert!(replay_audit_events([second_bytes.as_slice()]).is_err());
    assert_eq!(
        replay_audit_events([first_bytes.as_slice(), first_bytes.as_slice()])
            .unwrap()
            .event_count(),
        1
    );
    let changed = event("e1", "s", 1, AuditDigest::GENESIS, "different", pending());
    let altered = encode_audit_event(&changed).unwrap();
    assert!(replay_audit_events([first_bytes.as_slice(), altered.as_slice()]).is_err());
    let wrong_previous = event("e3", "s", 2, AuditDigest::GENESIS, "actor", pending());
    let wrong = encode_audit_event(&wrong_previous).unwrap();
    assert!(replay_audit_events([first_bytes.as_slice(), wrong.as_slice()]).is_err());
}

#[test]
fn every_reconciliation_payload_shape_roundtrips() {
    let c = case("case-1");
    let ahead = ReconciliationOutcome::ProviderAhead {
        case_id: c.id().clone(),
        proposed_terminal_event: DisbursementTerminalEvent::settled(
            DisbursementEventId::new("term-1").unwrap(),
            c.disbursement_id().clone(),
            ProviderEventReference::new("pe-1").unwrap(),
            ProviderSettlementReference::new("ps-1").unwrap(),
            53,
        ),
    };
    let kinds = [
        (pending(), ReconciliationAuditOutcomeKind::PendingAgreement),
        (ahead, ReconciliationAuditOutcomeKind::ProviderAhead),
        (
            ReconciliationOutcome::TerminalAgreement {
                case_id: c.id().clone(),
                status: DisbursementStatus::Settled,
            },
            ReconciliationAuditOutcomeKind::TerminalAgreement,
        ),
        (
            ReconciliationOutcome::Discrepancy {
                case_id: c.id().clone(),
                kind: DiscrepancyKind::TerminalTimestampMismatch,
            },
            ReconciliationAuditOutcomeKind::Discrepancy,
        ),
    ];
    for (idx, (outcome, kind)) in kinds.into_iter().enumerate() {
        let audit = event(
            &format!("e-{idx}"),
            &format!("stream-{idx}"),
            1,
            AuditDigest::GENESIS,
            "actor",
            outcome,
        );
        let round = decode_audit_event(&encode_audit_event(&audit).unwrap()).unwrap();
        assert_eq!(round, audit);
        let AuditPayload::Reconciliation(p) = round.payload();
        assert_eq!(p.outcome_kind(), kind);
    }
}

#[test]
fn malformed_payload_variant_and_unsupported_record_kind_are_rejected() {
    let first = event("e1", "s", 1, AuditDigest::GENESIS, "actor", pending());
    let baseline: serde_json::Value =
        serde_json::from_slice(&encode_audit_event(&first).unwrap()).unwrap();
    for (path, changed) in [
        (
            "/payload/reconciliation/outcome_kind",
            serde_json::json!("unknown"),
        ),
        (
            "/payload/reconciliation/discrepancy_kind",
            serde_json::json!("terminal_status_mismatch"),
        ),
        (
            "/payload/reconciliation/terminal_status",
            serde_json::json!("settled"),
        ),
        ("/payload/reconciliation/case_id", serde_json::json!("")),
        ("/record_type", serde_json::json!("audit.unknown")),
        (
            "/payload/reconciliation/provider_request_reference",
            serde_json::json!(""),
        ),
    ] {
        let mut value = baseline.clone();
        *value.pointer_mut(path).unwrap() = changed;
        assert!(
            decode_audit_event(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{path}"
        );
    }
}

#[test]
fn oversized_and_duplicate_json_object_keys_fail_closed() {
    let first = event("e1", "s", 1, AuditDigest::GENESIS, "actor", pending());
    let mut val: serde_json::Value =
        serde_json::from_slice(&encode_audit_event(&first).unwrap()).unwrap();
    val["payload"]["actor_id"] = serde_json::json!("x".repeat(1024 * 1024));
    assert!(decode_audit_event(&serde_json::to_vec(&val).unwrap()).is_err());
    // Serde struct decoding must reject both distinct values for a duplicated key;
    // neither first-write-wins nor last-write-wins is acceptable for audit.
    let s = String::from_utf8(encode_audit_event(&first).unwrap()).unwrap();
    let duplicated = s.replacen(
        "\"schema_version\":1",
        "\"schema_version\":1,\"schema_version\":1",
        1,
    );
    assert_ne!(duplicated, s);
    assert!(decode_audit_event(duplicated.as_bytes()).is_err());
}

#[test]
fn trusted_external_stream_tail_detects_truncation_and_coherent_rewrite() {
    use cofi_audit::codec::{AuditStreamAnchor, replay_audit_events_anchored};
    let first = event("e1", "s", 1, AuditDigest::GENESIS, "original", pending());
    let second = event("e2", "s", 2, first.digest(), "original", pending());
    let records = [
        encode_audit_event(&first).unwrap(),
        encode_audit_event(&second).unwrap(),
    ];
    let anchored =
        AuditStreamAnchor::new(AuditStreamId::new("s").unwrap(), 2, second.digest()).unwrap();
    assert_eq!(
        replay_audit_events_anchored(
            records.iter().map(Vec::as_slice),
            std::slice::from_ref(&anchored)
        )
        .unwrap()
        .event_count(),
        2
    );
    // A valid two-event chain with its terminal record lost still verifies
    // internally, but must fail against a trusted externally retained anchor.
    assert!(
        replay_audit_events_anchored([records[0].as_slice()], std::slice::from_ref(&anchored))
            .is_err()
    );

    // An attacker can recompute a perfectly coherent changed hash chain.
    // The old externally held tail digest must detect that replacement.
    let rewritten_first = event("e1", "s", 1, AuditDigest::GENESIS, "attacker", pending());
    let rewritten_second = event(
        "e2",
        "s",
        2,
        rewritten_first.digest(),
        "attacker",
        pending(),
    );
    let altered = [
        encode_audit_event(&rewritten_first).unwrap(),
        encode_audit_event(&rewritten_second).unwrap(),
    ];
    assert_eq!(
        replay_audit_events(altered.iter().map(Vec::as_slice))
            .unwrap()
            .event_count(),
        2
    );
    assert!(replay_audit_events_anchored(altered.iter().map(Vec::as_slice), &[anchored]).is_err());
}

#[test]
fn missing_extra_duplicate_or_zero_audit_anchors_fail_closed() {
    use cofi_audit::codec::{AuditStreamAnchor, replay_audit_events_anchored};
    let one = event("a", "stream-a", 1, AuditDigest::GENESIS, "actor", pending());
    let two = event("b", "stream-b", 1, AuditDigest::GENESIS, "actor", pending());
    let records = [
        encode_audit_event(&one).unwrap(),
        encode_audit_event(&two).unwrap(),
    ];
    let a =
        AuditStreamAnchor::new(AuditStreamId::new("stream-a").unwrap(), 1, one.digest()).unwrap();
    let b =
        AuditStreamAnchor::new(AuditStreamId::new("stream-b").unwrap(), 1, two.digest()).unwrap();
    assert!(
        replay_audit_events_anchored(records.iter().map(Vec::as_slice), std::slice::from_ref(&a))
            .is_err()
    );
    assert!(
        replay_audit_events_anchored(
            records.iter().map(Vec::as_slice),
            &[a.clone(), a.clone(), b.clone()]
        )
        .is_err()
    );
    assert!(replay_audit_events_anchored(records.iter().map(Vec::as_slice), &[]).is_err());
    assert_eq!(
        replay_audit_events_anchored(records.iter().map(Vec::as_slice), &[a.clone(), b])
            .unwrap()
            .stream_count(),
        2
    );
    assert!(
        AuditStreamAnchor::new(AuditStreamId::new("s").unwrap(), 0, AuditDigest::GENESIS).is_err()
    );
    assert!(
        AuditStreamAnchor::new(AuditStreamId::new("s").unwrap(), 1, AuditDigest::GENESIS).is_err()
    );
    let incorrect =
        AuditStreamAnchor::new(AuditStreamId::new("stream-a").unwrap(), 2, one.digest()).unwrap();
    assert!(replay_audit_events_anchored([records[0].as_slice()], &[incorrect]).is_err());
}
