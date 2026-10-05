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
use cofi_ledger::{Account, AccountId, AccountKind, Currency, Ledger, LedgerScopeId, Side};
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageAggregate,
    UsageEvent, UsageEventId, UsageValue, WindowSize,
};
use cofi_payment_authorization::{
    AuthorizedCapture, AuthorizedCaptureRegistry, AuthorizedCaptureRequest,
};
use cofi_payments::{
    BankTransactionReference, ConnectorTransactionId, PaymentApplyOutcome, PaymentEventId,
    PaymentId, PaymentLedgerAccounts, PaymentLedgerBridge, PayoutApplyOutcome, PayoutError,
    PayoutLedgerAccounts, PayoutLedgerBridge, ProcessorPayoutEvent, ProcessorPayoutEventId,
    ProcessorPayoutId,
};
use cofi_payout_authorization::{
    AuthorizedPayoutError, AuthorizedPayoutOutcome, AuthorizedPayoutRegistry,
    AuthorizedPayoutRequest,
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
    invoice_id: &str,
    customer_id: &str,
    usage_count: u64,
    included_units: i128,
) -> AuthorizedDraft {
    let meter_name = format!("meter-{invoice_id}");
    let subject_name = format!("subject-{invoice_id}");
    let subscription_id = format!("subscription-{invoice_id}");
    let plan_id = format!("plan-{invoice_id}");

    let mut subscriptions = SubscriptionRegistry::new();
    subscriptions
        .create(
            SubscriptionRequest::new(
                SubscriptionEventId::new(format!("subscription-event-{invoice_id}")).unwrap(),
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
                RatingEventId::new(format!("rating-{invoice_id}")).unwrap(),
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
                BillingEventId::new(format!("draft-{invoice_id}")).unwrap(),
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
    let draft = authorized_draft(invoice_id, customer_id, usage_count, included_units);
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

fn ledger_and_accounts(
    bank_kind: AccountKind,
) -> (
    Ledger,
    BillingLedgerAccounts,
    PaymentLedgerAccounts,
    PayoutLedgerAccounts,
) {
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
            AccountKind::Asset,
            usd(),
        ))
        .unwrap();
    ledger
        .register_account(Account::new(
            account_id("bank-cash"),
            scope(),
            bank_kind,
            usd(),
        ))
        .unwrap();
    ledger
        .register_account(Account::new(
            account_id("processor-fee-expense"),
            scope(),
            AccountKind::Expense,
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
    let payout = PayoutLedgerAccounts::new(
        account_id("processor-clearing"),
        account_id("bank-cash"),
        Some(account_id("processor-fee-expense")),
    )
    .unwrap();
    (ledger, billing, payment, payout)
}

fn authorized_capture(
    invoice_id: &str,
    usage_count: u64,
    included_units: i128,
    captured_at_unix_ms: i64,
    bank_kind: AccountKind,
) -> (
    AuthorizedCapture,
    Ledger,
    BillingLedgerAccounts,
    PaymentLedgerAccounts,
    PayoutLedgerAccounts,
) {
    let finalization =
        authorized_finalization(invoice_id, "customer-1", usage_count, included_units);
    let (mut ledger, billing_accounts, payment_accounts, payout_accounts) =
        ledger_and_accounts(bank_kind);
    let invoice_event = finalization.finalized().to_billing_event();
    let invoice_outcome = BillingLedgerBridge::new()
        .apply(&invoice_event, &billing_accounts, &mut ledger)
        .unwrap();
    assert!(matches!(
        invoice_outcome,
        BillingApplyOutcome::Committed { .. }
    ));

    let mut registry = AuthorizedCaptureRegistry::new();
    let capture = registry
        .capture(
            AuthorizedCaptureRequest::new(
                PaymentEventId::new(format!("capture-{invoice_id}")).unwrap(),
                PaymentId::new(format!("payment-{invoice_id}")).unwrap(),
                ConnectorTransactionId::new(format!("connector-{invoice_id}")).unwrap(),
                finalization,
                captured_at_unix_ms,
                captured_at_unix_ms + 100,
            ),
            &payment_accounts,
            &mut ledger,
        )
        .unwrap()
        .authorization()
        .clone();

    (
        capture,
        ledger,
        billing_accounts,
        payment_accounts,
        payout_accounts,
    )
}

#[allow(clippy::too_many_arguments)]
fn payout_request(
    event_id: &str,
    payout_id: &str,
    bank_reference: &str,
    capture: AuthorizedCapture,
    fee_minor: i128,
    net_minor: i128,
    paid_at_unix_ms: i64,
    observed_at_unix_ms: i64,
) -> AuthorizedPayoutRequest {
    AuthorizedPayoutRequest::new(
        ProcessorPayoutEventId::new(event_id).unwrap(),
        ProcessorPayoutId::new(payout_id).unwrap(),
        BankTransactionReference::new(bank_reference).unwrap(),
        capture,
        fee_minor,
        net_minor,
        paid_at_unix_ms,
        observed_at_unix_ms,
    )
}

#[test]
fn successful_payout_derives_payment_scope_currency_gross_and_preserves_lineage() {
    let (capture, mut ledger, _, _, payout_accounts) =
        authorized_capture("invoice-success", 3, 1, DAY + 300, AccountKind::Asset);
    assert_eq!(capture.payment_event().amount_captured_minor(), 20);
    assert_eq!(ledger.entry_count(), 2);

    let mut registry = AuthorizedPayoutRegistry::new();
    let outcome = registry
        .apply(
            payout_request(
                "payout-event-success",
                "payout-success",
                "bank-success",
                capture.clone(),
                5,
                15,
                DAY + 400,
                DAY + 500,
            ),
            &payout_accounts,
            &mut ledger,
        )
        .unwrap();

    assert!(matches!(outcome, AuthorizedPayoutOutcome::Created { .. }));
    let authorization = outcome.authorization();
    assert_eq!(authorization.authorized_capture(), &capture);
    assert_eq!(
        authorization
            .authorized_capture()
            .authorized_finalization()
            .authorized_draft()
            .authorizations()[0]
            .subscription()
            .id()
            .as_str(),
        "subscription-invoice-success"
    );
    let event = authorization.payout_event();
    assert_eq!(event.payment_id(), capture.payment_event().payment_id());
    assert_eq!(
        event.organization_scope(),
        capture.payment_event().organization_scope()
    );
    assert_eq!(event.currency(), capture.payment_event().currency());
    assert_eq!(event.payout_id().as_str(), "payout-success");

    let entry = ledger.entry(authorization.journal_entry_id()).unwrap();
    assert_eq!(entry.postings().len(), 3);
    assert!(entry.postings().iter().any(|posting| {
        posting.account_id() == &account_id("bank-cash")
            && posting.side() == Side::Debit
            && posting.amount().value() == 15
    }));
    assert!(entry.postings().iter().any(|posting| {
        posting.account_id() == &account_id("processor-fee-expense")
            && posting.side() == Side::Debit
            && posting.amount().value() == 5
    }));
    assert!(entry.postings().iter().any(|posting| {
        posting.account_id() == &account_id("processor-clearing")
            && posting.side() == Side::Credit
            && posting.amount().value() == 20
    }));
    assert_eq!(ledger.entry_count(), 3);
    assert_eq!(registry.authorization_count(), 1);
}

#[test]
fn fee_net_mismatch_reserves_nothing_and_corrected_retry_succeeds() {
    let (capture, mut ledger, _, _, payout_accounts) =
        authorized_capture("invoice-mismatch", 3, 1, DAY + 300, AccountKind::Asset);
    let mut registry = AuthorizedPayoutRegistry::new();

    let error = registry
        .apply(
            payout_request(
                "payout-event-retry",
                "payout-retry",
                "bank-retry",
                capture.clone(),
                5,
                14,
                DAY + 400,
                DAY + 500,
            ),
            &payout_accounts,
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedPayoutError::Payout(PayoutError::GrossNetFeeMismatch { .. })
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(ledger.entry_count(), 2);

    let corrected = registry
        .apply(
            payout_request(
                "payout-event-retry",
                "payout-retry",
                "bank-retry",
                capture,
                5,
                15,
                DAY + 400,
                DAY + 500,
            ),
            &payout_accounts,
            &mut ledger,
        )
        .unwrap();
    assert!(matches!(corrected, AuthorizedPayoutOutcome::Created { .. }));
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(ledger.entry_count(), 3);
}

#[test]
fn payout_before_capture_reserves_nothing_and_corrected_retry_succeeds() {
    let (capture, mut ledger, _, _, payout_accounts) =
        authorized_capture("invoice-time", 3, 1, DAY + 300, AccountKind::Asset);
    let mut registry = AuthorizedPayoutRegistry::new();

    let error = registry
        .apply(
            payout_request(
                "payout-event-time",
                "payout-time",
                "bank-time",
                capture.clone(),
                0,
                20,
                DAY + 299,
                DAY + 500,
            ),
            &payout_accounts,
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedPayoutError::Payout(PayoutError::PayoutPrecedesCapture { .. })
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(ledger.entry_count(), 2);

    let corrected = registry
        .apply(
            payout_request(
                "payout-event-time",
                "payout-time",
                "bank-time",
                capture,
                0,
                20,
                DAY + 300,
                DAY + 500,
            ),
            &payout_accounts,
            &mut ledger,
        )
        .unwrap();
    assert!(matches!(corrected, AuthorizedPayoutOutcome::Created { .. }));
}

#[test]
fn missing_payment_journal_reserves_nothing_and_corrected_retry_succeeds() {
    let (capture, _, _, _, _) = authorized_capture(
        "invoice-missing-payment",
        3,
        1,
        DAY + 300,
        AccountKind::Asset,
    );
    let (mut ledger, billing_accounts, payment_accounts, payout_accounts) =
        ledger_and_accounts(AccountKind::Asset);
    let invoice_event = capture
        .authorized_finalization()
        .finalized()
        .to_billing_event();
    BillingLedgerBridge::new()
        .apply(&invoice_event, &billing_accounts, &mut ledger)
        .unwrap();
    let request = payout_request(
        "payout-event-missing",
        "payout-missing",
        "bank-missing",
        capture.clone(),
        5,
        15,
        DAY + 400,
        DAY + 500,
    );
    let mut registry = AuthorizedPayoutRegistry::new();

    let error = registry
        .apply(request.clone(), &payout_accounts, &mut ledger)
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedPayoutError::Payout(PayoutError::MissingPaymentJournalEntry(_))
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(ledger.entry_count(), 1);

    let payment_outcome = PaymentLedgerBridge::new()
        .apply(capture.payment_event(), &payment_accounts, &mut ledger)
        .unwrap();
    assert!(matches!(
        payment_outcome,
        PaymentApplyOutcome::Committed { .. }
    ));
    let corrected = registry
        .apply(request, &payout_accounts, &mut ledger)
        .unwrap();
    assert!(matches!(corrected, AuthorizedPayoutOutcome::Created { .. }));
    assert_eq!(ledger.entry_count(), 3);
}

#[test]
fn exact_replay_is_historical_and_does_not_reapply_current_ledger() {
    let (capture, mut ledger, _, _, payout_accounts) =
        authorized_capture("invoice-replay", 3, 1, DAY + 300, AccountKind::Asset);
    let request = payout_request(
        "payout-event-replay",
        "payout-replay",
        "bank-replay",
        capture,
        5,
        15,
        DAY + 400,
        DAY + 500,
    );
    let mut registry = AuthorizedPayoutRegistry::new();
    let created = registry
        .apply(request.clone(), &payout_accounts, &mut ledger)
        .unwrap();
    let entry_count = ledger.entry_count();
    let unusable_accounts = PayoutLedgerAccounts::new(
        account_id("unknown-clearing"),
        account_id("unknown-bank"),
        Some(account_id("unknown-fee")),
    )
    .unwrap();
    let replay = registry
        .apply(request, &unusable_accounts, &mut ledger)
        .unwrap();

    assert!(matches!(created, AuthorizedPayoutOutcome::Created { .. }));
    assert!(matches!(replay, AuthorizedPayoutOutcome::Replayed { .. }));
    assert_eq!(created.authorization(), replay.authorization());
    assert_eq!(ledger.entry_count(), entry_count);
    assert_eq!(registry.authorization_count(), 1);
}

#[test]
fn conflicting_payout_event_reuse_fails_before_second_p06_application() {
    let (capture, mut ledger, _, _, payout_accounts) =
        authorized_capture("invoice-conflict", 3, 1, DAY + 300, AccountKind::Asset);
    let mut registry = AuthorizedPayoutRegistry::new();
    registry
        .apply(
            payout_request(
                "payout-event-conflict",
                "payout-conflict-a",
                "bank-conflict-a",
                capture.clone(),
                5,
                15,
                DAY + 400,
                DAY + 500,
            ),
            &payout_accounts,
            &mut ledger,
        )
        .unwrap();
    let entry_count = ledger.entry_count();
    let error = registry
        .apply(
            payout_request(
                "payout-event-conflict",
                "payout-conflict-b",
                "bank-conflict-b",
                capture,
                0,
                20,
                DAY + 600,
                DAY + 700,
            ),
            &payout_accounts,
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedPayoutError::SourceEventConflict(_)
    ));
    assert_eq!(ledger.entry_count(), entry_count);
    assert_eq!(registry.authorization_count(), 1);
}

#[test]
fn second_full_payout_for_same_payment_is_rejected_by_p06_business_key() {
    let (capture, mut ledger, _, _, payout_accounts) =
        authorized_capture("invoice-single-payout", 3, 1, DAY + 300, AccountKind::Asset);
    let mut registry = AuthorizedPayoutRegistry::new();
    registry
        .apply(
            payout_request(
                "payout-event-one",
                "payout-one",
                "bank-one",
                capture.clone(),
                5,
                15,
                DAY + 400,
                DAY + 500,
            ),
            &payout_accounts,
            &mut ledger,
        )
        .unwrap();
    let error = registry
        .apply(
            payout_request(
                "payout-event-two",
                "payout-two",
                "bank-two",
                capture,
                5,
                15,
                DAY + 600,
                DAY + 700,
            ),
            &payout_accounts,
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedPayoutError::Payout(PayoutError::LedgerCommit(_))
    ));
    assert_eq!(ledger.entry_count(), 3);
    assert_eq!(registry.authorization_count(), 1);
}

#[test]
fn canonical_p06_account_validation_remains_authoritative() {
    let (capture, mut ledger, _, _, payout_accounts) =
        authorized_capture("invoice-account", 3, 1, DAY + 300, AccountKind::Revenue);
    let mut registry = AuthorizedPayoutRegistry::new();
    let error = registry
        .apply(
            payout_request(
                "payout-event-account",
                "payout-account",
                "bank-account",
                capture,
                5,
                15,
                DAY + 400,
                DAY + 500,
            ),
            &payout_accounts,
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedPayoutError::Payout(PayoutError::AccountKindMismatch { .. })
    ));
    assert_eq!(ledger.entry_count(), 2);
    assert_eq!(registry.authorization_count(), 0);
}

#[test]
fn orphaned_canonical_p06_replay_is_an_invariant_violation() {
    let (capture, mut ledger, _, _, payout_accounts) =
        authorized_capture("invoice-orphan", 3, 1, DAY + 300, AccountKind::Asset);
    let payment_event = capture.payment_event();
    let raw_event = ProcessorPayoutEvent::new(
        ProcessorPayoutEventId::new("payout-event-orphan").unwrap(),
        payment_event.organization_scope().clone(),
        payment_event.payment_id().clone(),
        ProcessorPayoutId::new("payout-orphan").unwrap(),
        BankTransactionReference::new("bank-orphan").unwrap(),
        payment_event.currency(),
        payment_event.amount_captured_minor(),
        5,
        15,
        DAY + 400,
        DAY + 500,
    );
    let raw_outcome = PayoutLedgerBridge::new()
        .apply(&raw_event, &payout_accounts, &mut ledger)
        .unwrap();
    assert!(matches!(raw_outcome, PayoutApplyOutcome::Committed { .. }));

    let mut registry = AuthorizedPayoutRegistry::new();
    let error = registry
        .apply(
            payout_request(
                "payout-event-orphan",
                "payout-orphan",
                "bank-orphan",
                capture,
                5,
                15,
                DAY + 400,
                DAY + 500,
            ),
            &payout_accounts,
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedPayoutError::UnexpectedInternalReplay { .. }
    ));
    assert_eq!(ledger.entry_count(), 3);
    assert_eq!(registry.authorization_count(), 0);
}
