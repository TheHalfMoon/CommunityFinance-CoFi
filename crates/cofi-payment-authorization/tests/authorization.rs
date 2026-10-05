#![allow(clippy::unwrap_used)]

use cofi_billing::{
    BillingApplyOutcome, BillingCustomerId, BillingEventId, BillingInvoiceId,
    BillingLedgerAccounts, BillingLedgerBridge,
};
use cofi_finalization_authorization::{
    AuthorizedFinalization, AuthorizedFinalizationRegistry, AuthorizedFinalizationRequest,
};
use cofi_invoice_authorization::{
    AuthorizedDraft, AuthorizedDraftRegistry, AuthorizedDraftRequest,
};
use cofi_ledger::{Account, AccountId, AccountKind, Currency, Ledger, LedgerScopeId};
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageAggregate,
    UsageEvent, UsageEventId, UsageValue, WindowSize,
};
use cofi_payment_authorization::{
    AuthorizedCaptureError, AuthorizedCaptureOutcome, AuthorizedCaptureRegistry,
    AuthorizedCaptureRequest,
};
use cofi_payments::{
    ConnectorTransactionId, PaymentApplyOutcome, PaymentError, PaymentEvent, PaymentEventId,
    PaymentId, PaymentLedgerAccounts, PaymentLedgerBridge, PaymentStatus,
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

fn authorized_finalization(
    invoice_id: &str,
    customer_id: &str,
    usage_count: u64,
    included_units: i128,
) -> AuthorizedFinalization {
    let draft = authorized_draft(
        &format!("draft-{invoice_id}"),
        invoice_id,
        customer_id,
        usage_count,
        included_units,
    );
    let mut registry = AuthorizedFinalizationRegistry::new();
    registry
        .finalize(AuthorizedFinalizationRequest::new(
            BillingEventId::new(format!("finalize-{invoice_id}")).unwrap(),
            draft,
            DAY + 100,
            DAY + 200,
        ))
        .unwrap()
        .authorization()
        .clone()
}

fn capture_request(
    event_id: &str,
    payment_id: &str,
    connector_id: &str,
    authorization: AuthorizedFinalization,
    captured_at_unix_ms: i64,
    observed_at_unix_ms: i64,
) -> AuthorizedCaptureRequest {
    AuthorizedCaptureRequest::new(
        PaymentEventId::new(event_id).unwrap(),
        PaymentId::new(payment_id).unwrap(),
        ConnectorTransactionId::new(connector_id).unwrap(),
        authorization,
        captured_at_unix_ms,
        observed_at_unix_ms,
    )
}

fn ledger_and_accounts(
    processor_kind: AccountKind,
) -> (Ledger, BillingLedgerAccounts, PaymentLedgerAccounts) {
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
    ledger
        .register_account(Account::new(
            account_id("processor-clearing"),
            scope(),
            processor_kind,
            usd(),
        ))
        .unwrap();

    let billing =
        BillingLedgerAccounts::new(account_id("accounts-receivable"), account_id("revenue"))
            .unwrap();
    let payment = PaymentLedgerAccounts::new(
        account_id("accounts-receivable"),
        account_id("processor-clearing"),
    )
    .unwrap();
    (ledger, billing, payment)
}

fn recognize_invoice(
    authorization: &AuthorizedFinalization,
    accounts: &BillingLedgerAccounts,
    ledger: &mut Ledger,
) {
    let event = authorization.finalized().to_billing_event();
    let outcome = BillingLedgerBridge::new()
        .apply(&event, accounts, ledger)
        .unwrap();
    assert!(matches!(outcome, BillingApplyOutcome::Committed { .. }));
}

#[test]
fn successful_capture_derives_invoice_scope_currency_amount_and_preserves_lineage() {
    let finalization = authorized_finalization("invoice-success", "customer-1", 3, 1);
    let (mut ledger, billing_accounts, payment_accounts) = ledger_and_accounts(AccountKind::Asset);
    recognize_invoice(&finalization, &billing_accounts, &mut ledger);

    let mut registry = AuthorizedCaptureRegistry::new();
    let outcome = registry
        .capture(
            capture_request(
                "capture-success",
                "payment-success",
                "connector-success",
                finalization.clone(),
                DAY + 300,
                DAY + 400,
            ),
            &payment_accounts,
            &mut ledger,
        )
        .unwrap();

    assert!(matches!(outcome, AuthorizedCaptureOutcome::Created { .. }));
    let authorization = outcome.authorization();
    assert_eq!(authorization.authorized_finalization(), &finalization);
    assert_eq!(
        authorization
            .authorized_finalization()
            .authorized_draft()
            .authorizations()[0]
            .subscription()
            .id()
            .as_str(),
        "subscription-invoice-success"
    );
    let event = authorization.payment_event();
    let draft = finalization.finalized().draft();
    assert_eq!(event.status(), PaymentStatus::Charged);
    assert_eq!(event.invoice_id(), draft.invoice_id());
    assert_eq!(event.organization_scope(), draft.organization_scope());
    assert_eq!(event.currency(), draft.currency());
    assert_eq!(event.amount_captured_minor(), draft.total_minor());
    assert_eq!(event.payment_id().as_str(), "payment-success");
    assert_eq!(
        event.connector_transaction_id().as_str(),
        "connector-success"
    );
    assert!(ledger.entry(authorization.journal_entry_id()).is_some());
    assert_eq!(ledger.entry_count(), 2);
    assert_eq!(registry.authorization_count(), 1);
}

#[test]
fn missing_invoice_journal_reserves_nothing_and_corrected_retry_succeeds() {
    let finalization = authorized_finalization("invoice-missing-journal", "customer-1", 2, 0);
    let (mut ledger, billing_accounts, payment_accounts) = ledger_and_accounts(AccountKind::Asset);
    let request = capture_request(
        "capture-retry",
        "payment-retry",
        "connector-retry",
        finalization.clone(),
        DAY + 300,
        DAY + 400,
    );
    let mut registry = AuthorizedCaptureRegistry::new();

    let error = registry
        .capture(request.clone(), &payment_accounts, &mut ledger)
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedCaptureError::Payment(PaymentError::MissingInvoiceJournalEntry(_))
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(ledger.entry_count(), 0);

    recognize_invoice(&finalization, &billing_accounts, &mut ledger);
    let corrected = registry
        .capture(request, &payment_accounts, &mut ledger)
        .unwrap();
    assert!(matches!(
        corrected,
        AuthorizedCaptureOutcome::Created { .. }
    ));
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(ledger.entry_count(), 2);
}

#[test]
fn capture_before_finalization_reserves_nothing_and_corrected_retry_succeeds() {
    let finalization = authorized_finalization("invoice-time", "customer-1", 2, 0);
    let (mut ledger, billing_accounts, payment_accounts) = ledger_and_accounts(AccountKind::Asset);
    recognize_invoice(&finalization, &billing_accounts, &mut ledger);
    let mut registry = AuthorizedCaptureRegistry::new();

    let error = registry
        .capture(
            capture_request(
                "capture-time",
                "payment-time",
                "connector-time",
                finalization.clone(),
                DAY + 99,
                DAY + 200,
            ),
            &payment_accounts,
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedCaptureError::Payment(PaymentError::CapturePrecedesInvoiceFinalization { .. })
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(ledger.entry_count(), 1);

    let corrected = registry
        .capture(
            capture_request(
                "capture-time",
                "payment-time",
                "connector-time",
                finalization,
                DAY + 100,
                DAY + 200,
            ),
            &payment_accounts,
            &mut ledger,
        )
        .unwrap();
    assert!(matches!(
        corrected,
        AuthorizedCaptureOutcome::Created { .. }
    ));
}

#[test]
fn exact_replay_is_historical_and_does_not_reapply_current_ledger() {
    let finalization = authorized_finalization("invoice-replay", "customer-1", 2, 0);
    let (mut ledger, billing_accounts, payment_accounts) = ledger_and_accounts(AccountKind::Asset);
    recognize_invoice(&finalization, &billing_accounts, &mut ledger);
    let request = capture_request(
        "capture-replay",
        "payment-replay",
        "connector-replay",
        finalization,
        DAY + 300,
        DAY + 400,
    );
    let mut registry = AuthorizedCaptureRegistry::new();

    let created = registry
        .capture(request.clone(), &payment_accounts, &mut ledger)
        .unwrap();
    let entry_count = ledger.entry_count();
    let unusable_accounts = PaymentLedgerAccounts::new(
        account_id("unknown-receivable"),
        account_id("unknown-clearing"),
    )
    .unwrap();
    let replay = registry
        .capture(request, &unusable_accounts, &mut ledger)
        .unwrap();

    assert!(matches!(created, AuthorizedCaptureOutcome::Created { .. }));
    assert!(matches!(replay, AuthorizedCaptureOutcome::Replayed { .. }));
    assert_eq!(created.authorization(), replay.authorization());
    assert_eq!(ledger.entry_count(), entry_count);
    assert_eq!(registry.authorization_count(), 1);
}

#[test]
fn conflicting_capture_event_reuse_fails_before_second_p05_application() {
    let first = authorized_finalization("invoice-conflict-a", "customer-1", 2, 0);
    let second = authorized_finalization("invoice-conflict-b", "customer-1", 3, 0);
    let (mut ledger, billing_accounts, payment_accounts) = ledger_and_accounts(AccountKind::Asset);
    recognize_invoice(&first, &billing_accounts, &mut ledger);
    recognize_invoice(&second, &billing_accounts, &mut ledger);
    let mut registry = AuthorizedCaptureRegistry::new();

    registry
        .capture(
            capture_request(
                "capture-conflict",
                "payment-conflict-a",
                "connector-conflict-a",
                first,
                DAY + 300,
                DAY + 400,
            ),
            &payment_accounts,
            &mut ledger,
        )
        .unwrap();
    let entry_count = ledger.entry_count();
    let error = registry
        .capture(
            capture_request(
                "capture-conflict",
                "payment-conflict-b",
                "connector-conflict-b",
                second,
                DAY + 300,
                DAY + 400,
            ),
            &payment_accounts,
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedCaptureError::SourceEventConflict(_)
    ));
    assert_eq!(ledger.entry_count(), entry_count);
    assert_eq!(registry.authorization_count(), 1);
}

#[test]
fn second_full_capture_for_same_invoice_is_rejected_by_p05_business_key() {
    let finalization = authorized_finalization("invoice-single-capture", "customer-1", 2, 0);
    let (mut ledger, billing_accounts, payment_accounts) = ledger_and_accounts(AccountKind::Asset);
    recognize_invoice(&finalization, &billing_accounts, &mut ledger);
    let mut registry = AuthorizedCaptureRegistry::new();

    registry
        .capture(
            capture_request(
                "capture-one",
                "payment-one",
                "connector-one",
                finalization.clone(),
                DAY + 300,
                DAY + 400,
            ),
            &payment_accounts,
            &mut ledger,
        )
        .unwrap();
    let error = registry
        .capture(
            capture_request(
                "capture-two",
                "payment-two",
                "connector-two",
                finalization,
                DAY + 500,
                DAY + 600,
            ),
            &payment_accounts,
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedCaptureError::Payment(PaymentError::LedgerCommit(_))
    ));
    assert_eq!(ledger.entry_count(), 2);
    assert_eq!(registry.authorization_count(), 1);
}

