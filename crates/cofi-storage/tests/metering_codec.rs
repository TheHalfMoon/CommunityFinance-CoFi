#![allow(clippy::unwrap_used)]

use cofi_metering::{
    Aggregation, EventType, IngestionOutcome, MeterDefinition, MeterId, MeteringEngine,
    RegistrationOutcome, SubjectId, UsageEvent, UsageEventId, UsageValue,
    WindowSize,
};
use cofi_storage::metering::{
    MeteringFact, decode_metering_fact, encode_meter_definition, encode_usage_event,
    replay_metering,
};
use cofi_storage::CodecError;

fn meter(id: &str, aggregation: Aggregation, active_from: Option<i64>) -> MeterDefinition {
    MeterDefinition::new(
        MeterId::new(id).unwrap(),
        EventType::new("tokens").unwrap(),
        aggregation,
        WindowSize::Minute,
        active_from,
    )
    .unwrap()
}

fn event(id: &str, meter_id: &str, amount: i128) -> UsageEvent {
    UsageEvent::new(
        UsageEventId::new(id).unwrap(),
        MeterId::new(meter_id).unwrap(),
        EventType::new("tokens").unwrap(),
        SubjectId::new("subject-one").unwrap(),
        60_000,
        60_001,
        UsageValue::Sum(amount),
    )
    .unwrap()
}

#[test]
fn roundtrip_preserves_meter_definition_and_usage_exactly() {
    let definition = meter("meter-1", Aggregation::Sum, Some(60_000));
    let usage = event("usage-1", "meter-1", i128::MAX);
    let definition_encoded = encode_meter_definition(&definition).unwrap();
    let event_encoded = encode_usage_event(&usage).unwrap();

    let decoded_definition = decode_metering_fact(&definition_encoded).unwrap();
    let decoded_event = decode_metering_fact(&event_encoded).unwrap();
    assert!(matches!(decoded_definition, MeteringFact::Definition(_)));
    assert!(matches!(decoded_event, MeteringFact::Event(_)));
    if let MeteringFact::Definition(actual) = decoded_definition {
        assert_eq!(actual, definition);
    }
    if let MeteringFact::Event(actual) = decoded_event {
        assert_eq!(actual, usage);
    }

    // Money/usage values must never silently round through JSON floating point.
    let json: serde_json::Value = serde_json::from_slice(&event_encoded).unwrap();
    assert_eq!(
        json["payload"]["value"]["quantity"],
        i128::MAX.to_string()
    );
}

#[test]
fn replay_preserves_aggregate_and_exact_source_replay() {
    let definition = meter("meter-1", Aggregation::Sum, Some(60_000));
    let usage = event("usage-1", "meter-1", i128::MAX);
    let facts = [
        encode_meter_definition(&definition).unwrap(),
        encode_usage_event(&usage).unwrap(),
    ];

    let mut reference = MeteringEngine::new();
    assert_eq!(reference.register_meter(definition.clone()), Ok(RegistrationOutcome::Registered));
    assert_eq!(reference.ingest(usage.clone()), Ok(IngestionOutcome::Accepted));

    let mut recovered = replay_metering(facts.iter().map(Vec::as_slice)).unwrap();
    assert_eq!(recovered.meter_count(), reference.meter_count());
    assert_eq!(recovered.event_count(), reference.event_count());
    assert_eq!(recovered.meter(definition.id()), reference.meter(definition.id()));
    assert_eq!(recovered.event(usage.id()), reference.event(usage.id()));
    assert_eq!(
        recovered.aggregate(
            definition.id(), usage.subject_id(), 60_000, 120_000
        ),
        reference.aggregate(
            definition.id(), usage.subject_id(), 60_000, 120_000
        ),
    );
    assert_eq!(
        recovered.register_meter(definition),
        Ok(RegistrationOutcome::Replayed)
    );
    assert_eq!(recovered.ingest(usage), Ok(IngestionOutcome::Replayed));
}

#[test]
fn count_shape_roundtrip_remains_count_not_sum() {
    let definition = meter("count-meter", Aggregation::Count, None);
    let usage = UsageEvent::new(
        UsageEventId::new("count-event").unwrap(),
        definition.id().clone(),
        EventType::new("tokens").unwrap(),
        SubjectId::new("subject-one").unwrap(),
        0,
        0,
        UsageValue::Count,
    )
    .unwrap();
    let facts = [
        encode_meter_definition(&definition).unwrap(),
        encode_usage_event(&usage).unwrap(),
    ];
    let recovered = replay_metering(facts.iter().map(Vec::as_slice)).unwrap();
    let agg = recovered.aggregate(definition.id(), usage.subject_id(), 0, 60_000).unwrap();
    assert_eq!(agg.event_count(), 1);
    assert_eq!(agg.aggregate_value(), 1);
}

