#![allow(clippy::unwrap_used)]
use cofi_billing::BillingCustomerId;
use cofi_ledger::Currency;
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageEvent,
    UsageEventId, UsageValue, WindowSize,
};
use cofi_rating::{
    RatePlan, RatedCharge, RatingEventId, RatingPlanId, RatingRegistry, RatingRequest,
};
use cofi_storage::CodecError;
use cofi_storage::rating::{
    RatingAcceptance, decode_accepted_rating_request, decode_rate_plan, encode_rate_plan,
    encode_rating_acceptance, replay_rating_acceptances,
};

fn meter() -> MeterDefinition {
    MeterDefinition::new(
        MeterId::new("messages").unwrap(),
        EventType::new("tokens").unwrap(),
        Aggregation::Sum,
        WindowSize::Minute,
        Some(0),
    )
    .unwrap()
}
fn plan(price: i128) -> RatePlan {
    RatePlan::new(
        RatingPlanId::new("plan-v1").unwrap(),
        MeterId::new("messages").unwrap(),
        Currency::new("SAR").unwrap(),
        price,
        2,
        0,
        None,
    )
    .unwrap()
}
fn usage(id: &str, amount: i128, observed: i64) -> UsageEvent {
    UsageEvent::new(
        UsageEventId::new(id).unwrap(),
        MeterId::new("messages").unwrap(),
        EventType::new("tokens").unwrap(),
        SubjectId::new("customer-subject").unwrap(),
        10_000,
        observed,
        UsageValue::Sum(amount),
    )
    .unwrap()
}
fn charge_for(
    meter: &MeterDefinition,
    events: &[UsageEvent],
    p: &RatePlan,
    id: &RatingEventId,
    customer: &BillingCustomerId,
) -> RatedCharge {
    let mut engine = MeteringEngine::new();
    engine.register_meter(meter.clone()).unwrap();
    for event in events {
        engine.ingest(event.clone()).unwrap();
    }
    let aggregate = engine
        .aggregate(
            meter.id(),
            &SubjectId::new("customer-subject").unwrap(),
            0,
            60_000,
        )
        .unwrap();
    let request =
        RatingRequest::new(id.clone(), aggregate, customer.clone(), p.clone(), 70_000).unwrap();
    RatingRegistry::new()
        .rate(request)
        .unwrap()
        .charge()
        .clone()
}
#[test]
fn checked_plan_roundtrip_and_bad_numeric() {
    let original = plan(i128::MAX);
    let bytes = encode_rate_plan(&original).unwrap();
    assert_eq!(decode_rate_plan(&bytes).unwrap(), original);
    let mut tampered: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    tampered["payload"]["unit_price_minor"] = serde_json::json!(1.5);
    assert!(decode_rate_plan(&serde_json::to_vec(&tampered).unwrap()).is_err());
    tampered["payload"]["unit_price_minor"] = serde_json::json!("-1");
    assert!(decode_rate_plan(&serde_json::to_vec(&tampered).unwrap()).is_err());
}
#[test]
fn accepted_rating_keeps_historical_charge_after_late_usage() {
    let meter = meter();
    let source = [usage("first", 7, 20_000)];
    let plan = plan(3);
    let id = RatingEventId::new("rating-1").unwrap();
    let customer = BillingCustomerId::new("customer-1").unwrap();
    let charge = charge_for(&meter, &source, &plan, &id, &customer);
    assert_eq!(charge.amount_minor(), 15);
    let acceptance = RatingAcceptance {
        meter: &meter,
        events: &source,
        plan: &plan,
        rating_event_id: &id,
        subject_id: &SubjectId::new("customer-subject").unwrap(),
        billing_customer_id: &customer,
        window_start_unix_ms: 0,
        window_end_unix_ms: 60_000,
        rated_at_unix_ms: 70_000,
        accepted_charge: &charge,
    };
    let bytes = encode_rating_acceptance(&acceptance).unwrap();
    assert_eq!(
        charge_for(
            &meter,
            &[source[0].clone(), usage("late", 8, 90_000)],
            &plan,
            &id,
            &customer
        )
        .amount_minor(),
        39
    );
    let recovered = replay_rating_acceptances([bytes.as_slice()]).unwrap();
    assert_eq!(recovered.charge_for_event(&id), Some(&charge));
}
#[test]
fn tampering_source_or_result_fails_closed() {
    let meter = meter();
    let source = [usage("first", 7, 20_000)];
    let p = plan(3);
    let id = RatingEventId::new("rating-1").unwrap();
    let customer = BillingCustomerId::new("customer-1").unwrap();
    let charge = charge_for(&meter, &source, &p, &id, &customer);
    let bytes = encode_rating_acceptance(&RatingAcceptance {
        meter: &meter,
        events: &source,
        plan: &p,
        rating_event_id: &id,
        subject_id: &SubjectId::new("customer-subject").unwrap(),
        billing_customer_id: &customer,
        window_start_unix_ms: 0,
        window_end_unix_ms: 60_000,
        rated_at_unix_ms: 70_000,
        accepted_charge: &charge,
    })
    .unwrap();
    assert_eq!(
        replay_rating_acceptances([bytes.as_slice(), bytes.as_slice()])
            .unwrap()
            .event_count(),
        1
    );
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (path, value) in [
        ("/payload/charge/amount_minor", serde_json::json!("999")),
        ("/payload/plan/unit_price_minor", serde_json::json!("5")),
        (
            "/payload/usage_events/0/payload/value/quantity",
            serde_json::json!("6"),
        ),
        ("/payload/subject_id", serde_json::json!("wrong")),
        (
            "/payload/usage_events/0/payload/observed_at_unix_ms",
            serde_json::json!("80000"),
        ),
    ] {
        let mut tampered = original.clone();
        *tampered.pointer_mut(path).unwrap() = value;
        let changed = serde_json::to_vec(&tampered).unwrap();
        assert!(
            replay_rating_acceptances([changed.as_slice()]).is_err(),
            "{path}"
        );
        assert!(
            replay_rating_acceptances([bytes.as_slice(), changed.as_slice()]).is_err(),
            "{path}"
        );
    }
    let mut missing = original.clone();
    missing["payload"]["usage_events"] = serde_json::json!([]);
    assert!(replay_rating_acceptances([serde_json::to_vec(&missing).unwrap().as_slice()]).is_err());
    let mut duplicate = original;
    let repeated = duplicate["payload"]["usage_events"][0].clone();
    duplicate["payload"]["usage_events"]
        .as_array_mut()
        .unwrap()
        .push(repeated);
    assert!(
        replay_rating_acceptances([serde_json::to_vec(&duplicate).unwrap().as_slice()]).is_err()
    );
}
#[test]
fn rating_arithmetic_overflow_fails_closed() {
    let meter = meter();
    let mut engine = MeteringEngine::new();
    engine.register_meter(meter.clone()).unwrap();
    engine.ingest(usage("huge", i128::MAX, 20_000)).unwrap();
    let aggregate = engine
        .aggregate(
            meter.id(),
            &SubjectId::new("customer-subject").unwrap(),
            0,
            60_000,
        )
        .unwrap();
    let request = RatingRequest::new(
        RatingEventId::new("overflow").unwrap(),
        aggregate,
        BillingCustomerId::new("c").unwrap(),
        plan(i128::MAX),
        70_000,
    )
    .unwrap();
    assert!(RatingRegistry::new().rate(request).is_err());
}

