#![allow(clippy::unwrap_used)]

use cofi_billing::{
    BillingApplyOutcome, BillingCustomerId, BillingEventId, BillingInvoiceId,
    BillingLedgerAccounts, BillingLedgerBridge, InvoiceStatus,
};
use cofi_finalization_authorization::{
    AuthorizedFinalizationError, AuthorizedFinalizationOutcome, AuthorizedFinalizationRegistry,
    AuthorizedFinalizationRequest,
};
use cofi_invoice_authorization::{
    AuthorizedDraft, AuthorizedDraftRegistry, AuthorizedDraftRequest,
};
use cofi_invoicing::FinalizationError;
use cofi_ledger::{Account, AccountId, AccountKind, Currency, Ledger, LedgerScopeId};
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageAggregate,
    UsageEvent, UsageEventId, UsageValue, WindowSize,
};
use cofi_rating::{RatePlan, RatingEventId, RatingPlanId};
use cofi_rating_authorization::{AuthorizedRatingRegistry, AuthorizedRatingRequest};
use cofi_subscriptions::{
    SubscriptionEventId, SubscriptionId, SubscriptionRegistry, SubscriptionRequest,
};

const DAY: i64 = 86_400_000;

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}

fn scope() -> LedgerScopeId {
    LedgerScopeId::new("org-1").unwrap()
}

fn customer(value: &str) -> BillingCustomerId {
    BillingCustomerId::new(value).unwrap()
}

fn meter(value: &str) -> MeterId {
    MeterId::new(value).unwrap()
}

fn subject(value: &str) -> SubjectId {
    SubjectId::new(value).unwrap()
}

fn account_id(value: &str) -> AccountId {
    AccountId::new(value).unwrap()
}

fn plan(id: &str, meter_id: &str, included_units: i128) -> RatePlan {
    RatePlan::new(
        RatingPlanId::new(id).unwrap(),
        meter(meter_id),
        usd(),
        10,
        included_units,
        0,
        Some(DAY),
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
                        "usage-{meter_id}-{subject_id}-{index}",
                        meter_id = meter_id.as_str(),
                        subject_id = subject_id.as_str(),
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

fn authorized_draft(
    draft_event: &str,
    invoice_id: &str,
    customer_id: &str,
    usage_count: u64,
    included_units: i128,
) -> AuthorizedDraft {
    let meter_name = format!("meter-{invoice_id}");
    let subject_name = format!("subject-{invoice_id}");
    let subscription_id = format!("subscription-{invoice_id}");
    let subscription_event_id = format!("subscription-event-{invoice_id}");
    let plan_id = format!("plan-{invoice_id}");
    let rating_event_id = format!("rating-{invoice_id}");

    let mut subscriptions = SubscriptionRegistry::new();
    subscriptions
        .create(
            SubscriptionRequest::new(
                SubscriptionEventId::new(subscription_event_id).unwrap(),
                SubscriptionId::new(subscription_id).unwrap(),
                scope(),
                customer(customer_id),
                subject(&subject_name),
                plan(&plan_id, &meter_name, included_units),
                0,
                Some(DAY),
            )
            .unwrap(),
        )
        .unwrap();

    let mut ratings = AuthorizedRatingRegistry::new();
    let rating = ratings
        .rate(
            AuthorizedRatingRequest::new(
                RatingEventId::new(rating_event_id).unwrap(),
                scope(),
                customer(customer_id),
                count_aggregate(&meter_name, &subject_name, usage_count),
                DAY,
            ),
            &subscriptions,
        )
        .unwrap()
        .authorization()
        .clone();

    let mut drafts = AuthorizedDraftRegistry::new();
    drafts
        .assemble(
            AuthorizedDraftRequest::new(
                BillingEventId::new(draft_event).unwrap(),
                BillingInvoiceId::new(invoice_id).unwrap(),
                0,
                DAY,
                DAY,
                vec![rating],
            )
            .unwrap(),
        )
        .unwrap()
        .authorization()
        .clone()
}

fn finalization_request(
    event_id: &str,
    draft: AuthorizedDraft,
    finalized_at_unix_ms: i64,
    observed_at_unix_ms: i64,
) -> AuthorizedFinalizationRequest {
    AuthorizedFinalizationRequest::new(
        BillingEventId::new(event_id).unwrap(),
        draft,
        finalized_at_unix_ms,
        observed_at_unix_ms,
    )
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
fn successful_finalization_preserves_p22_and_subscription_lineage() {
    let draft = authorized_draft("draft-success", "invoice-success", "customer-1", 3, 1);
    let mut registry = AuthorizedFinalizationRegistry::new();
    let outcome = registry
        .finalize(finalization_request(
            "finalize-success",
            draft.clone(),
            DAY + 100,
            DAY + 200,
        ))
        .unwrap();

    assert!(matches!(
        outcome,
        AuthorizedFinalizationOutcome::Created { .. }
    ));
    let authorization = outcome.authorization();
    assert_eq!(authorization.authorized_draft(), &draft);
    assert_eq!(authorization.finalized().draft(), draft.draft());
    assert_eq!(
        authorization.authorized_draft().authorizations()[0]
            .subscription()
            .id()
            .as_str(),
        "subscription-invoice-success"
    );
    assert_eq!(
        authorization.authorized_draft().authorizations()[0]
            .subscription()
            .source_event_id()
            .as_str(),
        "subscription-event-invoice-success"
    );

    let event = authorization.finalized().to_billing_event();
    assert_eq!(event.status(), InvoiceStatus::Finalized);
    assert_eq!(event.invoice_id(), draft.draft().invoice_id());
    assert_eq!(event.customer_id(), draft.draft().customer_id());
    assert_eq!(
        event.organization_scope(),
        draft.draft().organization_scope()
    );
    assert_eq!(event.currency(), draft.draft().currency());
    assert_eq!(event.amount_due_minor(), draft.draft().total_minor());
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.finalization_event_count(), 1);
}

#[test]
fn exact_replay_returns_historical_authorized_finalization() {
    let draft = authorized_draft("draft-replay", "invoice-replay", "customer-1", 2, 0);
    let request = finalization_request("finalize-replay", draft, DAY + 1, DAY + 2);
    let mut registry = AuthorizedFinalizationRegistry::new();

    let first = registry.finalize(request.clone()).unwrap();
    let replay = registry.finalize(request).unwrap();
    assert!(matches!(
        first,
        AuthorizedFinalizationOutcome::Created { .. }
    ));
    assert!(matches!(
        replay,
        AuthorizedFinalizationOutcome::Replayed { .. }
    ));
    assert_eq!(first.authorization(), replay.authorization());
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.finalization_event_count(), 1);
}

#[test]
fn conflicting_finalization_event_reuse_fails_before_second_p19_history() {
    let first = authorized_draft("draft-conflict-a", "invoice-conflict-a", "customer-1", 1, 0);
    let second = authorized_draft("draft-conflict-b", "invoice-conflict-b", "customer-1", 2, 0);
    let mut registry = AuthorizedFinalizationRegistry::new();
    registry
        .finalize(finalization_request(
            "finalize-conflict",
            first,
            DAY + 1,
            DAY + 2,
        ))
        .unwrap();

    assert_eq!(
        registry.finalize(finalization_request(
            "finalize-conflict",
            second,
            DAY + 1,
            DAY + 2,
        )),
        Err(AuthorizedFinalizationError::SourceEventConflict(
            BillingEventId::new("finalize-conflict").unwrap()
        ))
    );
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.finalization_event_count(), 1);
}

