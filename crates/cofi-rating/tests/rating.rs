#![allow(clippy::unwrap_used)]

use cofi_billing::BillingCustomerId;
use cofi_ledger::Currency;
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageAggregate,
    UsageEvent, UsageEventId, UsageValue, WindowSize,
};
use cofi_rating::{
    RatePlan, RatedChargeId, RatingError, RatingEventId, RatingOutcome, RatingPlanId,
    RatingRegistry, RatingRequest,
};

const DAY: i64 = 86_400_000;

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}

fn count_aggregate(meter: &str, subject: &str, count: u64) -> UsageAggregate {
    let meter_id = MeterId::new(meter).unwrap();
    let subject_id = SubjectId::new(subject).unwrap();
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
                    UsageEventId::new(format!("count-{index}")).unwrap(),
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

fn sum_aggregate(meter: &str, subject: &str, values: &[i128]) -> UsageAggregate {
    let meter_id = MeterId::new(meter).unwrap();
    let subject_id = SubjectId::new(subject).unwrap();
    let event_type = EventType::new("storage.byte").unwrap();
    let mut engine = MeteringEngine::new();
    engine
        .register_meter(
            MeterDefinition::new(
                meter_id.clone(),
                event_type.clone(),
                Aggregation::Sum,
                WindowSize::Day,
                Some(0),
            )
            .unwrap(),
        )
        .unwrap();
    for (index, value) in values.iter().copied().enumerate() {
        engine
            .ingest(
                UsageEvent::new(
                    UsageEventId::new(format!("sum-{index}")).unwrap(),
                    meter_id.clone(),
                    event_type.clone(),
                    subject_id.clone(),
                    1_000 + i64::try_from(index).unwrap(),
                    2_000 + i64::try_from(index).unwrap(),
                    UsageValue::Sum(value),
                )
                .unwrap(),
            )
            .unwrap();
    }
    engine.aggregate(&meter_id, &subject_id, 0, DAY).unwrap()
}

fn plan(meter: &str, price: i128, included: i128) -> RatePlan {
    RatePlan::new(
        RatingPlanId::new("plan-1").unwrap(),
        MeterId::new(meter).unwrap(),
        usd(),
        price,
        included,
        0,
        Some(DAY),
    )
    .unwrap()
}

fn request(
    event: &str,
    aggregate: UsageAggregate,
    plan: RatePlan,
    customer: &str,
) -> RatingRequest {
    RatingRequest::new(
        RatingEventId::new(event).unwrap(),
        aggregate,
        BillingCustomerId::new(customer).unwrap(),
        plan,
        DAY,
    )
    .unwrap()
}

#[test]
fn identifiers_and_plan_parameters_fail_closed() {
    assert_eq!(
        RatingPlanId::new(" "),
        Err(RatingError::EmptyIdentifier("rating_plan_id"))
    );
    assert_eq!(
        RatingEventId::new(""),
        Err(RatingError::EmptyIdentifier("rating_event_id"))
    );
    assert_eq!(
        RatedChargeId::new("\t"),
        Err(RatingError::EmptyIdentifier("rated_charge_id"))
    );

    let meter = MeterId::new("meter-1").unwrap();
    let plan_id = RatingPlanId::new("plan-x").unwrap();
    assert_eq!(
        RatePlan::new(plan_id.clone(), meter.clone(), usd(), 0, 0, 0, None),
        Err(RatingError::NonPositiveUnitPrice(0))
    );
    assert_eq!(
        RatePlan::new(plan_id.clone(), meter.clone(), usd(), 1, -1, 0, None),
        Err(RatingError::NegativeIncludedUnits(-1))
    );
    assert_eq!(
        RatePlan::new(plan_id.clone(), meter.clone(), usd(), 1, 0, -1, None),
        Err(RatingError::InvalidEffectiveTimestamp(
            "effective_from_unix_ms"
        ))
    );
    assert_eq!(
        RatePlan::new(plan_id, meter, usd(), 1, 0, 10, Some(10)),
        Err(RatingError::InvalidEffectiveRange {
            effective_from_unix_ms: 10,
            effective_until_unix_ms: 10,
        })
    );
}

#[test]
fn rating_request_requires_closed_window_and_valid_timestamp() {
    let aggregate = count_aggregate("meter-1", "subject-1", 1);
    assert_eq!(
        RatingRequest::new(
            RatingEventId::new("event-negative").unwrap(),
            aggregate.clone(),
            BillingCustomerId::new("customer-1").unwrap(),
            plan("meter-1", 10, 0),
            -1,
        ),
        Err(RatingError::InvalidRatedAt(-1))
    );
    assert_eq!(
        RatingRequest::new(
            RatingEventId::new("event-early").unwrap(),
            aggregate,
            BillingCustomerId::new("customer-1").unwrap(),
            plan("meter-1", 10, 0),
            DAY - 1,
        ),
        Err(RatingError::RatingBeforeWindowEnd {
            rated_at_unix_ms: DAY - 1,
            window_end_unix_ms: DAY,
        })
    );
}

#[test]
fn count_usage_rates_deterministically_and_preserves_explicit_customer() {
    let aggregate = count_aggregate("meter-1", "meter-subject-1", 3);
    let mut registry = RatingRegistry::new();
    let outcome = registry
        .rate(request(
            "rating-1",
            aggregate,
            plan("meter-1", 25, 1),
            "billing-customer-9",
        ))
        .unwrap();

    let charge = outcome.charge();
    assert!(matches!(outcome, RatingOutcome::Rated { .. }));
    assert_eq!(charge.id().as_str(), "rating:rating-1:charge");
    assert_eq!(charge.rating_event_id().as_str(), "rating-1");
    assert_eq!(charge.plan_id().as_str(), "plan-1");
    assert_eq!(charge.meter_id().as_str(), "meter-1");
    assert_eq!(charge.subject_id().as_str(), "meter-subject-1");
    assert_eq!(charge.billing_customer_id().as_str(), "billing-customer-9");
    assert_eq!(charge.source_event_count(), 3);
    assert_eq!(charge.aggregate_units(), 3);
    assert_eq!(charge.billable_units(), 2);
    assert_eq!(charge.unit_price_minor(), 25);
    assert_eq!(charge.included_units(), 1);
    assert_eq!(charge.amount_minor(), 50);
    assert_eq!(charge.currency(), usd());
    assert_eq!(charge.window_start_unix_ms(), 0);
    assert_eq!(charge.window_end_unix_ms(), DAY);
    assert_eq!(charge.rated_at_unix_ms(), DAY);
}

#[test]
fn sum_usage_rates_from_aggregate_value_not_event_count() {
    let aggregate = sum_aggregate("meter-sum", "subject-1", &[4, 6]);
    let mut registry = RatingRegistry::new();
    let outcome = registry
        .rate(request(
            "rating-sum",
            aggregate,
            plan("meter-sum", 3, 2),
            "customer-1",
        ))
        .unwrap();
    let charge = outcome.charge();
    assert_eq!(charge.source_event_count(), 2);
    assert_eq!(charge.aggregate_units(), 10);
    assert_eq!(charge.billable_units(), 8);
    assert_eq!(charge.amount_minor(), 24);
}

#[test]
fn fully_included_usage_produces_explicit_zero_charge() {
    let aggregate = count_aggregate("meter-1", "subject-1", 2);
    let mut registry = RatingRegistry::new();
    let charge = registry
        .rate(request(
            "rating-zero",
            aggregate,
            plan("meter-1", 500, 10),
            "customer-1",
        ))
        .unwrap()
        .charge()
        .clone();
    assert_eq!(charge.aggregate_units(), 2);
    assert_eq!(charge.billable_units(), 0);
    assert_eq!(charge.amount_minor(), 0);
    assert_eq!(registry.event_count(), 1);
}

#[test]
fn meter_mismatch_rejects_without_reserving_and_same_event_can_retry() {
    let aggregate = count_aggregate("meter-1", "subject-1", 2);
    let mut registry = RatingRegistry::new();
    let bad = request(
        "rating-retry",
        aggregate.clone(),
        plan("meter-other", 10, 0),
        "customer-1",
    );
    assert!(matches!(
        registry.rate(bad),
        Err(RatingError::MeterMismatch { .. })
    ));
    assert_eq!(registry.event_count(), 0);
    assert_eq!(registry.rating_key_count(), 0);

    let corrected = request(
        "rating-retry",
        aggregate,
        plan("meter-1", 10, 0),
        "customer-1",
    );
    assert!(matches!(
        registry.rate(corrected).unwrap(),
        RatingOutcome::Rated { .. }
    ));
    assert_eq!(registry.event_count(), 1);
}

#[test]
fn usage_window_must_be_fully_inside_plan_interval() {
    let aggregate = count_aggregate("meter-1", "subject-1", 1);
    let plan_id = RatingPlanId::new("plan-window").unwrap();
    let before = RatePlan::new(
        plan_id.clone(),
        MeterId::new("meter-1").unwrap(),
        usd(),
        1,
        0,
        1,
        Some(DAY + 1),
    )
    .unwrap();
    let ending_early = RatePlan::new(
        plan_id,
        MeterId::new("meter-1").unwrap(),
        usd(),
        1,
        0,
        0,
        Some(DAY - 1),
    )
    .unwrap();

    let mut registry = RatingRegistry::new();
    for (event, rate_plan) in [("before", before), ("ending-early", ending_early)] {
        let result = registry.rate(request(event, aggregate.clone(), rate_plan, "customer-1"));
        assert!(matches!(result, Err(RatingError::WindowOutsidePlan { .. })));
    }
    assert_eq!(registry.event_count(), 0);
    assert_eq!(registry.rating_key_count(), 0);
}

#[test]
fn multiplication_overflow_fails_closed_without_reserving_identity() {
    let aggregate = sum_aggregate("meter-sum", "subject-1", &[i128::MAX]);
    let mut registry = RatingRegistry::new();
    let result = registry.rate(request(
        "rating-overflow",
        aggregate,
        plan("meter-sum", 2, 0),
        "customer-1",
    ));
    assert_eq!(result, Err(RatingError::ArithmeticOverflow));
    assert_eq!(registry.event_count(), 0);
    assert_eq!(registry.rating_key_count(), 0);
}

#[test]
fn exact_replay_returns_same_charge_without_duplicate_effect() {
    let aggregate = count_aggregate("meter-1", "subject-1", 3);
    let req = request(
        "rating-replay",
        aggregate,
        plan("meter-1", 7, 0),
        "customer-1",
    );
    let mut registry = RatingRegistry::new();
    let first = registry.rate(req.clone()).unwrap();
    let second = registry.rate(req).unwrap();

    assert!(matches!(first, RatingOutcome::Rated { .. }));
    assert!(matches!(second, RatingOutcome::Replayed { .. }));
    assert_eq!(first.charge(), second.charge());
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.rating_key_count(), 1);
    assert_eq!(
        registry
            .charge_for_event(&RatingEventId::new("rating-replay").unwrap())
            .unwrap(),
        first.charge()
    );
}

