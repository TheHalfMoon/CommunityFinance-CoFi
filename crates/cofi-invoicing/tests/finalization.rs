#![allow(clippy::unwrap_used)]

use cofi_billing::{
    BillingApplyOutcome, BillingCustomerId, BillingEventId, BillingInvoiceId,
    BillingLedgerAccounts, BillingLedgerBridge, InvoiceStatus,
};
use cofi_invoicing::{
    DraftInvoice, DraftInvoiceRegistry, DraftInvoiceRequest, FinalizationError,
    FinalizationOutcome, FinalizationRegistry, FinalizationRequest,
};
use cofi_ledger::{Account, AccountId, AccountKind, Currency, Ledger, LedgerScopeId};
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageEvent,
    UsageEventId, UsageValue, WindowSize,
};
use cofi_rating::{RatePlan, RatingEventId, RatingPlanId, RatingRegistry, RatingRequest};

const DAY: i64 = 86_400_000;

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}

fn scope() -> LedgerScopeId {
    LedgerScopeId::new("org-1").unwrap()
}

fn account_id(value: &str) -> AccountId {
    AccountId::new(value).unwrap()
}

fn rated_charge(event: &str, customer: &str, aggregate_value: i128) -> cofi_rating::RatedCharge {
    let meter_id = MeterId::new(format!("meter-{event}")).unwrap();
    let subject_id = SubjectId::new(format!("subject-{event}")).unwrap();
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
        usd(),
        10,
        0,
        0,
        Some(DAY),
    )
    .unwrap();
    RatingRegistry::new()
        .rate(
            RatingRequest::new(
                RatingEventId::new(event).unwrap(),
                aggregate,
                BillingCustomerId::new(customer).unwrap(),
                plan,
                DAY,
            )
            .unwrap(),
        )
        .unwrap()
        .charge()
        .clone()
}

fn draft(draft_event: &str, invoice: &str, customer: &str, aggregate_value: i128) -> DraftInvoice {
    let request = DraftInvoiceRequest::new(
        BillingEventId::new(draft_event).unwrap(),
        BillingInvoiceId::new(invoice).unwrap(),
        BillingCustomerId::new(customer).unwrap(),
        scope(),
        0,
        DAY,
        DAY,
        vec![rated_charge(
            &format!("charge-{invoice}"),
            customer,
            aggregate_value,
        )],
    )
    .unwrap();
    DraftInvoiceRegistry::new()
        .assemble(request)
        .unwrap()
        .draft()
        .clone()
}

fn finalization_request(
    event: &str,
    draft: DraftInvoice,
    finalized_at_unix_ms: i64,
    observed_at_unix_ms: i64,
) -> FinalizationRequest {
    FinalizationRequest::new(
        BillingEventId::new(event).unwrap(),
        draft,
        finalized_at_unix_ms,
        observed_at_unix_ms,
    )
    .unwrap()
}

fn ledger_and_accounts() -> (Ledger, BillingLedgerAccounts) {
    let mut ledger = Ledger::new();
    ledger
        .register_account(Account::new(
            account_id("accounts-receivable"),
            scope(),
            AccountKind::Asset,
            usd(),
        ))
        .unwrap();
    ledger
        .register_account(Account::new(
            account_id("revenue"),
            scope(),
            AccountKind::Revenue,
            usd(),
        ))
        .unwrap();
    let accounts =
        BillingLedgerAccounts::new(account_id("accounts-receivable"), account_id("revenue"))
            .unwrap();
    (ledger, accounts)
}

#[test]
fn finalized_invoice_maps_exact_draft_and_bridge_commits_once() {
    let draft = draft("draft-1", "invoice-1", "customer-1", 5);
    let mut registry = FinalizationRegistry::new();
    let outcome = registry
        .finalize(finalization_request(
            "finalize-1",
            draft.clone(),
            DAY + 100,
            DAY + 200,
        ))
        .unwrap();
    let finalized = outcome.finalized();
    assert_eq!(finalized.source_event_id().as_str(), "finalize-1");
    assert_eq!(finalized.draft(), &draft);
    assert_eq!(finalized.finalized_at_unix_ms(), DAY + 100);
    assert_eq!(finalized.observed_at_unix_ms(), DAY + 200);

    let event = finalized.to_billing_event();
    assert_eq!(event.status(), InvoiceStatus::Finalized);
    assert_eq!(event.invoice_id(), draft.invoice_id());
    assert_eq!(event.customer_id(), draft.customer_id());
    assert_eq!(event.organization_scope(), draft.organization_scope());
    assert_eq!(event.currency(), draft.currency());
    assert_eq!(event.amount_due_minor(), draft.total_minor());

    let (mut ledger, accounts) = ledger_and_accounts();
    assert_eq!(ledger.entry_count(), 0);
    let bridge = BillingLedgerBridge::new();
    let first = bridge.apply(&event, &accounts, &mut ledger).unwrap();
    assert!(matches!(first, BillingApplyOutcome::Committed { .. }));
    assert_eq!(ledger.entry_count(), 1);
    let replay = bridge.apply(&event, &accounts, &mut ledger).unwrap();
    assert!(matches!(replay, BillingApplyOutcome::Replayed { .. }));
    assert_eq!(ledger.entry_count(), 1);
}

