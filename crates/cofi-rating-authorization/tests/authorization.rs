#![allow(clippy::unwrap_used)]

use cofi_billing::BillingCustomerId;
use cofi_ledger::{Currency, LedgerScopeId};
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageAggregate,
    UsageEvent, UsageEventId, UsageValue, WindowSize,
};
use cofi_rating::{RatePlan, RatingError, RatingEventId, RatingPlanId};
use cofi_rating_authorization::{
    AuthorizedRatingError, AuthorizedRatingOutcome, AuthorizedRatingRegistry,
    AuthorizedRatingRequest,
};
use cofi_subscriptions::{
    SubscriptionEventId, SubscriptionId, SubscriptionRegistry, SubscriptionRequest,
};

const DAY: i64 = 86_400_000;

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}

fn scope(value: &str) -> LedgerScopeId {
    LedgerScopeId::new(value).unwrap()
}

fn customer(value: &str) -> BillingCustomerId {
    BillingCustomerId::new(value).unwrap()
}

fn subject(value: &str) -> SubjectId {
    SubjectId::new(value).unwrap()
}

fn meter(value: &str) -> MeterId {
    MeterId::new(value).unwrap()
}

fn plan(
    id: &str,
    meter_id: &str,
    price: i128,
    included: i128,
    effective_from: i64,
    effective_until: Option<i64>,
) -> RatePlan {
    RatePlan::new(
        RatingPlanId::new(id).unwrap(),
        meter(meter_id),
        usd(),
        price,
        included,
        effective_from,
        effective_until,
    )
    .unwrap()
}

fn count_aggregate(meter_id: &str, subject_id: &str, count: u64) -> UsageAggregate {
    let meter_id = meter(meter_id);
    let subject_id = subject(subject_id);
    let event_type = EventType::new("api.call").unwrap();
    let mut engine = MeteringEngine::new();
    engine
        .register_meter(
            MeterDefinition::new(
                meter_id.clone(),
                event_type.clone(),
                Aggregation::Count,
                WindowSize::Day,
                Some(0),
            )
            .unwrap(),
        )
        .unwrap();
    for index in 0..count {
        engine
            .ingest(
                UsageEvent::new(
                    UsageEventId::new(format!(
                        "usage-{subject_id}-{index}",
                        subject_id = subject_id.as_str()
                    ))
                    .unwrap(),
                    meter_id.clone(),
                    event_type.clone(),
                    subject_id.clone(),
                    1_000 + i64::try_from(index).unwrap(),
                    2_000 + i64::try_from(index).unwrap(),
                    UsageValue::Count,
                )
                .unwrap(),
            )
            .unwrap();
    }
    engine.aggregate(&meter_id, &subject_id, 0, DAY).unwrap()
}

#[allow(clippy::too_many_arguments)]
fn add_subscription(
    registry: &mut SubscriptionRegistry,
    event_id: &str,
    subscription_id: &str,
    scope_id: &str,
    customer_id: &str,
    subject_id: &str,
    plan: RatePlan,
    active_from: i64,
    active_until: Option<i64>,
) {
    registry
        .create(
            SubscriptionRequest::new(
                SubscriptionEventId::new(event_id).unwrap(),
                SubscriptionId::new(subscription_id).unwrap(),
                scope(scope_id),
                customer(customer_id),
                subject(subject_id),
                plan,
                active_from,
                active_until,
            )
            .unwrap(),
        )
        .unwrap();
}

fn request(
    event_id: &str,
    scope_id: &str,
    customer_id: &str,
    aggregate: UsageAggregate,
    rated_at: i64,
) -> AuthorizedRatingRequest {
    AuthorizedRatingRequest::new(
        RatingEventId::new(event_id).unwrap(),
        scope(scope_id),
        customer(customer_id),
        aggregate,
        rated_at,
    )
}

fn standard_subscriptions() -> SubscriptionRegistry {
    let mut subscriptions = SubscriptionRegistry::new();
    add_subscription(
        &mut subscriptions,
        "subscription-event-1",
        "subscription-1",
        "org-1",
        "customer-1",
        "subject-1",
        plan("plan-assigned", "meter-1", 10, 1, 0, Some(2 * DAY)),
        0,
        Some(DAY),
    );
    subscriptions
}

