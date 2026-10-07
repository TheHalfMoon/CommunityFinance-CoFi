#![allow(clippy::unwrap_used)]

use cofi_billing::BillingCustomerId;
use cofi_ledger::{Currency, LedgerScopeId};
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageEvent,
    UsageEventId, UsageValue, WindowSize,
};
use cofi_rating::{RatePlan, RatingEventId, RatingPlanId};
use cofi_rating_authorization::{
    AuthorizedRating, AuthorizedRatingRegistry, AuthorizedRatingRequest,
};
use cofi_storage::authorized_rating::{
    decode_authorized_rating, encode_authorized_rating, replay_authorized_ratings,
};
use cofi_storage::rating::{RatingAcceptance, encode_rating_acceptance};
use cofi_subscriptions::{
    SubscriptionEventId, SubscriptionId, SubscriptionRegistry, SubscriptionRequest,
};

const DAY: i64 = 86_400_000;

struct Fixture {
    subscription: SubscriptionRequest,
    evidence: Vec<u8>,
    accepted: AuthorizedRating,
}

fn fixture() -> Fixture {
    let meter = MeterDefinition::new(
        MeterId::new("api").unwrap(),
        EventType::new("call").unwrap(),
        Aggregation::Count,
        WindowSize::Day,
        Some(0),
    )
    .unwrap();
    let events = (0..3)
        .map(|idx| {
            UsageEvent::new(
                UsageEventId::new(format!("source-{idx}")).unwrap(),
                meter.id().clone(),
                EventType::new("call").unwrap(),
                SubjectId::new("subject-1").unwrap(),
                1_000 + idx,
                2_000 + idx,
                UsageValue::Count,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let plan = RatePlan::new(
        RatingPlanId::new("rate-1").unwrap(),
        meter.id().clone(),
        Currency::new("SAR").unwrap(),
        10,
        1,
        0,
        Some(2 * DAY),
    )
    .unwrap();
    let subscription = SubscriptionRequest::new(
        SubscriptionEventId::new("sub-event").unwrap(),
        SubscriptionId::new("subscription-1").unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        SubjectId::new("subject-1").unwrap(),
        plan.clone(),
        0,
        Some(DAY),
    )
    .unwrap();
    let mut subscriptions = SubscriptionRegistry::new();
    subscriptions.create(subscription.clone()).unwrap();
    let mut meter_engine = MeteringEngine::new();
    meter_engine.register_meter(meter.clone()).unwrap();
    for ev in &events {
        meter_engine.ingest(ev.clone()).unwrap();
    }
    let aggregate = meter_engine
        .aggregate(meter.id(), &SubjectId::new("subject-1").unwrap(), 0, DAY)
        .unwrap();
    let rating_id = RatingEventId::new("rating-1").unwrap();
    let customer = BillingCustomerId::new("customer-1").unwrap();
    let outcome = AuthorizedRatingRegistry::new()
        .rate(
            AuthorizedRatingRequest::new(
                rating_id.clone(),
                LedgerScopeId::new("org-1").unwrap(),
                customer.clone(),
                aggregate,
                DAY,
            ),
            &subscriptions,
        )
        .unwrap();
    let accepted = outcome.authorization().clone();
    let evidence = encode_rating_acceptance(&RatingAcceptance {
        meter: &meter,
        events: &events,
        plan: &plan,
        rating_event_id: &rating_id,
        subject_id: &SubjectId::new("subject-1").unwrap(),
        billing_customer_id: &customer,
        window_start_unix_ms: 0,
        window_end_unix_ms: DAY,
        rated_at_unix_ms: DAY,
        accepted_charge: accepted.charge(),
    })
    .unwrap();
    Fixture {
        subscription,
        evidence,
        accepted,
    }
}

#[test]
fn accepted_authorized_rating_replays_exact_source_subscription_and_charge() {
    let f = fixture();
    assert_eq!(f.accepted.charge().amount_minor(), 20);
    let bytes = encode_authorized_rating(&f.subscription, &f.evidence, &f.accepted).unwrap();
    assert_eq!(decode_authorized_rating(&bytes).unwrap(), f.accepted);
    let mut recovered = replay_authorized_ratings([bytes.as_slice()]).unwrap();
    assert_eq!(recovered.authorization_count(), 1);
    assert_eq!(recovered.rating_event_count(), 1);
    assert_eq!(
        recovered.authorization_for_event(&RatingEventId::new("rating-1").unwrap()),
        Some(&f.accepted),
    );
    assert!(matches!(
        recovered.rate(
            AuthorizedRatingRequest::new(
                RatingEventId::new("rating-1").unwrap(),
                LedgerScopeId::new("org-1").unwrap(),
                BillingCustomerId::new("customer-1").unwrap(),
                {
                    let mut engine = MeteringEngine::new();
                    let meter = MeterDefinition::new(
                        MeterId::new("api").unwrap(),
                        EventType::new("call").unwrap(),
                        Aggregation::Count,
                        WindowSize::Day,
                        Some(0),
                    )
                    .unwrap();
                    engine.register_meter(meter.clone()).unwrap();
                    for idx in 0..3 {
                        engine
                            .ingest(
                                UsageEvent::new(
                                    UsageEventId::new(format!("source-{idx}")).unwrap(),
                                    meter.id().clone(),
                                    EventType::new("call").unwrap(),
                                    SubjectId::new("subject-1").unwrap(),
                                    1_000 + idx,
                                    2_000 + idx,
                                    UsageValue::Count,
                                )
                                .unwrap(),
                            )
                            .unwrap();
                    }
                    engine
                        .aggregate(meter.id(), &SubjectId::new("subject-1").unwrap(), 0, DAY)
                        .unwrap()
                },
                DAY,
            ),
            &{
                let mut s = SubscriptionRegistry::new();
                s.create(f.subscription).unwrap();
                s
            }
        ),
        Ok(cofi_rating_authorization::AuthorizedRatingOutcome::Replayed { .. }),
    ));
}

#[test]
fn altered_price_scope_usage_and_receipt_cannot_be_accepted() {
    let f = fixture();
    let encoded = encode_authorized_rating(&f.subscription, &f.evidence, &f.accepted).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    for (path, replacement) in [
        (
            "/payload/subscription/payload/plan/payload/unit_price_minor",
            serde_json::json!("999"),
        ),
        (
            "/payload/subscription/payload/organization_scope",
            serde_json::json!("other-org"),
        ),
        (
            "/payload/rating/payload/usage_events/0/payload/id",
            serde_json::json!("changed-source"),
        ),
        (
            "/payload/rating/payload/charge/amount_minor",
            serde_json::json!("9999"),
        ),
        (
            "/payload/rating/payload/usage_events",
            serde_json::json!([]),
        ),
        ("/schema_version", serde_json::json!(2)),
    ] {
        let mut modified = original.clone();
        *modified.pointer_mut(path).unwrap() = replacement;
        assert!(
            decode_authorized_rating(&serde_json::to_vec(&modified).unwrap()).is_err(),
            "{path}"
        );
    }
}

#[test]
fn same_rating_identity_replay_cannot_swap_subscription_snapshot() {
    let f = fixture();
    let encoded = encode_authorized_rating(&f.subscription, &f.evidence, &f.accepted).unwrap();
    assert_eq!(
        replay_authorized_ratings([encoded.as_slice(), encoded.as_slice()])
            .unwrap()
            .authorization_count(),
        1
    );
    let mut altered: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    altered["payload"]["subscription"]["payload"]["subscription_id"] =
        serde_json::json!("other-subscription");
    let edited = serde_json::to_vec(&altered).unwrap();
    assert!(replay_authorized_ratings([encoded.as_slice(), edited.as_slice()]).is_err());
    let mut extra: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    extra["payload"]["unexpected"] = serde_json::json!(true);
    assert!(decode_authorized_rating(&serde_json::to_vec(&extra).unwrap()).is_err());
}

#[test]
fn encoder_cannot_claim_wrong_accepted_authorization() {
    let f = fixture();
    let plan = RatePlan::new(
        RatingPlanId::new("changed-plan").unwrap(),
        MeterId::new("api").unwrap(),
        Currency::new("SAR").unwrap(),
        100,
        1,
        0,
        Some(2 * DAY),
    )
    .unwrap();
    let different_subscription = SubscriptionRequest::new(
        SubscriptionEventId::new("changed-source").unwrap(),
        SubscriptionId::new("changed-sub").unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        SubjectId::new("subject-1").unwrap(),
        plan,
        0,
        Some(DAY),
    )
    .unwrap();
    assert!(encode_authorized_rating(&different_subscription, &f.evidence, &f.accepted).is_err());
}
