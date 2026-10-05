#![allow(clippy::unwrap_used)]

use cofi_billing::{
    BillingApplyOutcome, BillingCustomerId, BillingEventId, BillingInvoiceId,
    BillingLedgerAccounts, BillingLedgerBridge, InvoiceEvent, InvoiceStatus,
};
use cofi_invoicing::{
    DraftAssemblyOutcome, DraftInvoiceRegistry, DraftInvoiceRequest, InvoicingError,
};
use cofi_ledger::{AccountId, Currency, Ledger, LedgerScopeId};
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageEvent,
    UsageEventId, UsageValue, WindowSize,
};
use cofi_rating::{
    RatePlan, RatedCharge, RatingEventId, RatingPlanId, RatingRegistry, RatingRequest,
};

const DAY: i64 = 86_400_000;

fn currency(code: &str) -> Currency {
    Currency::new(code).unwrap()
}

#[allow(clippy::too_many_arguments)]
fn rated_charge(
    event: &str,
    meter: &str,
    subject: &str,
    customer: &str,
    currency_code: &str,
    aggregate_value: i128,
    unit_price_minor: i128,
    included_units: i128,
    rated_at_unix_ms: i64,
) -> RatedCharge {
    let meter_id = MeterId::new(meter).unwrap();
    let subject_id = SubjectId::new(subject).unwrap();
    let event_type = EventType::new("usage.unit").unwrap();
    let mut metering = MeteringEngine::new();
    metering
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
    if aggregate_value > 0 {
        metering
            .ingest(
                UsageEvent::new(
                    UsageEventId::new(format!("usage-{event}")).unwrap(),
                    meter_id.clone(),
                    event_type,
                    subject_id.clone(),
                    1_000,
                    2_000,
                    UsageValue::Sum(aggregate_value),
                )
                .unwrap(),
            )
            .unwrap();
    }
    let aggregate = metering.aggregate(&meter_id, &subject_id, 0, DAY).unwrap();
    let plan = RatePlan::new(
        RatingPlanId::new(format!("plan-{event}")).unwrap(),
        meter_id,
        currency(currency_code),
        unit_price_minor,
        included_units,
        0,
        Some(DAY),
    )
    .unwrap();
    let request = RatingRequest::new(
        RatingEventId::new(event).unwrap(),
        aggregate,
        BillingCustomerId::new(customer).unwrap(),
        plan,
        rated_at_unix_ms,
    )
    .unwrap();
    RatingRegistry::new()
        .rate(request)
        .unwrap()
        .charge()
        .clone()
}

fn request(
    event: &str,
    invoice: &str,
    customer: &str,
    charges: Vec<RatedCharge>,
) -> DraftInvoiceRequest {
    DraftInvoiceRequest::new(
        BillingEventId::new(event).unwrap(),
        BillingInvoiceId::new(invoice).unwrap(),
        BillingCustomerId::new(customer).unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        0,
        DAY,
        DAY,
        charges,
    )
    .unwrap()
}

fn normal_charge(event: &str, customer: &str, amount_units: i128) -> RatedCharge {
    rated_charge(
        event,
        &format!("meter-{event}"),
        &format!("subject-{event}"),
        customer,
        "USD",
        amount_units,
        10,
        0,
        DAY,
    )
}