#[test]
fn canonical_p05_account_validation_remains_authoritative() {
    let finalization = authorized_finalization("invoice-account", "customer-1", 2, 0);
    let (mut ledger, billing_accounts, payment_accounts) =
        ledger_and_accounts(AccountKind::Revenue);
    recognize_invoice(&finalization, &billing_accounts, &mut ledger);
    let mut registry = AuthorizedCaptureRegistry::new();

    let error = registry
        .capture(
            capture_request(
                "capture-account",
                "payment-account",
                "connector-account",
                finalization,
                DAY + 300,
                DAY + 400,
            ),
            &payment_accounts,
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedCaptureError::Payment(PaymentError::AccountKindMismatch { .. })
    ));
    assert_eq!(ledger.entry_count(), 1);
    assert_eq!(registry.authorization_count(), 0);
}

#[test]
fn orphaned_canonical_p05_replay_is_an_invariant_violation() {
    let finalization = authorized_finalization("invoice-orphan", "customer-1", 2, 0);
    let (mut ledger, billing_accounts, payment_accounts) = ledger_and_accounts(AccountKind::Asset);
    recognize_invoice(&finalization, &billing_accounts, &mut ledger);
    let draft = finalization.finalized().draft();
    let raw_event = PaymentEvent::new(
        PaymentEventId::new("capture-orphan").unwrap(),
        draft.organization_scope().clone(),
        draft.invoice_id().clone(),
        PaymentId::new("payment-orphan").unwrap(),
        ConnectorTransactionId::new("connector-orphan").unwrap(),
        PaymentStatus::Charged,
        draft.currency(),
        draft.total_minor(),
        Some(DAY + 300),
        DAY + 400,
    );
    let raw_outcome = PaymentLedgerBridge::new()
        .apply(&raw_event, &payment_accounts, &mut ledger)
        .unwrap();
    assert!(matches!(raw_outcome, PaymentApplyOutcome::Committed { .. }));

    let mut registry = AuthorizedCaptureRegistry::new();
    let error = registry
        .capture(
            capture_request(
                "capture-orphan",
                "payment-orphan",
                "connector-orphan",
                finalization,
                DAY + 300,
                DAY + 400,
            ),
            &payment_accounts,
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedCaptureError::UnexpectedInternalReplay { .. }
    ));
    assert_eq!(ledger.entry_count(), 2);
    assert_eq!(registry.authorization_count(), 0);
}