#[test]
fn same_rating_identity_cannot_swap_equal_value_source_membership() {
    let meter = meter();
    let source = [usage("first", 7, 20_000)];
    let p = plan(3);
    let id = RatingEventId::new("rating-1").unwrap();
    let customer = BillingCustomerId::new("customer-1").unwrap();
    let charge = charge_for(&meter, &source, &p, &id, &customer);
    let original = encode_rating_acceptance(&RatingAcceptance {
        meter: &meter,
        events: &source,
        plan: &p,
        rating_event_id: &id,
        subject_id: &SubjectId::new("customer-subject").unwrap(),
        billing_customer_id: &customer,
        window_start_unix_ms: 0,
        window_end_unix_ms: 60_000,
        rated_at_unix_ms: 70_000,
        accepted_charge: &charge,
    })
    .unwrap();
    let mut edited: serde_json::Value = serde_json::from_slice(&original).unwrap();
    edited["payload"]["usage_events"][0]["payload"]["id"] =
        serde_json::json!("different-source-id");
    let other = serde_json::to_vec(&edited).unwrap();
    // Same total and charge does not entitle a reissued rating event to
    // silently replace its historical source identities.
    assert!(replay_rating_acceptances([original.as_slice(), other.as_slice()]).is_err());

    edited["payload"]["rating_event_id"] = serde_json::json!("rating-2");
    edited["payload"]["charge"]["rating_event_id"] = serde_json::json!("rating-2");
    edited["payload"]["charge"]["id"] = serde_json::json!("rating:rating-2:charge");
    let other_id = serde_json::to_vec(&edited).unwrap();
    // The canonical rating key rejects a second rating for that same
    // subject/meter/window/plan, even with a different event identity.
    assert!(replay_rating_acceptances([original.as_slice(), other_id.as_slice()]).is_err());
}