#[test]
fn exact_finalization_replay_has_zero_duplicate_registry_effect() {
    let draft = draft("draft-replay", "invoice-replay", "customer-1", 3);
    let request = finalization_request("finalize-replay", draft, DAY + 10, DAY + 20);
    let mut registry = FinalizationRegistry::new();
    let created = registry.finalize(request.clone()).unwrap();
    let replayed = registry.finalize(request).unwrap();
    assert!(matches!(created, FinalizationOutcome::Created { .. }));
    assert!(matches!(replayed, FinalizationOutcome::Replayed { .. }));
    assert_eq!(created.finalized(), replayed.finalized());
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.invoice_count(), 1);
}

#[test]
fn draft_source_event_identity_cannot_be_reused_for_finalization() {
    let draft = draft("draft-same-event", "invoice-same-event", "customer-1", 2);
    let error = FinalizationRequest::new(draft.source_event_id().clone(), draft, DAY + 1, DAY + 2)
        .unwrap_err();
    assert!(matches!(
        error,
        FinalizationError::ReusedDraftSourceEvent(_)
    ));
}

#[test]
fn zero_total_draft_fails_without_reserving_and_corrected_retry_succeeds() {
    let zero = draft("draft-zero", "invoice-zero", "customer-1", 0);
    let error = FinalizationRequest::new(
        BillingEventId::new("finalize-retry").unwrap(),
        zero,
        DAY + 1,
        DAY + 2,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        FinalizationError::NonPositiveDraftTotal { total_minor: 0, .. }
    ));

    let mut registry = FinalizationRegistry::new();
    assert_eq!(registry.event_count(), 0);
    assert_eq!(registry.invoice_count(), 0);
    let corrected = draft("draft-positive", "invoice-zero", "customer-1", 1);
    let outcome = registry
        .finalize(finalization_request(
            "finalize-retry",
            corrected,
            DAY + 1,
            DAY + 2,
        ))
        .unwrap();
    assert!(matches!(outcome, FinalizationOutcome::Created { .. }));
}

#[test]
fn finalization_timestamps_are_monotonic_and_fail_closed() {
    let base = draft("draft-time", "invoice-time", "customer-1", 1);
    let invalid_finalized = FinalizationRequest::new(
        BillingEventId::new("finalize-negative").unwrap(),
        base.clone(),
        -1,
        DAY + 1,
    )
    .unwrap_err();
    assert!(matches!(
        invalid_finalized,
        FinalizationError::InvalidFinalizedAt(-1)
    ));

    let before_ready = FinalizationRequest::new(
        BillingEventId::new("finalize-before-ready").unwrap(),
        base.clone(),
        DAY - 1,
        DAY + 1,
    )
    .unwrap_err();
    assert!(matches!(
        before_ready,
        FinalizationError::FinalizedBeforeDraftReady { .. }
    ));

    let invalid_observed = FinalizationRequest::new(
        BillingEventId::new("finalize-observed-negative").unwrap(),
        base.clone(),
        DAY + 1,
        -1,
    )
    .unwrap_err();
    assert!(matches!(
        invalid_observed,
        FinalizationError::InvalidObservedAt(-1)
    ));

    let observed_before = FinalizationRequest::new(
        BillingEventId::new("finalize-observed-before").unwrap(),
        base,
        DAY + 10,
        DAY + 9,
    )
    .unwrap_err();
    assert!(matches!(
        observed_before,
        FinalizationError::ObservedBeforeFinalized { .. }
    ));
}

#[test]
fn conflicting_finalization_event_reuse_fails_closed() {
    let first_draft = draft("draft-conflict-a", "invoice-conflict-a", "customer-1", 1);
    let second_draft = draft("draft-conflict-b", "invoice-conflict-b", "customer-1", 2);
    let mut registry = FinalizationRegistry::new();
    registry
        .finalize(finalization_request(
            "finalize-conflict",
            first_draft,
            DAY + 1,
            DAY + 2,
        ))
        .unwrap();
    let error = registry
        .finalize(finalization_request(
            "finalize-conflict",
            second_draft,
            DAY + 1,
            DAY + 2,
        ))
        .unwrap_err();
    assert!(matches!(error, FinalizationError::SourceEventConflict(_)));
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.invoice_count(), 1);
}

#[test]
fn one_invoice_cannot_create_second_finalization_history() {
    let first = draft("draft-invoice-one", "invoice-one", "customer-1", 1);
    let second = first.clone();
    let mut registry = FinalizationRegistry::new();
    registry
        .finalize(finalization_request(
            "finalize-one-a",
            first,
            DAY + 1,
            DAY + 2,
        ))
        .unwrap();
    let error = registry
        .finalize(finalization_request(
            "finalize-one-b",
            second,
            DAY + 3,
            DAY + 4,
        ))
        .unwrap_err();
    assert!(matches!(
        error,
        FinalizationError::InvoiceAlreadyFinalized { .. }
    ));
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.invoice_count(), 1);
}

#[test]
fn registry_lookup_is_deterministic_and_finalization_itself_has_zero_ledger_effect() {
    let draft = draft("draft-lookup", "invoice-lookup", "customer-1", 4);
    let event_id = BillingEventId::new("finalize-lookup").unwrap();
    let mut registry = FinalizationRegistry::new();
    let (ledger, _) = ledger_and_accounts();
    assert_eq!(ledger.entry_count(), 0);
    let outcome = registry
        .finalize(FinalizationRequest::new(event_id.clone(), draft, DAY + 1, DAY + 2).unwrap())
        .unwrap();
    assert_eq!(ledger.entry_count(), 0);
    assert_eq!(
        registry.finalized_for_event(&event_id),
        Some(outcome.finalized())
    );
}