#[test]
fn changed_duplicate_event_and_meter_are_rejected() {
    let def = meter("meter-1", Aggregation::Sum, None);
    let usage = event("usage-1", "meter-1", 20);
    let changed_usage = event("usage-1", "meter-1", 21);
    let facts = [
        encode_meter_definition(&def).unwrap(),
        encode_usage_event(&usage).unwrap(),
        encode_usage_event(&changed_usage).unwrap(),
    ];
    assert!(matches!(
        replay_metering(facts.iter().map(Vec::as_slice)),
        Err(CodecError::Replay(_))
    ));

    let changed_def = meter("meter-1", Aggregation::Count, None);
    let facts = [
        encode_meter_definition(&def).unwrap(),
        encode_meter_definition(&changed_def).unwrap(),
    ];
    assert!(matches!(
        replay_metering(facts.iter().map(Vec::as_slice)),
        Err(CodecError::Replay(_))
    ));
}

#[test]
fn missing_or_inactive_meter_is_rejected() {
    let usage = event("usage-1", "missing", 1);
    let bytes = encode_usage_event(&usage).unwrap();
    assert!(matches!(
        replay_metering([bytes.as_slice()]),
        Err(CodecError::Replay(_))
    ));

    let future_meter = meter("meter-1", Aggregation::Sum, Some(120_000));
    let usage = event("usage-1", "meter-1", 1);
    let facts = [
        encode_meter_definition(&future_meter).unwrap(),
        encode_usage_event(&usage).unwrap(),
    ];
    assert!(matches!(
        replay_metering(facts.iter().map(Vec::as_slice)),
        Err(CodecError::Replay(_))
    ));
}

#[test]
fn malformed_values_types_and_versions_fail_closed() {
    let definition = meter("meter-1", Aggregation::Sum, None);
    let mut value: serde_json::Value =
        serde_json::from_slice(&encode_meter_definition(&definition).unwrap()).unwrap();
    value["schema_version"] = serde_json::json!(2);
    assert!(matches!(
        decode_metering_fact(&serde_json::to_vec(&value).unwrap()),
        Err(CodecError::UnsupportedVersion(2))
    ));
    value["schema_version"] = serde_json::json!(1);
    value["record_type"] = serde_json::json!("unknown");
    assert!(matches!(
        decode_metering_fact(&serde_json::to_vec(&value).unwrap()),
        Err(CodecError::UnsupportedRecordKind(_))
    ));
    value["record_type"] = serde_json::json!("meter.definition");
    value["payload"]["extra"] = serde_json::json!(1);
    assert!(matches!(
        decode_metering_fact(&serde_json::to_vec(&value).unwrap()),
        Err(CodecError::InvalidPayload(_))
    ));

    let usage = event("usage-1", "meter-1", 1);
    let original: serde_json::Value =
        serde_json::from_slice(&encode_usage_event(&usage).unwrap()).unwrap();
    for malformed in ["01", "1e3", "1.0", "+1", "-1", "170141183460469231731687303715884105728"] {
        let mut changed = original.clone();
        changed["payload"]["value"]["quantity"] = serde_json::json!(malformed);
        assert!(decode_metering_fact(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
    let mut nonstring = original.clone();
    nonstring["payload"]["value"]["quantity"] = serde_json::json!(1.1);
    assert!(matches!(
        decode_metering_fact(&serde_json::to_vec(&nonstring).unwrap()),
        Err(CodecError::InvalidPayload(_))
    ));
    let mut bad_id = original.clone();
    bad_id["payload"]["subject_id"] = serde_json::json!(" ");
    assert!(matches!(
        decode_metering_fact(&serde_json::to_vec(&bad_id).unwrap()),
        Err(CodecError::InvalidDomain(_))
    ));
    let mut bad_time = original.clone();
    bad_time["payload"]["observed_at_unix_ms"] = serde_json::json!("-1");
    assert!(matches!(
        decode_metering_fact(&serde_json::to_vec(&bad_time).unwrap()),
        Err(CodecError::InvalidDomain(_))
    ));
    let mut mismatch = original.clone();
    mismatch["payload"]["value"]["kind"] = serde_json::json!("count");
    assert!(matches!(
        decode_metering_fact(&serde_json::to_vec(&mismatch).unwrap()),
        Err(CodecError::InvalidPayload(_))
    ));
}
