#![allow(clippy::unwrap_used)]

use cofi_metering::{
    Aggregation, EventType, IngestionOutcome, MeterDefinition, MeterId, MeteringEngine,
    MeteringError, RegistrationOutcome, SubjectId, UsageEvent, UsageEventId, UsageValue,
    WindowSize,
};

fn meter_id(value: &str) -> MeterId {
    MeterId::new(value).unwrap()
}

fn event_id(value: &str) -> UsageEventId {
    UsageEventId::new(value).unwrap()
}

fn subject(value: &str) -> SubjectId {
    SubjectId::new(value).unwrap()
}

fn event_type(value: &str) -> EventType {
    EventType::new(value).unwrap()
}

fn meter(
    id: &str,
    event: &str,
    aggregation: Aggregation,
    window: WindowSize,
    active_from: Option<i64>,
) -> MeterDefinition {
    MeterDefinition::new(
        meter_id(id),
        event_type(event),
        aggregation,
        window,
        active_from,
    )
    .unwrap()
}

fn usage(
    id: &str,
    meter: &str,
    event: &str,
    subject_id: &str,
    occurred_at: i64,
    observed_at: i64,
    value: UsageValue,
) -> UsageEvent {
    UsageEvent::new(
        event_id(id),
        meter_id(meter),
        event_type(event),
        subject(subject_id),
        occurred_at,
        observed_at,
        value,
    )
    .unwrap()
}
#[test]
fn identifiers_and_temporal_constructors_fail_closed() {
    assert!(MeterId::new("").is_err());
    assert!(UsageEventId::new("   ").is_err());
    assert!(SubjectId::new("").is_err());
    assert!(EventType::new(" ").is_err());
    assert_eq!(
        MeterDefinition::new(
            meter_id("m"),
            event_type("evt"),
            Aggregation::Count,
            WindowSize::Minute,
            Some(-1),
        )
        .unwrap_err(),
        MeteringError::InvalidTimestamp("active_from_unix_ms")
    );
    assert!(matches!(
        UsageEvent::new(
            event_id("e"),
            meter_id("m"),
            event_type("evt"),
            subject("s"),
            10,
            9,
            UsageValue::Count,
        ),
        Err(MeteringError::ObservedBeforeOccurrence { .. })
    ));
}
#[test]
fn sum_constructor_requires_positive_value() {
    assert_eq!(
        UsageEvent::new(
            event_id("e"),
            meter_id("m"),
            event_type("evt"),
            subject("s"),
            0,
            0,
            UsageValue::Sum(0),
        )
        .unwrap_err(),
        MeteringError::NonPositiveUsageValue
    );
    assert_eq!(
        UsageEvent::new(
            event_id("e2"),
            meter_id("m"),
            event_type("evt"),
            subject("s"),
            0,
            0,
            UsageValue::Sum(-1),
        )
        .unwrap_err(),
        MeteringError::NonPositiveUsageValue
    );
}
#[test]
fn meter_registration_replay_and_conflict_are_deterministic() {
    let mut engine = MeteringEngine::new();
    let definition = meter(
        "requests",
        "request.completed",
        Aggregation::Count,
        WindowSize::Minute,
        None,
    );
    assert_eq!(
        engine.register_meter(definition.clone()).unwrap(),
        RegistrationOutcome::Registered
    );
    assert_eq!(
        engine.register_meter(definition).unwrap(),
        RegistrationOutcome::Replayed
    );
    assert_eq!(engine.meter_count(), 1);
    let conflict = meter(
        "requests",
        "request.completed",
        Aggregation::Sum,
        WindowSize::Minute,
        None,
    );
    assert!(matches!(
        engine.register_meter(conflict),
        Err(MeteringError::MeterIdConflict(_))
    ));
    assert_eq!(engine.meter_count(), 1);
}
#[test]
fn exact_event_replay_is_idempotent_and_conflict_fails_closed() {
    let mut engine = MeteringEngine::new();
    engine
        .register_meter(meter(
            "requests",
            "request.completed",
            Aggregation::Count,
            WindowSize::Minute,
            None,
        ))
        .unwrap();
    let event = usage(
        "evt-1",
        "requests",
        "request.completed",
        "customer-1",
        1_000,
        1_001,
        UsageValue::Count,
    );
    assert_eq!(
        engine.ingest(event.clone()).unwrap(),
        IngestionOutcome::Accepted
    );
    assert_eq!(engine.ingest(event).unwrap(), IngestionOutcome::Replayed);
    assert_eq!(engine.event_count(), 1);
    let conflict = usage(
        "evt-1",
        "requests",
        "request.completed",
        "customer-2",
        1_000,
        1_001,
        UsageValue::Count,
    );
    assert!(matches!(
        engine.ingest(conflict),
        Err(MeteringError::UsageEventIdConflict(_))
    ));
    assert_eq!(engine.event_count(), 1);
}
#[test]
fn unknown_meter_rejection_does_not_reserve_event_identity() {
    let mut engine = MeteringEngine::new();
    let event = usage(
        "evt-1",
        "requests",
        "request.completed",
        "customer-1",
        1_000,
        1_001,
        UsageValue::Count,
    );
    assert!(matches!(
        engine.ingest(event.clone()),
        Err(MeteringError::UnknownMeter(_))
    ));
    assert_eq!(engine.event_count(), 0);
    engine
        .register_meter(meter(
            "requests",
            "request.completed",
            Aggregation::Count,
            WindowSize::Minute,
            None,
        ))
        .unwrap();
    assert_eq!(engine.ingest(event).unwrap(), IngestionOutcome::Accepted);
    assert_eq!(engine.event_count(), 1);
}
#[test]
fn event_type_mismatch_does_not_reserve_event_identity() {
    let mut engine = MeteringEngine::new();
    engine
        .register_meter(meter(
            "requests",
            "request.completed",
            Aggregation::Count,
            WindowSize::Minute,
            None,
        ))
        .unwrap();
    let wrong = usage(
        "evt-1",
        "requests",
        "request.failed",
        "customer-1",
        1_000,
        1_001,
        UsageValue::Count,
    );
    assert_eq!(
        engine.ingest(wrong).unwrap_err(),
        MeteringError::EventTypeMismatch
    );
    assert_eq!(engine.event_count(), 0);
    let corrected = usage(
        "evt-1",
        "requests",
        "request.completed",
        "customer-1",
        1_000,
        1_001,
        UsageValue::Count,
    );
    assert_eq!(
        engine.ingest(corrected).unwrap(),
        IngestionOutcome::Accepted
    );
    assert_eq!(engine.event_count(), 1);
}
#[test]
fn pre_activation_event_fails_without_reserving_identity() {
    let mut engine = MeteringEngine::new();
    engine
        .register_meter(meter(
            "requests",
            "request.completed",
            Aggregation::Count,
            WindowSize::Minute,
            Some(60_000),
        ))
        .unwrap();
    let early = usage(
        "evt-1",
        "requests",
        "request.completed",
        "customer-1",
        59_999,
        60_000,
        UsageValue::Count,
    );
    assert_eq!(
        engine.ingest(early).unwrap_err(),
        MeteringError::EventBeforeActivation
    );
    assert_eq!(engine.event_count(), 0);
    let valid = usage(
        "evt-1",
        "requests",
        "request.completed",
        "customer-1",
        60_000,
        60_000,
        UsageValue::Count,
    );
    assert_eq!(engine.ingest(valid).unwrap(), IngestionOutcome::Accepted);
    assert_eq!(engine.event_count(), 1);
}
#[test]
fn aggregation_value_shape_is_enforced_without_reserving_identity() {
    let mut engine = MeteringEngine::new();
    engine
        .register_meter(meter(
            "count-meter",
            "evt",
            Aggregation::Count,
            WindowSize::Minute,
            None,
        ))
        .unwrap();
    engine
        .register_meter(meter(
            "sum-meter",
            "evt",
            Aggregation::Sum,
            WindowSize::Minute,
            None,
        ))
        .unwrap();

    let numeric_count = usage(
        "count-event",
        "count-meter",
        "evt",
        "s",
        0,
        0,
        UsageValue::Sum(5),
    );
    assert_eq!(
        engine.ingest(numeric_count).unwrap_err(),
        MeteringError::UsageShapeMismatch
    );
    let missing_sum = usage(
        "sum-event",
        "sum-meter",
        "evt",
        "s",
        0,
        0,
        UsageValue::Count,
    );
    assert_eq!(
        engine.ingest(missing_sum).unwrap_err(),
        MeteringError::UsageShapeMismatch
    );
    assert_eq!(engine.event_count(), 0);

    assert_eq!(
        engine
            .ingest(usage(
                "count-event",
                "count-meter",
                "evt",
                "s",
                0,
                0,
                UsageValue::Count
            ))
            .unwrap(),
        IngestionOutcome::Accepted
    );
}
#[test]
fn fixed_window_durations_and_alignment_are_deterministic() {
    assert_eq!(WindowSize::Minute.duration_ms(), 60_000);
    assert_eq!(WindowSize::Hour.duration_ms(), 3_600_000);
    assert_eq!(WindowSize::Day.duration_ms(), 86_400_000);

    assert!(WindowSize::Minute.is_aligned(0));
    assert!(WindowSize::Minute.is_aligned(60_000));
    assert!(!WindowSize::Minute.is_aligned(1));
    assert!(WindowSize::Hour.is_aligned(7_200_000));
    assert!(!WindowSize::Hour.is_aligned(60_000));
    assert!(WindowSize::Day.is_aligned(86_400_000));
    assert!(!WindowSize::Day.is_aligned(-86_400_000));
}
#[test]
fn count_query_is_half_open_and_subject_scoped() {
    let mut engine = MeteringEngine::new();
    engine
        .register_meter(meter(
            "requests",
            "evt",
            Aggregation::Count,
            WindowSize::Minute,
            None,
        ))
        .unwrap();
    for event in [
        usage("e0", "requests", "evt", "s1", 0, 0, UsageValue::Count),
        usage(
            "e1",
            "requests",
            "evt",
            "s1",
            59_999,
            59_999,
            UsageValue::Count,
        ),
        usage(
            "e2",
            "requests",
            "evt",
            "s1",
            60_000,
            60_000,
            UsageValue::Count,
        ),
        usage(
            "e3",
            "requests",
            "evt",
            "s2",
            1_000,
            1_000,
            UsageValue::Count,
        ),
    ] {
        engine.ingest(event).unwrap();
    }
    let aggregate = engine
        .aggregate(&meter_id("requests"), &subject("s1"), 0, 60_000)
        .unwrap();
    assert_eq!(aggregate.event_count(), 2);
    assert_eq!(aggregate.aggregate_value(), 2);
    assert_eq!(aggregate.window_start_unix_ms(), 0);
    assert_eq!(aggregate.window_end_unix_ms(), 60_000);
    assert_eq!(aggregate.meter_id(), &meter_id("requests"));
    assert_eq!(aggregate.subject_id(), &subject("s1"));
}
#[test]
fn sum_query_uses_checked_integer_units() {
    let mut engine = MeteringEngine::new();
    engine
        .register_meter(meter(
            "tokens",
            "token.used",
            Aggregation::Sum,
            WindowSize::Hour,
            None,
        ))
        .unwrap();
    for event in [
        usage(
            "e1",
            "tokens",
            "token.used",
            "s1",
            1_000,
            1_000,
            UsageValue::Sum(7),
        ),
        usage(
            "e2",
            "tokens",
            "token.used",
            "s1",
            2_000,
            2_000,
            UsageValue::Sum(11),
        ),
        usage(
            "e3",
            "tokens",
            "token.used",
            "s2",
            3_000,
            3_000,
            UsageValue::Sum(100),
        ),
    ] {
        engine.ingest(event).unwrap();
    }
    let aggregate = engine
        .aggregate(&meter_id("tokens"), &subject("s1"), 0, 3_600_000)
        .unwrap();
    assert_eq!(aggregate.event_count(), 2);
    assert_eq!(aggregate.aggregate_value(), 18);
}
#[test]
fn aggregation_is_independent_of_ingestion_order() {
    let definition = meter(
        "tokens",
        "token.used",
        Aggregation::Sum,
        WindowSize::Minute,
        None,
    );
    let events = [
        usage("z", "tokens", "token.used", "s", 10, 10, UsageValue::Sum(3)),
        usage("a", "tokens", "token.used", "s", 20, 20, UsageValue::Sum(5)),
        usage("m", "tokens", "token.used", "s", 30, 30, UsageValue::Sum(7)),
    ];
    let mut first = MeteringEngine::new();
    let mut second = MeteringEngine::new();
    first.register_meter(definition.clone()).unwrap();
    second.register_meter(definition).unwrap();
    for event in events.clone() {
        first.ingest(event).unwrap();
    }
    for event in events.into_iter().rev() {
        second.ingest(event).unwrap();
    }
    assert_eq!(
        first
            .aggregate(&meter_id("tokens"), &subject("s"), 0, 60_000)
            .unwrap(),
        second
            .aggregate(&meter_id("tokens"), &subject("s"), 0, 60_000)
            .unwrap()
    );
}
#[test]
fn sum_overflow_fails_closed() {
    let mut engine = MeteringEngine::new();
    engine
        .register_meter(meter(
            "tokens",
            "token.used",
            Aggregation::Sum,
            WindowSize::Minute,
            None,
        ))
        .unwrap();
    engine
        .ingest(usage(
            "e1",
            "tokens",
            "token.used",
            "s",
            1,
            1,
            UsageValue::Sum(i128::MAX),
        ))
        .unwrap();
    engine
        .ingest(usage(
            "e2",
            "tokens",
            "token.used",
            "s",
            2,
            2,
            UsageValue::Sum(1),
        ))
        .unwrap();
    assert_eq!(
        engine
            .aggregate(&meter_id("tokens"), &subject("s"), 0, 60_000)
            .unwrap_err(),
        MeteringError::AggregationOverflow
    );
}
#[test]
fn query_validation_fails_closed() {
    let mut engine = MeteringEngine::new();
    engine
        .register_meter(meter(
            "requests",
            "evt",
            Aggregation::Count,
            WindowSize::Minute,
            None,
        ))
        .unwrap();

    assert!(matches!(
        engine.aggregate(&meter_id("missing"), &subject("s"), 0, 60_000),
        Err(MeteringError::UnknownMeter(_))
    ));
    assert_eq!(
        engine
            .aggregate(&meter_id("requests"), &subject("s"), 1, 60_000)
            .unwrap_err(),
        MeteringError::UnalignedWindow
    );
    assert_eq!(
        engine
            .aggregate(&meter_id("requests"), &subject("s"), 60_000, 60_000)
            .unwrap_err(),
        MeteringError::InvalidWindowRange {
            start_unix_ms: 60_000,
            end_unix_ms: 60_000,
        }
    );
}