#[test]
fn encoding_rejects_a_late_or_omitted_source_snapshot() {
    let meter = meter();
    let events = [usage("first", 7, 20_000)];
    let p = plan(3);
    let id = RatingEventId::new("rating-1").unwrap();
    let customer = BillingCustomerId::new("customer-1").unwrap();
    let expected = charge_for(&meter, &events, &p, &id, &customer);
    let late = [usage("late", 8, 90_000)];
    let wrong = RatingAcceptance {
        meter: &meter,
        events: &late,
        plan: &p,
        rating_event_id: &id,
        subject_id: &SubjectId::new("customer-subject").unwrap(),
        billing_customer_id: &customer,
        window_start_unix_ms: 0,
        window_end_unix_ms: 60_000,
        rated_at_unix_ms: 70_000,
        accepted_charge: &expected,
    };
    assert!(encode_rating_acceptance(&wrong).is_err());
    let missing = RatingAcceptance {
        events: &[],
        ..wrong
    };
    assert!(encode_rating_acceptance(&missing).is_err());
}

#[test]
fn rating_codec_rejects_oversize_original_plan_before_json_parse() {
    let original = plan(3);
    let mut bytes = encode_rate_plan(&original).unwrap();
    bytes.resize(1024 * 1024, b' ');
    assert_eq!(decode_rate_plan(&bytes), Ok(original));
    bytes.push(b' ');
    assert!(matches!(
        decode_rate_plan(&bytes),
        Err(CodecError::InvalidPayload(_))
    ));
}

#[test]
fn rating_codec_rejects_oversize_original_plan_encoding() {
    let original = RatePlan::new(
        RatingPlanId::new(format!("plan-{}", "x".repeat(1024 * 1024))).unwrap(),
        MeterId::new("messages").unwrap(),
        Currency::new("SAR").unwrap(),
        3,
        2,
        0,
        None,
    )
    .unwrap();
    assert!(matches!(
        encode_rate_plan(&original),
        Err(CodecError::InvalidPayload(_))
    ));
}

#[test]
fn rating_codec_rejects_oversize_original_accepted_rating_source() {
    let definition = meter();
    let events = [usage("first", 7, 20_000)];
    let plan = plan(3);
    let id = RatingEventId::new("rating-1").unwrap();
    let customer = BillingCustomerId::new("customer-1").unwrap();
    let charge = charge_for(&definition, &events, &plan, &id, &customer);
    let original = encode_rating_acceptance(&RatingAcceptance {
        meter: &definition,
        events: &events,
        plan: &plan,
        rating_event_id: &id,
        subject_id: &SubjectId::new("customer-subject").unwrap(),
        billing_customer_id: &customer,
        window_start_unix_ms: 0,
        window_end_unix_ms: 60_000,
        rated_at_unix_ms: 70_000,
        accepted_charge: &charge,
    })
    .unwrap();
    let mut bytes = original;
    bytes.resize(1024 * 1024, b' ');
    assert!(decode_accepted_rating_request(&bytes).is_ok());
    assert!(replay_rating_acceptances([bytes.as_slice()]).is_ok());
    bytes.push(b' ');
    assert!(matches!(
        decode_accepted_rating_request(&bytes),
        Err(CodecError::InvalidPayload(_))
    ));
    assert!(matches!(
        replay_rating_acceptances([bytes.as_slice()]),
        Err(CodecError::InvalidPayload(_))
    ));
}

#[test]
fn rating_codec_rejects_oversize_original_accepted_rating_encoding() {
    let definition = meter();
    let events = [usage("first", 7, 20_000)];
    let price_plan = plan(3);
    let id = RatingEventId::new(format!("rating-{}", "x".repeat(1024 * 1024))).unwrap();
    let customer = BillingCustomerId::new("customer-1").unwrap();
    let charge = charge_for(&definition, &events, &price_plan, &id, &customer);
    assert!(matches!(
        encode_rating_acceptance(&RatingAcceptance {
            meter: &definition,
            events: &events,
            plan: &price_plan,
            rating_event_id: &id,
            subject_id: &SubjectId::new("customer-subject").unwrap(),
            billing_customer_id: &customer,
            window_start_unix_ms: 0,
            window_end_unix_ms: 60_000,
            rated_at_unix_ms: 70_000,
            accepted_charge: &charge,
        }),
        Err(CodecError::InvalidPayload(_))
    ));
}