#[test]
fn request_shape_validation_fails_closed() {
    let ids = || {
        (
            BillingEventId::new("event-shape").unwrap(),
            BillingInvoiceId::new("invoice-shape").unwrap(),
            BillingCustomerId::new("customer-1").unwrap(),
            LedgerScopeId::new("org-1").unwrap(),
        )
    };
    let (event, invoice, customer, scope) = ids();
    assert_eq!(
        DraftInvoiceRequest::new(event, invoice, customer, scope, 0, DAY, DAY, vec![]),
        Err(InvoicingError::EmptyChargeSet)
    );

    let charge = normal_charge("shape-charge", "customer-1", 1);
    let (event, invoice, customer, scope) = ids();
    assert_eq!(
        DraftInvoiceRequest::new(
            event,
            invoice,
            customer,
            scope,
            DAY,
            DAY,
            DAY,
            vec![charge.clone()]
        ),
        Err(InvoicingError::InvalidBillingPeriod {
            period_start_unix_ms: DAY,
            period_end_unix_ms: DAY,
        })
    );
    let (event, invoice, customer, scope) = ids();
    assert_eq!(
        DraftInvoiceRequest::new(
            event,
            invoice,
            customer,
            scope,
            0,
            DAY,
            -1,
            vec![charge.clone()]
        ),
        Err(InvoicingError::InvalidObservedAt(-1))
    );
    let (event, invoice, customer, scope) = ids();
    assert_eq!(
        DraftInvoiceRequest::new(
            event,
            invoice,
            customer,
            scope,
            0,
            DAY,
            DAY - 1,
            vec![charge]
        ),
        Err(InvoicingError::ObservedBeforePeriodEnd {
            observed_at_unix_ms: DAY - 1,
            period_end_unix_ms: DAY,
        })
    );
}

#[test]
fn duplicate_charge_id_in_one_request_fails_closed() {
    let charge = normal_charge("duplicate", "customer-1", 1);
    let result = DraftInvoiceRequest::new(
        BillingEventId::new("event-duplicate").unwrap(),
        BillingInvoiceId::new("invoice-duplicate").unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        0,
        DAY,
        DAY,
        vec![charge.clone(), charge.clone()],
    );
    assert_eq!(
        result,
        Err(InvoicingError::DuplicateRatedChargeId(charge.id().clone()))
    );
}

#[test]
fn customer_mismatch_fails_closed() {
    let charge = normal_charge("customer-mismatch", "customer-other", 1);
    let result = DraftInvoiceRequest::new(
        BillingEventId::new("event-customer").unwrap(),
        BillingInvoiceId::new("invoice-customer").unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        0,
        DAY,
        DAY,
        vec![charge.clone()],
    );
    assert_eq!(
        result,
        Err(InvoicingError::CustomerMismatch {
            rated_charge_id: charge.id().clone()
        })
    );
}

#[test]
fn currency_mismatch_fails_closed() {
    let usd = normal_charge("a-usd", "customer-1", 1);
    let eur = rated_charge(
        "eur",
        "meter-eur",
        "subject-eur",
        "customer-1",
        "EUR",
        1,
        10,
        0,
        DAY,
    );
    let result = DraftInvoiceRequest::new(
        BillingEventId::new("event-currency").unwrap(),
        BillingInvoiceId::new("invoice-currency").unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        0,
        DAY,
        DAY,
        vec![usd, eur.clone()],
    );
    assert!(matches!(
        result,
        Err(InvoicingError::CurrencyMismatch { rated_charge_id, .. })
            if rated_charge_id == *eur.id()
    ));
}

#[test]
fn charge_window_must_be_inside_billing_period() {
    let charge = normal_charge("period", "customer-1", 1);
    let result = DraftInvoiceRequest::new(
        BillingEventId::new("event-period").unwrap(),
        BillingInvoiceId::new("invoice-period").unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        1,
        DAY,
        DAY,
        vec![charge.clone()],
    );
    assert!(matches!(
        result,
        Err(InvoicingError::ChargeOutsideBillingPeriod { rated_charge_id, .. })
            if rated_charge_id == *charge.id()
    ));
}

#[test]
fn future_rated_charge_fails_closed() {
    let charge = rated_charge(
        "future",
        "meter-future",
        "subject-future",
        "customer-1",
        "USD",
        1,
        10,
        0,
        DAY + 10,
    );
    let result = DraftInvoiceRequest::new(
        BillingEventId::new("event-future").unwrap(),
        BillingInvoiceId::new("invoice-future").unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        0,
        DAY,
        DAY,
        vec![charge.clone()],
    );
    assert_eq!(
        result,
        Err(InvoicingError::ChargeRatedAfterObservation {
            rated_charge_id: charge.id().clone(),
            rated_at_unix_ms: DAY + 10,
            observed_at_unix_ms: DAY,
        })
    );
}

