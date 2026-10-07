#![allow(clippy::unwrap_used)]

use cofi_billing::{BillingCustomerId, BillingEventId, BillingInvoiceId};
use cofi_invoicing::{DraftAssemblyOutcome, DraftInvoiceRegistry, DraftInvoiceRequest};
use cofi_ledger::{Currency, LedgerScopeId};
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageEvent,
    UsageEventId, UsageValue, WindowSize,
};
use cofi_rating::{RatePlan, RatingEventId, RatingPlanId, RatingRegistry, RatingRequest};
use cofi_storage::invoice::{decode_draft_invoice, encode_draft_invoice, replay_draft_invoices};
use cofi_storage::rating::{RatingAcceptance, encode_rating_acceptance};

fn rating(id: &str, units: i128) -> (cofi_rating::RatedCharge, Vec<u8>) {
    let meter = MeterDefinition::new(
        MeterId::new(format!("meter-{id}")).unwrap(),
        EventType::new("usage").unwrap(),
        Aggregation::Sum,
        WindowSize::Minute,
        Some(0),
    )
    .unwrap();
    let event = UsageEvent::new(
        UsageEventId::new(format!("usage-{id}")).unwrap(),
        meter.id().clone(),
        EventType::new("usage").unwrap(),
        SubjectId::new("subject-1").unwrap(),
        15_000,
        16_000,
        UsageValue::Sum(units),
    )
    .unwrap();
    let plan = RatePlan::new(
        RatingPlanId::new(format!("plan-{id}")).unwrap(),
        meter.id().clone(),
        Currency::new("SAR").unwrap(),
        3,
        0,
        0,
        Some(60_000),
    )
    .unwrap();
    let id = RatingEventId::new(id).unwrap();
    let customer = BillingCustomerId::new("customer-1").unwrap();
    let mut engine = MeteringEngine::new();
    engine.register_meter(meter.clone()).unwrap();
    engine.ingest(event.clone()).unwrap();
    let aggregate = engine
        .aggregate(meter.id(), &SubjectId::new("subject-1").unwrap(), 0, 60_000)
        .unwrap();
    let request = RatingRequest::new(
        id.clone(),
        aggregate,
        customer.clone(),
        plan.clone(),
        70_000,
    )
    .unwrap();
    let charge = RatingRegistry::new()
        .rate(request)
        .unwrap()
        .charge()
        .clone();
    let bytes = encode_rating_acceptance(&RatingAcceptance {
        meter: &meter,
        events: &[event],
        plan: &plan,
        rating_event_id: &id,
        subject_id: &SubjectId::new("subject-1").unwrap(),
        billing_customer_id: &customer,
        window_start_unix_ms: 0,
        window_end_unix_ms: 60_000,
        rated_at_unix_ms: 70_000,
        accepted_charge: &charge,
    })
    .unwrap();
    (charge, bytes)
}
fn request(id: &str, charges: Vec<cofi_rating::RatedCharge>) -> DraftInvoiceRequest {
    DraftInvoiceRequest::new(
        BillingEventId::new(format!("draft-event-{id}")).unwrap(),
        BillingInvoiceId::new(format!("invoice-{id}")).unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        0,
        100_000,
        100_000,
        charges,
    )
    .unwrap()
}

#[test]
fn original_invoice_lines_totals_and_derived_bindings_roundtrip() {
    let (a, ra) = rating("a", 7);
    let (b, rb) = rating("b", 8);
    let req = request("1", vec![b, a]); // domain sorts by charge identity
    assert_eq!(req.total_minor(), 45);
    let encoded = encode_draft_invoice(&req, &[rb, ra]).unwrap();
    assert_eq!(decode_draft_invoice(&encoded).unwrap(), req);
    let mut reference = DraftInvoiceRegistry::new();
    let created = reference.assemble(req.clone()).unwrap().draft().clone();
    let mut recovered = replay_draft_invoices([encoded.as_slice()]).unwrap();
    assert_eq!(recovered.event_count(), reference.event_count());
    assert_eq!(recovered.invoice_count(), reference.invoice_count());
    assert_eq!(
        recovered.charge_binding_count(),
        reference.charge_binding_count()
    );
    assert_eq!(
        recovered.draft_for_event(req.source_event_id()),
        Some(&created)
    );
    assert_eq!(
        recovered
            .draft_for_event(req.source_event_id())
            .unwrap()
            .to_billing_event(),
        created.to_billing_event()
    );
    assert!(matches!(
        recovered.assemble(req),
        Ok(DraftAssemblyOutcome::Replayed { .. })
    ));
}

#[test]
fn duplicate_receipt_or_missing_line_fails_closed() {
    let (a, ra) = rating("a", 7);
    let (b, rb) = rating("b", 8);
    let req = request("1", vec![a, b]);
    assert!(encode_draft_invoice(&req, std::slice::from_ref(&ra)).is_err());
    assert!(encode_draft_invoice(&req, &[ra.clone(), ra]).is_err());
    assert!(encode_draft_invoice(&req, &[rb]).is_err());
}

#[test]
fn corrupted_receipt_scope_and_total_fail_closed() {
    let (charge, receipt) = rating("a", 7);
    let req = request("1", vec![charge]);
    let encoded = encode_draft_invoice(&req, &[receipt]).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    for (path, v) in [
        ("/schema_version", serde_json::json!(2)),
        ("/payload/organization_scope", serde_json::json!("")),
        (
            "/payload/customer_id",
            serde_json::json!("another-customer"),
        ),
        ("/payload/expected_total_minor", serde_json::json!("22")),
        ("/payload/period_end_unix_ms", serde_json::json!("1")),
        ("/payload/observed_at_unix_ms", serde_json::json!("60000")),
        (
            "/payload/ratings/0/evidence/payload/charge/amount_minor",
            serde_json::json!("999"),
        ),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(path).unwrap() = v;
        assert!(
            decode_draft_invoice(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{path}"
        );
    }
    let mut extra = original;
    extra["payload"]["unknown"] = serde_json::json!(true);
    assert!(decode_draft_invoice(&serde_json::to_vec(&extra).unwrap()).is_err());
}

#[test]
fn conflicting_draft_id_invoice_id_and_rebound_charge_fail_closed() {
    let (a, ra) = rating("a", 7);
    let first = request("1", vec![a.clone()]);
    let encoded = encode_draft_invoice(&first, std::slice::from_ref(&ra)).unwrap();
    assert_eq!(
        replay_draft_invoices([encoded.as_slice(), encoded.as_slice()])
            .unwrap()
            .invoice_count(),
        1
    );
    let second = request("2", vec![a]);
    let other = encode_draft_invoice(&second, &[ra]).unwrap();
    assert!(replay_draft_invoices([encoded.as_slice(), other.as_slice()]).is_err());
    let mut changed: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    changed["payload"]["invoice_id"] = serde_json::json!("new-invoice");
    let other = serde_json::to_vec(&changed).unwrap();
    assert!(replay_draft_invoices([encoded.as_slice(), other.as_slice()]).is_err());
}
