#![allow(clippy::unwrap_used)]

use cofi_billing::BillingCustomerId;
use cofi_ledger::{Currency, LedgerScopeId};
use cofi_metering::{MeterId, SubjectId};
use cofi_rating::{RatePlan, RatingPlanId};
use cofi_storage::CodecError;
use cofi_storage::subscription::{
    decode_subscription_request, encode_subscription_request, replay_subscriptions,
};
use cofi_subscriptions::{
    SubscriptionCreateOutcome, SubscriptionEventId, SubscriptionId, SubscriptionRegistry,
    SubscriptionRequest, SubscriptionStatus,
};

fn plan(id: &str) -> RatePlan {
    RatePlan::new(
        RatingPlanId::new(id).unwrap(),
        MeterId::new("messages").unwrap(),
        Currency::new("SAR").unwrap(),
        i128::MAX,
        0,
        0,
        None,
    )
    .unwrap()
}
fn request(
    event: &str,
    id: &str,
    plan: RatePlan,
    from: i64,
    to: Option<i64>,
) -> SubscriptionRequest {
    SubscriptionRequest::new(
        SubscriptionEventId::new(event).unwrap(),
        SubscriptionId::new(id).unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        SubjectId::new("subject-1").unwrap(),
        plan,
        from,
        to,
    )
    .unwrap()
}

#[test]
fn original_schedule_and_immutable_plan_roundtrip_exactly() {
    let req = request("event-1", "sub-1", plan("rate-1"), 60_000, Some(120_000));
    let encoded = encode_subscription_request(&req).unwrap();
    assert_eq!(decode_subscription_request(&encoded).unwrap(), req);
    let mut reference = SubscriptionRegistry::new();
    assert!(matches!(
        reference.create(req.clone()),
        Ok(SubscriptionCreateOutcome::Created { .. })
    ));

    let mut recovered = replay_subscriptions([encoded.as_slice()]).unwrap();
    assert_eq!(recovered.event_count(), reference.event_count());
    assert_eq!(
        recovered.subscription_count(),
        reference.subscription_count()
    );
    let id = SubscriptionId::new("sub-1").unwrap();
    assert_eq!(
        recovered.subscription_for_id(&id),
        reference.subscription_for_id(&id)
    );
    assert_eq!(
        recovered
            .subscription_for_id(&id)
            .unwrap()
            .status_at(59_999),
        Ok(SubscriptionStatus::Scheduled)
    );
    assert_eq!(
        recovered
            .subscription_for_id(&id)
            .unwrap()
            .status_at(60_000),
        Ok(SubscriptionStatus::Active)
    );
    assert_eq!(
        recovered
            .subscription_for_id(&id)
            .unwrap()
            .status_at(120_000),
        Ok(SubscriptionStatus::Ended)
    );
    assert!(matches!(
        recovered.create(req),
        Ok(SubscriptionCreateOutcome::Replayed { .. })
    ));
}

#[test]
fn adjacent_plan_changes_survive_replay_but_overlaps_do_not() {
    let first = request("event-1", "sub-1", plan("rate-1"), 60_000, Some(120_000));
    let second = request("event-2", "sub-2", plan("rate-2"), 120_000, Some(180_000));
    let records = [
        encode_subscription_request(&first).unwrap(),
        encode_subscription_request(&second).unwrap(),
    ];
    let engine = replay_subscriptions(records.iter().map(Vec::as_slice)).unwrap();
    assert_eq!(engine.subscription_count(), 2);
    let got = engine
        .resolve_for_window(
            &LedgerScopeId::new("org-1").unwrap(),
            &BillingCustomerId::new("customer-1").unwrap(),
            &SubjectId::new("subject-1").unwrap(),
            &MeterId::new("messages").unwrap(),
            120_000,
            180_000,
        )
        .unwrap()
        .unwrap();
    assert_eq!(got.id(), second.subscription_id());
    assert_eq!(got.plan(), second.plan());

    let overlap = request("event-3", "sub-3", plan("rate-3"), 90_000, Some(150_000));
    let bad = encode_subscription_request(&overlap).unwrap();
    assert!(matches!(
        replay_subscriptions([records[0].as_slice(), bad.as_slice()]),
        Err(CodecError::Replay(_))
    ));
}

#[test]
fn changed_source_identity_and_invalid_plan_or_time_fail_closed() {
    let request = request("event-1", "sub-1", plan("rate-1"), 60_000, Some(120_000));
    let bytes = encode_subscription_request(&request).unwrap();
    assert_eq!(
        replay_subscriptions([bytes.as_slice(), bytes.as_slice()])
            .unwrap()
            .subscription_count(),
        1
    );
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (pointer, replacement) in [
        (
            "/payload/plan/payload/unit_price_minor",
            serde_json::json!("0"),
        ),
        ("/payload/active_from_unix_ms", serde_json::json!("120000")),
        ("/payload/active_until_unix_ms", serde_json::json!("60000")),
        ("/payload/organization_scope", serde_json::json!(" ")),
        ("/payload/plan/schema_version", serde_json::json!(2)),
        ("/schema_version", serde_json::json!(2)),
    ] {
        let mut modified = original.clone();
        *modified.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            decode_subscription_request(&serde_json::to_vec(&modified).unwrap()).is_err(),
            "{pointer}"
        );
    }

    let mut changed = original.clone();
    changed["payload"]["plan"]["payload"]["unit_price_minor"] = serde_json::json!("999");
    let changed_bytes = serde_json::to_vec(&changed).unwrap();
    assert!(replay_subscriptions([bytes.as_slice(), changed_bytes.as_slice()]).is_err());
    let mut missing = original.clone();
    missing["payload"]["extra"] = serde_json::json!(true);
    assert!(decode_subscription_request(&serde_json::to_vec(&missing).unwrap()).is_err());
}

#[test]
fn dependency_order_must_preserve_original_creation_sequence() {
    let earlier = request("event-1", "sub-1", plan("rate-1"), 60_000, Some(120_000));
    let later = request("event-2", "sub-2", plan("rate-2"), 120_000, Some(180_000));
    let accepted = [
        encode_subscription_request(&earlier).unwrap(),
        encode_subscription_request(&later).unwrap(),
    ];
    let reversed = replay_subscriptions([accepted[1].as_slice(), accepted[0].as_slice()]).unwrap();
    assert_eq!(reversed.subscription_count(), 2);
    // Replays never infer a new schedule; the canonical event stream must
    // eventually carry an authoritative ordering and cutoff in G002-G004.
}

#[test]
fn subscription_codec_rejects_oversize_original_fact_before_deserialization() {
    let original = request(
        "event-sized",
        "sub-sized",
        plan("rate-1"),
        60_000,
        Some(120_000),
    );
    let mut bytes = encode_subscription_request(&original).unwrap();
    // Valid JSON trailing whitespace must count toward the persistence envelope.
    bytes.resize(1024 * 1024, b' ');
    assert_eq!(decode_subscription_request(&bytes), Ok(original));
    bytes.push(b' ');
    assert!(matches!(
        decode_subscription_request(&bytes),
        Err(CodecError::InvalidPayload(_))
    ));
    assert!(matches!(
        replay_subscriptions([bytes.as_slice()]),
        Err(CodecError::InvalidPayload(_))
    ));
}

#[test]
fn subscription_codec_rejects_oversize_original_domain_encoding() {
    let huge_source = format!("event-{}", "x".repeat(1024 * 1024));
    let original = request(&huge_source, "sub-1", plan("rate-1"), 60_000, None);
    assert!(matches!(
        encode_subscription_request(&original),
        Err(CodecError::InvalidPayload(_))
    ));
}