#[test]
fn authorized_rating_derives_plan_from_subscription_and_preserves_lineage() {
    let subscriptions = standard_subscriptions();
    let aggregate = count_aggregate("meter-1", "subject-1", 3);
    let _unassigned_plan = plan("plan-unassigned", "meter-1", 999, 0, 0, Some(2 * DAY));
    let mut registry = AuthorizedRatingRegistry::new();

    let outcome = registry
        .rate(
            request("rating-1", "org-1", "customer-1", aggregate, DAY),
            &subscriptions,
        )
        .unwrap();
    assert!(matches!(outcome, AuthorizedRatingOutcome::Rated { .. }));
    let authorization = outcome.authorization();
    assert_eq!(authorization.subscription().id().as_str(), "subscription-1");
    assert_eq!(
        authorization.subscription().source_event_id().as_str(),
        "subscription-event-1"
    );
    assert_eq!(authorization.charge().plan_id().as_str(), "plan-assigned");
    assert_eq!(
        authorization.charge().billing_customer_id().as_str(),
        "customer-1"
    );
    assert_eq!(authorization.charge().subject_id().as_str(), "subject-1");
    assert_eq!(authorization.charge().meter_id().as_str(), "meter-1");
    assert_eq!(authorization.charge().aggregate_units(), 3);
    assert_eq!(authorization.charge().billable_units(), 2);
    assert_eq!(authorization.charge().unit_price_minor(), 10);
    assert_eq!(authorization.charge().amount_minor(), 20);
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.rating_event_count(), 1);
}

#[test]
fn exact_replay_returns_historical_authorization_without_re_resolving() {
    let mut subscriptions = standard_subscriptions();
    let aggregate = count_aggregate("meter-1", "subject-1", 2);
    let rating_request = request("rating-replay", "org-1", "customer-1", aggregate, DAY);
    let mut registry = AuthorizedRatingRegistry::new();

    let first = registry
        .rate(rating_request.clone(), &subscriptions)
        .unwrap();
    add_subscription(
        &mut subscriptions,
        "subscription-event-next",
        "subscription-next",
        "org-1",
        "customer-1",
        "subject-1",
        plan("plan-next", "meter-1", 20, 0, DAY, Some(2 * DAY)),
        DAY,
        Some(2 * DAY),
    );
    let replay = registry.rate(rating_request, &subscriptions).unwrap();

    assert!(matches!(first, AuthorizedRatingOutcome::Rated { .. }));
    assert!(matches!(replay, AuthorizedRatingOutcome::Replayed { .. }));
    assert_eq!(first.authorization(), replay.authorization());
    assert_eq!(
        replay.authorization().subscription().id().as_str(),
        "subscription-1"
    );
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.rating_event_count(), 1);
}

#[test]
fn missing_subscription_reserves_nothing_and_corrected_retry_succeeds() {
    let aggregate = count_aggregate("meter-1", "subject-1", 2);
    let rating_request = request("rating-retry", "org-1", "customer-1", aggregate, DAY);
    let mut subscriptions = SubscriptionRegistry::new();
    let mut registry = AuthorizedRatingRegistry::new();

    let error = registry
        .rate(rating_request.clone(), &subscriptions)
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedRatingError::NoCanonicalSubscription { .. }
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.rating_event_count(), 0);

    add_subscription(
        &mut subscriptions,
        "subscription-event-retry",
        "subscription-retry",
        "org-1",
        "customer-1",
        "subject-1",
        plan("plan-retry", "meter-1", 10, 0, 0, Some(DAY)),
        0,
        Some(DAY),
    );
    let corrected = registry.rate(rating_request, &subscriptions).unwrap();
    assert!(matches!(corrected, AuthorizedRatingOutcome::Rated { .. }));
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.rating_event_count(), 1);
}

#[test]
fn wrong_scope_customer_or_subject_fails_closed() {
    let subscriptions = standard_subscriptions();
    let mut registry = AuthorizedRatingRegistry::new();

    for (event, scope_id, customer_id, subject_id) in [
        ("rating-wrong-scope", "org-2", "customer-1", "subject-1"),
        ("rating-wrong-customer", "org-1", "customer-2", "subject-1"),
        ("rating-wrong-subject", "org-1", "customer-1", "subject-2"),
    ] {
        let error = registry
            .rate(
                request(
                    event,
                    scope_id,
                    customer_id,
                    count_aggregate("meter-1", subject_id, 1),
                    DAY,
                ),
                &subscriptions,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            AuthorizedRatingError::NoCanonicalSubscription { .. }
        ));
    }
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.rating_event_count(), 0);
}