#[test]
fn total_overflow_fails_closed() {
    let huge = rated_charge(
        "huge",
        "meter-huge",
        "subject-huge",
        "customer-1",
        "USD",
        1,
        i128::MAX,
        0,
        DAY,
    );
    let one = rated_charge(
        "one",
        "meter-one",
        "subject-one",
        "customer-1",
        "USD",
        1,
        1,
        0,
        DAY,
    );
    assert_eq!(
        DraftInvoiceRequest::new(
            BillingEventId::new("event-overflow").unwrap(),
            BillingInvoiceId::new("invoice-overflow").unwrap(),
            BillingCustomerId::new("customer-1").unwrap(),
            LedgerScopeId::new("org-1").unwrap(),
            0,
            DAY,
            DAY,
            vec![huge, one],
        ),
        Err(InvoicingError::TotalOverflow)
    );
}

#[test]
fn canonical_line_order_total_and_zero_line_are_deterministic() {
    let charge_b = normal_charge("b", "customer-1", 3);
    let charge_a = normal_charge("a", "customer-1", 2);
    let zero = rated_charge(
        "zero",
        "meter-zero",
        "subject-zero",
        "customer-1",
        "USD",
        2,
        100,
        2,
        DAY,
    );
    let mut registry = DraftInvoiceRegistry::new();
    let outcome = registry
        .assemble(request(
            "event-canonical",
            "invoice-canonical",
            "customer-1",
            vec![charge_b, zero.clone(), charge_a],
        ))
        .unwrap();
    let draft = outcome.draft();
    let ids = draft
        .lines()
        .iter()
        .map(|line| line.rated_charge_id().as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec!["rating:a:charge", "rating:b:charge", "rating:zero:charge"]
    );
    assert_eq!(draft.total_minor(), 50);
    assert_eq!(draft.lines()[2].rated_charge().id(), zero.id());
    assert_eq!(draft.lines()[2].amount_minor(), 0);
}

#[test]
fn exact_replay_is_input_order_independent() {
    let first_charge = normal_charge("replay-a", "customer-1", 1);
    let second_charge = normal_charge("replay-b", "customer-1", 2);
    let first_request = request(
        "event-replay",
        "invoice-replay",
        "customer-1",
        vec![first_charge.clone(), second_charge.clone()],
    );
    let second_request = request(
        "event-replay",
        "invoice-replay",
        "customer-1",
        vec![second_charge, first_charge],
    );
    assert_eq!(first_request, second_request);

    let mut registry = DraftInvoiceRegistry::new();
    let first = registry.assemble(first_request).unwrap();
    let replay = registry.assemble(second_request).unwrap();
    assert!(matches!(first, DraftAssemblyOutcome::Created { .. }));
    assert!(matches!(replay, DraftAssemblyOutcome::Replayed { .. }));
    assert_eq!(first.draft(), replay.draft());
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.invoice_count(), 1);
    assert_eq!(registry.charge_binding_count(), 2);
}

#[test]
fn conflicting_source_event_fails_closed() {
    let mut registry = DraftInvoiceRegistry::new();
    registry
        .assemble(request(
            "event-conflict",
            "invoice-a",
            "customer-1",
            vec![normal_charge("conflict-a", "customer-1", 1)],
        ))
        .unwrap();
    let result = registry.assemble(request(
        "event-conflict",
        "invoice-b",
        "customer-1",
        vec![normal_charge("conflict-b", "customer-1", 1)],
    ));
    assert_eq!(
        result,
        Err(InvoicingError::SourceEventConflict(
            BillingEventId::new("event-conflict").unwrap()
        ))
    );
    assert_eq!(registry.event_count(), 1);
}