#[test]
fn conflicting_event_id_fails_closed() {
    let aggregate = count_aggregate("meter-1", "subject-1", 1);
    let mut registry = RatingRegistry::new();
    registry
        .rate(request(
            "rating-conflict",
            aggregate.clone(),
            plan("meter-1", 10, 0),
            "customer-1",
        ))
        .unwrap();

    let conflict = request(
        "rating-conflict",
        aggregate,
        plan("meter-1", 11, 0),
        "customer-1",
    );
    assert_eq!(
        registry.rate(conflict),
        Err(RatingError::RatingEventConflict(
            RatingEventId::new("rating-conflict").unwrap()
        ))
    );
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.rating_key_count(), 1);
}

#[test]
fn same_meter_subject_window_and_plan_cannot_create_second_history() {
    let aggregate = count_aggregate("meter-1", "subject-1", 2);
    let mut registry = RatingRegistry::new();
    registry
        .rate(request(
            "rating-first",
            aggregate.clone(),
            plan("meter-1", 10, 0),
            "customer-1",
        ))
        .unwrap();

    let duplicate = request(
        "rating-second",
        aggregate,
        plan("meter-1", 10, 0),
        "customer-2",
    );
    assert_eq!(
        registry.rate(duplicate),
        Err(RatingError::DuplicateRatingKey {
            existing_event_id: RatingEventId::new("rating-first").unwrap(),
        })
    );
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.rating_key_count(), 1);
}