#[test]
fn p19_timing_error_reserves_nothing_and_corrected_retry_succeeds() {
    let draft = authorized_draft("draft-time", "invoice-time", "customer-1", 1, 0);
    let mut registry = AuthorizedFinalizationRegistry::new();

    let error = registry
        .finalize(finalization_request(
            "finalize-time",
            draft.clone(),
            DAY - 1,
            DAY + 1,
        ))
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFinalizationError::Finalization(
            FinalizationError::FinalizedBeforeDraftReady { .. }
        )
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.finalization_event_count(), 0);

    let corrected = registry
        .finalize(finalization_request(
            "finalize-time",
            draft,
            DAY + 1,
            DAY + 2,
        ))
        .unwrap();
    assert!(matches!(
        corrected,
        AuthorizedFinalizationOutcome::Created { .. }
    ));
}

#[test]
fn draft_source_event_cannot_be_reused_as_p23_finalization_event() {
    let draft = authorized_draft(
        "shared-draft-finalization-event",
        "invoice-reused-event",
        "customer-1",
        1,
        0,
    );
    let mut registry = AuthorizedFinalizationRegistry::new();
    let error = registry
        .finalize(finalization_request(
            "shared-draft-finalization-event",
            draft,
            DAY + 1,
            DAY + 2,
        ))
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFinalizationError::Finalization(FinalizationError::ReusedDraftSourceEvent(_))
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.finalization_event_count(), 0);
}

#[test]
fn zero_total_authorized_draft_fails_through_canonical_p19() {
    let draft = authorized_draft("draft-zero", "invoice-zero", "customer-1", 2, 2);
    assert_eq!(draft.draft().total_minor(), 0);
    let mut registry = AuthorizedFinalizationRegistry::new();
    let error = registry
        .finalize(finalization_request(
            "finalize-zero",
            draft,
            DAY + 1,
            DAY + 2,
        ))
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFinalizationError::Finalization(FinalizationError::NonPositiveDraftTotal {
            total_minor: 0,
            ..
        })
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.finalization_event_count(), 0);
}

#[test]
fn canonical_p19_uniqueness_blocks_second_finalization_for_same_invoice() {
    let draft = authorized_draft("draft-unique", "invoice-unique", "customer-1", 1, 0);
    let mut registry = AuthorizedFinalizationRegistry::new();
    registry
        .finalize(finalization_request(
            "finalize-unique-a",
            draft.clone(),
            DAY + 1,
            DAY + 2,
        ))
        .unwrap();

    let error = registry
        .finalize(finalization_request(
            "finalize-unique-b",
            draft,
            DAY + 3,
            DAY + 4,
        ))
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFinalizationError::Finalization(
            FinalizationError::InvoiceAlreadyFinalized { .. }
        )
    ));
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.finalization_event_count(), 1);
}

#[test]
fn canonical_billing_bridge_recognizes_receivable_once() {
    let draft = authorized_draft("draft-ledger", "invoice-ledger", "customer-1", 4, 1);
    let mut registry = AuthorizedFinalizationRegistry::new();
    let finalized = registry
        .finalize(finalization_request(
            "finalize-ledger",
            draft.clone(),
            DAY + 1,
            DAY + 2,
        ))
        .unwrap()
        .authorization()
        .finalized()
        .clone();
    let event = finalized.to_billing_event();

    let (mut ledger, accounts) = ledger_and_accounts();
    let bridge = BillingLedgerBridge::new();
    let first = bridge.apply(&event, &accounts, &mut ledger).unwrap();
    assert!(matches!(first, BillingApplyOutcome::Committed { .. }));
    assert_eq!(ledger.entry_count(), 1);
    let replay = bridge.apply(&event, &accounts, &mut ledger).unwrap();
    assert!(matches!(replay, BillingApplyOutcome::Replayed { .. }));
    assert_eq!(ledger.entry_count(), 1);
    assert_eq!(event.amount_due_minor(), draft.draft().total_minor());
}