#[test]
fn invoice_id_cannot_create_second_draft_history() {
    let mut registry = DraftInvoiceRegistry::new();
    registry
        .assemble(request(
            "event-first",
            "invoice-one",
            "customer-1",
            vec![normal_charge("invoice-a", "customer-1", 1)],
        ))
        .unwrap();
    let result = registry.assemble(request(
        "event-second",
        "invoice-one",
        "customer-1",
        vec![normal_charge("invoice-b", "customer-1", 1)],
    ));
    assert_eq!(
        result,
        Err(InvoicingError::InvoiceIdConflict {
            invoice_id: BillingInvoiceId::new("invoice-one").unwrap(),
            existing_event_id: BillingEventId::new("event-first").unwrap(),
        })
    );
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.invoice_count(), 1);
}

#[test]
fn rated_charge_cannot_bind_to_two_invoice_histories() {
    let charge = normal_charge("shared-charge", "customer-1", 1);
    let mut registry = DraftInvoiceRegistry::new();
    registry
        .assemble(request(
            "event-charge-first",
            "invoice-charge-first",
            "customer-1",
            vec![charge.clone()],
        ))
        .unwrap();
    let result = registry.assemble(request(
        "event-charge-second",
        "invoice-charge-second",
        "customer-1",
        vec![charge.clone()],
    ));
    assert_eq!(
        result,
        Err(InvoicingError::RatedChargeAlreadyBound {
            rated_charge_id: charge.id().clone(),
            existing_invoice_id: BillingInvoiceId::new("invoice-charge-first").unwrap(),
        })
    );
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.charge_binding_count(), 1);
}

#[test]
fn rejected_request_reserves_nothing_and_corrected_retry_succeeds() {
    let wrong_customer = normal_charge("retry", "customer-other", 1);
    let bad = DraftInvoiceRequest::new(
        BillingEventId::new("event-retry").unwrap(),
        BillingInvoiceId::new("invoice-retry").unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        0,
        DAY,
        DAY,
        vec![wrong_customer],
    );
    assert!(matches!(bad, Err(InvoicingError::CustomerMismatch { .. })));

    let mut registry = DraftInvoiceRegistry::new();
    assert_eq!(registry.event_count(), 0);
    assert_eq!(registry.invoice_count(), 0);
    assert_eq!(registry.charge_binding_count(), 0);

    let corrected = request(
        "event-retry",
        "invoice-retry",
        "customer-1",
        vec![normal_charge("retry-corrected", "customer-1", 1)],
    );
    assert!(matches!(
        registry.assemble(corrected).unwrap(),
        DraftAssemblyOutcome::Created { .. }
    ));
    assert_eq!(registry.event_count(), 1);
}

#[test]
fn billing_event_is_exact_draft_and_has_zero_ledger_effect() {
    let charge = normal_charge("billing-event", "customer-1", 4);
    let mut registry = DraftInvoiceRegistry::new();
    let draft = registry
        .assemble(request(
            "event-billing",
            "invoice-billing",
            "customer-1",
            vec![charge],
        ))
        .unwrap()
        .draft()
        .clone();

    let generated = draft.to_billing_event();
    let expected = InvoiceEvent::new(
        BillingEventId::new("event-billing").unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        BillingInvoiceId::new("invoice-billing").unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        InvoiceStatus::Draft,
        currency("USD"),
        40,
        40,
        0,
        None,
        DAY,
    );
    assert_eq!(generated, expected);
    assert_eq!(generated.status(), InvoiceStatus::Draft);
    assert_eq!(generated.amount_due_minor(), 40);

    let accounts = BillingLedgerAccounts::new(
        AccountId::new("receivable-not-registered").unwrap(),
        AccountId::new("revenue-not-registered").unwrap(),
    )
    .unwrap();
    let mut ledger = Ledger::new();
    assert_eq!(
        BillingLedgerBridge::new()
            .apply(&generated, &accounts, &mut ledger)
            .unwrap(),
        BillingApplyOutcome::IgnoredDraft
    );
    assert_eq!(ledger.entry_count(), 0);
}