#[test]
fn wrong_meter_and_partial_window_coverage_fail_closed() {
    let mut subscriptions = SubscriptionRegistry::new();
    add_subscription(
        &mut subscriptions,
        "subscription-event-partial",
        "subscription-partial",
        "org-1",
        "customer-1",
        "subject-1",
        plan("plan-partial", "meter-1", 10, 0, 0, Some(2 * DAY)),
        0,
        Some(DAY - 1),
    );
    let mut registry = AuthorizedRatingRegistry::new();

    for (event, meter_id) in [
        ("rating-partial", "meter-1"),
        ("rating-wrong-meter", "meter-2"),
    ] {
        let error = registry
            .rate(
                request(
                    event,
                    "org-1",
                    "customer-1",
                    count_aggregate(meter_id, "subject-1", 1),
                    DAY,
                ),
                &subscriptions,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            AuthorizedRatingError::NoCanonicalSubscription { .. }
        ));
    }
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.rating_event_count(), 0);
}

#[test]
fn p17_timestamp_validation_propagates_without_reserving_authorization() {
    let subscriptions = standard_subscriptions();
    let mut registry = AuthorizedRatingRegistry::new();
    let error = registry
        .rate(
            request(
                "rating-early",
                "org-1",
                "customer-1",
                count_aggregate("meter-1", "subject-1", 1),
                DAY - 1,
            ),
            &subscriptions,
        )
        .unwrap_err();
    assert_eq!(
        error,
        AuthorizedRatingError::Rating(RatingError::RatingBeforeWindowEnd {
            rated_at_unix_ms: DAY - 1,
            window_end_unix_ms: DAY,
        })
    );
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.rating_event_count(), 0);
}

#[test]
fn conflicting_reuse_of_rating_event_fails_before_second_rating() {
    let subscriptions = standard_subscriptions();
    let aggregate = count_aggregate("meter-1", "subject-1", 2);
    let mut registry = AuthorizedRatingRegistry::new();
    registry
        .rate(
            request(
                "rating-conflict",
                "org-1",
                "customer-1",
                aggregate.clone(),
                DAY,
            ),
            &subscriptions,
        )
        .unwrap();

    let error = registry
        .rate(
            request("rating-conflict", "org-1", "customer-1", aggregate, DAY + 1),
            &subscriptions,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedRatingError::RatingEventConflict(_)
    ));
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.rating_event_count(), 1);
}

#[test]
fn second_event_for_same_usage_window_is_rejected_by_canonical_rating_key() {
    let subscriptions = standard_subscriptions();
    let aggregate = count_aggregate("meter-1", "subject-1", 2);
    let mut registry = AuthorizedRatingRegistry::new();
    registry
        .rate(
            request(
                "rating-first",
                "org-1",
                "customer-1",
                aggregate.clone(),
                DAY,
            ),
            &subscriptions,
        )
        .unwrap();

    let error = registry
        .rate(
            request("rating-second", "org-1", "customer-1", aggregate, DAY),
            &subscriptions,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedRatingError::Rating(RatingError::DuplicateRatingKey { .. })
    ));
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.rating_event_count(), 1);
}

#[test]
fn event_and_charge_lookups_bind_the_same_canonical_authorization() {
    let subscriptions = standard_subscriptions();
    let event_id = RatingEventId::new("rating-lookup").unwrap();
    let mut registry = AuthorizedRatingRegistry::new();
    let outcome = registry
        .rate(
            AuthorizedRatingRequest::new(
                event_id.clone(),
                scope("org-1"),
                customer("customer-1"),
                count_aggregate("meter-1", "subject-1", 1),
                DAY,
            ),
            &subscriptions,
        )
        .unwrap();

    assert_eq!(
        registry.authorization_for_event(&event_id),
        Some(outcome.authorization())
    );
    assert_eq!(
        registry.charge_for_event(&event_id),
        Some(outcome.authorization().charge())
    );
}

#[test]
fn zero_usage_is_authorized_but_keeps_zero_economic_amount() {
    let subscriptions = standard_subscriptions();
    let mut registry = AuthorizedRatingRegistry::new();
    let outcome = registry
        .rate(
            request(
                "rating-zero",
                "org-1",
                "customer-1",
                count_aggregate("meter-1", "subject-1", 0),
                DAY,
            ),
            &subscriptions,
        )
        .unwrap();

    assert_eq!(outcome.authorization().charge().aggregate_units(), 0);
    assert_eq!(outcome.authorization().charge().billable_units(), 0);
    assert_eq!(outcome.authorization().charge().amount_minor(), 0);
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.rating_event_count(), 1);
}
