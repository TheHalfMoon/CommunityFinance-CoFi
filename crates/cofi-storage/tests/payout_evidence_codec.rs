#![allow(clippy::unwrap_used)]

use cofi_billing::{BillingCustomerId, BillingEventId, BillingInvoiceId};
use cofi_ledger::{Currency, LedgerScopeId};
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageEvent,
    UsageEventId, UsageValue, WindowSize,
};
use cofi_rating::{RatePlan, RatingEventId, RatingPlanId};
use cofi_rating_authorization::{
    AuthorizedRating, AuthorizedRatingRegistry, AuthorizedRatingRequest,
};
use cofi_storage::authorized_rating::encode_authorized_rating;
use cofi_storage::rating::{RatingAcceptance, encode_rating_acceptance};
use cofi_subscriptions::{
    SubscriptionEventId, SubscriptionId, SubscriptionRegistry, SubscriptionRequest,
};

const DAY: i64 = 86_400_000;

struct Fixture {
    subscription: SubscriptionRequest,
    evidence: Vec<u8>,
    accepted: AuthorizedRating,
}

fn fixture() -> Fixture {
    let meter = MeterDefinition::new(
        MeterId::new("api").unwrap(),
        EventType::new("call").unwrap(),
        Aggregation::Count,
        WindowSize::Day,
        Some(0),
    )
    .unwrap();
    let events = (0..3)
        .map(|idx| {
            UsageEvent::new(
                UsageEventId::new(format!("source-{idx}")).unwrap(),
                meter.id().clone(),
                EventType::new("call").unwrap(),
                SubjectId::new("subject-1").unwrap(),
                1_000 + idx,
                2_000 + idx,
                UsageValue::Count,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let plan = RatePlan::new(
        RatingPlanId::new("rate-1").unwrap(),
        meter.id().clone(),
        Currency::new("SAR").unwrap(),
        10,
        1,
        0,
        Some(2 * DAY),
    )
    .unwrap();
    let subscription = SubscriptionRequest::new(
        SubscriptionEventId::new("sub-event").unwrap(),
        SubscriptionId::new("subscription-1").unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        SubjectId::new("subject-1").unwrap(),
        plan.clone(),
        0,
        Some(DAY),
    )
    .unwrap();
    let mut subscriptions = SubscriptionRegistry::new();
    subscriptions.create(subscription.clone()).unwrap();
    let mut meter_engine = MeteringEngine::new();
    meter_engine.register_meter(meter.clone()).unwrap();
    for ev in &events {
        meter_engine.ingest(ev.clone()).unwrap();
    }
    let aggregate = meter_engine
        .aggregate(meter.id(), &SubjectId::new("subject-1").unwrap(), 0, DAY)
        .unwrap();
    let rating_id = RatingEventId::new("rating-1").unwrap();
    let customer = BillingCustomerId::new("customer-1").unwrap();
    let outcome = AuthorizedRatingRegistry::new()
        .rate(
            AuthorizedRatingRequest::new(
                rating_id.clone(),
                LedgerScopeId::new("org-1").unwrap(),
                customer.clone(),
                aggregate,
                DAY,
            ),
            &subscriptions,
        )
        .unwrap();
    let accepted = outcome.authorization().clone();
    let evidence = encode_rating_acceptance(&RatingAcceptance {
        meter: &meter,
        events: &events,
        plan: &plan,
        rating_event_id: &rating_id,
        subject_id: &SubjectId::new("subject-1").unwrap(),
        billing_customer_id: &customer,
        window_start_unix_ms: 0,
        window_end_unix_ms: DAY,
        rated_at_unix_ms: DAY,
        accepted_charge: accepted.charge(),
    })
    .unwrap();
    Fixture {
        subscription,
        evidence,
        accepted,
    }
}

use cofi_invoice_authorization::{
    AuthorizedDraft, AuthorizedDraftRegistry, AuthorizedDraftRequest,
};
use cofi_storage::authorized_draft::encode_authorized_draft;

fn p22_fixture() -> (AuthorizedDraftRequest, Vec<Vec<u8>>, AuthorizedDraft) {
    let f = fixture();
    let p21 = encode_authorized_rating(&f.subscription, &f.evidence, &f.accepted).unwrap();
    let request = AuthorizedDraftRequest::new(
        BillingEventId::new("draft-event-1").unwrap(),
        BillingInvoiceId::new("invoice-1").unwrap(),
        0,
        DAY,
        2 * DAY,
        vec![f.accepted],
    )
    .unwrap();
    let expected = AuthorizedDraftRegistry::new()
        .assemble(request.clone())
        .unwrap()
        .authorization()
        .clone();
    (request, vec![p21], expected)
}

use cofi_finalization_authorization::{
    AuthorizedFinalization, AuthorizedFinalizationRegistry, AuthorizedFinalizationRequest,
};
use cofi_storage::authorized_finalization::encode_authorized_finalization;

fn finalization_fixture() -> (
    AuthorizedFinalizationRequest,
    Vec<u8>,
    AuthorizedFinalization,
) {
    let (draft_request, ratings, draft) = p22_fixture();
    let p22 = encode_authorized_draft(&draft_request, &ratings, &draft).unwrap();
    let request = AuthorizedFinalizationRequest::new(
        BillingEventId::new("finalize-event-1").unwrap(),
        draft,
        3 * DAY,
        4 * DAY,
    );
    let expected = AuthorizedFinalizationRegistry::new()
        .finalize(request.clone())
        .unwrap()
        .authorization()
        .clone();
    (request, p22, expected)
}

use cofi_billing::{BillingApplyOutcome, BillingLedgerAccounts, BillingLedgerBridge};
use cofi_ledger::{Account, AccountId, AccountKind, Ledger};
use cofi_payment_authorization::{AuthorizedCaptureRegistry, AuthorizedCaptureRequest};
use cofi_payments::{ConnectorTransactionId, PaymentEventId, PaymentId, PaymentLedgerAccounts};
use cofi_storage::authorized_capture_evidence::encode_capture_evidence;

fn evidence_fixture() -> (
    AuthorizedCaptureRequest,
    Vec<u8>,
    PaymentLedgerAccounts,
    Ledger,
    Ledger,
) {
    let (p23_request, p22, finalization) = finalization_fixture();
    let p23 = encode_authorized_finalization(&p23_request, &p22, &finalization).unwrap();
    let scope = LedgerScopeId::new("org-1").unwrap();
    let c = Currency::new("SAR").unwrap();
    let mut ledger = Ledger::new();
    for (name, kind) in [
        ("accounts-receivable", AccountKind::Asset),
        ("revenue", AccountKind::Revenue),
        ("processor-clearing", AccountKind::Asset),
    ] {
        ledger
            .register_account(Account::new(
                AccountId::new(name).unwrap(),
                scope.clone(),
                kind,
                c,
            ))
            .unwrap();
    }
    let billing = BillingLedgerAccounts::new(
        AccountId::new("accounts-receivable").unwrap(),
        AccountId::new("revenue").unwrap(),
    )
    .unwrap();
    let accounts = PaymentLedgerAccounts::new(
        AccountId::new("accounts-receivable").unwrap(),
        AccountId::new("processor-clearing").unwrap(),
    )
    .unwrap();
    let billing_event = finalization.finalized().to_billing_event();
    assert!(matches!(
        BillingLedgerBridge::new()
            .apply(&billing_event, &billing, &mut ledger)
            .unwrap(),
        BillingApplyOutcome::Committed { .. }
    ));
    let prior = ledger.clone();
    let request = AuthorizedCaptureRequest::new(
        PaymentEventId::new("captured-event").unwrap(),
        PaymentId::new("payment-1").unwrap(),
        ConnectorTransactionId::new("processor-transaction-1").unwrap(),
        finalization,
        5 * DAY,
        5 * DAY + 100,
    );
    let mut captures = AuthorizedCaptureRegistry::new();
    let original = captures
        .capture(request.clone(), &accounts, &mut ledger)
        .unwrap()
        .authorization()
        .clone();
    assert_eq!(original.payment_event().payment_id(), request.payment_id());
    (request, p23, accounts, ledger, prior)
}

use cofi_payments::{
    BankTransactionReference, PayoutApplyOutcome, PayoutLedgerAccounts, PayoutLedgerBridge,
    ProcessorPayoutEvent, ProcessorPayoutEventId, ProcessorPayoutId,
};
use cofi_storage::payout_evidence::{
    decode_payout_evidence, encode_payout_evidence, verify_payout_evidence_history,
    verify_payout_journal,
};

fn payout_fixture() -> (
    ProcessorPayoutEvent,
    PayoutLedgerAccounts,
    Vec<u8>,
    Ledger,
    Ledger,
) {
    let (capture_request, p23, payment_accounts, mut ledger, _before_capture) = evidence_fixture();
    let p24 = encode_capture_evidence(&capture_request, &p23, &payment_accounts).unwrap();
    let scope = LedgerScopeId::new("org-1").unwrap();
    let currency = Currency::new("SAR").unwrap();
    for (name, kind) in [
        ("bank-cash", AccountKind::Asset),
        ("processor-fee-expense", AccountKind::Expense),
    ] {
        ledger
            .register_account(Account::new(
                AccountId::new(name).unwrap(),
                scope.clone(),
                kind,
                currency,
            ))
            .unwrap();
    }
    let payout_accounts = PayoutLedgerAccounts::new(
        AccountId::new("processor-clearing").unwrap(),
        AccountId::new("bank-cash").unwrap(),
        Some(AccountId::new("processor-fee-expense").unwrap()),
    )
    .unwrap();
    let gross = capture_request
        .authorized_finalization()
        .finalized()
        .draft()
        .total_minor();
    let event = ProcessorPayoutEvent::new(
        ProcessorPayoutEventId::new("payout-source-1").unwrap(),
        scope,
        capture_request.payment_id().clone(),
        ProcessorPayoutId::new("payout-1").unwrap(),
        BankTransactionReference::new("bank-ref-1").unwrap(),
        currency,
        gross,
        2,
        gross - 2,
        6 * DAY,
        6 * DAY + 100,
    );
    let pre = ledger.clone();
    assert!(matches!(
        PayoutLedgerBridge::new()
            .apply(&event, &payout_accounts, &mut ledger)
            .unwrap(),
        PayoutApplyOutcome::Committed { .. }
    ));
    (event, payout_accounts, p24, ledger, pre)
}

#[test]
fn checked_p24_ancestor_and_existing_original_payout_journal_replay() {
    let (event, accounts, p24, ledger, pre) = payout_fixture();
    let n = ledger.entry_count();
    let bytes = encode_payout_evidence(&event, &accounts, &p24).unwrap();
    let restored = decode_payout_evidence(&bytes).unwrap();
    assert_eq!(restored.event(), &event);
    assert_eq!(
        verify_payout_journal(&restored, &ledger).unwrap().as_str(),
        "payments:payout:payout-1:reconciled"
    );
    assert_eq!(
        verify_payout_evidence_history([bytes.as_slice(), bytes.as_slice()], &ledger).unwrap(),
        1
    );
    assert_eq!(ledger.entry_count(), n);
    assert!(verify_payout_journal(&restored, &pre).is_err());
}
#[test]
fn bad_p25_source_fee_payment_scope_bank_or_timing_cannot_reuse_journal() {
    let (event, accounts, p24, ledger, _) = payout_fixture();
    let original: serde_json::Value =
        serde_json::from_slice(&encode_payout_evidence(&event, &accounts, &p24).unwrap()).unwrap();
    for (path, value) in [
        (
            "/payload/source_event_id",
            serde_json::json!("another-payout-event"),
        ),
        ("/payload/payout_id", serde_json::json!("another-payout")),
        (
            "/payload/bank_transaction_reference",
            serde_json::json!("another-bank"),
        ),
        ("/payload/payment_id", serde_json::json!("other-payment")),
        ("/payload/gross_amount_minor", serde_json::json!("19")),
        ("/payload/processor_fee_minor", serde_json::json!("3")),
        ("/payload/net_amount_minor", serde_json::json!("17")),
        ("/payload/paid_at_unix_ms", serde_json::json!("1")),
        ("/payload/observed_at_unix_ms", serde_json::json!("1")),
        (
            "/payload/bank_cash_account_id",
            serde_json::json!("accounts-receivable"),
        ),
        (
            "/payload/p24_capture/payload/payment_id",
            serde_json::json!("other-payment"),
        ),
    ] {
        let mut x = original.clone();
        *x.pointer_mut(path).unwrap() = value;
        let bytes = serde_json::to_vec(&x).unwrap();
        if let Ok(changed) = decode_payout_evidence(&bytes) {
            assert!(verify_payout_journal(&changed, &ledger).is_err(), "{path}");
        }
    }
    let mut changed = original.clone();
    changed["payload"]["bank_transaction_reference"] = serde_json::json!("another");
    let second = serde_json::to_vec(&changed).unwrap();
    let first = serde_json::to_vec(&original).unwrap();
    assert!(
        verify_payout_evidence_history([first.as_slice(), second.as_slice()], &ledger).is_err()
    );
}
#[test]
fn invalid_nested_capture_payout_schema_and_overflows_fail_closed() {
    let (event, accounts, p24, ledger, _) = payout_fixture();
    let bytes = encode_payout_evidence(&event, &accounts, &p24).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (path, value) in [
        ("/schema_version", serde_json::json!(2)),
        ("/record_type", serde_json::json!("unknown")),
        ("/payload/source_event_id", serde_json::json!("")),
        ("/payload/gross_amount_minor", serde_json::json!("-5")),
        ("/payload/processor_fee_minor", serde_json::json!("999")),
        ("/payload/paid_at_unix_ms", serde_json::json!("1.2")),
        (
            "/payload/paid_at_unix_ms",
            serde_json::json!("9223372036854775808"),
        ),
        (
            "/payload/p24_capture/payload/authorized_finalization/payload/expected_total_minor",
            serde_json::json!("999"),
        ),
    ] {
        let mut x = original.clone();
        *x.pointer_mut(path).unwrap() = value;
        assert!(
            decode_payout_evidence(&serde_json::to_vec(&x).unwrap()).is_err(),
            "{path}"
        );
    }
    let mut added = original;
    added["payload"]["forged"] = serde_json::json!(true);
    assert!(decode_payout_evidence(&serde_json::to_vec(&added).unwrap()).is_err());
    assert!(decode_payout_evidence(&vec![b' '; 1024 * 1024 + 1]).is_err());
    let raw = String::from_utf8(bytes.clone()).unwrap();
    let duplicate = raw.replacen(
        r#""schema_version":1"#,
        r#""schema_version":1,"schema_version":1"#,
        1,
    );
    assert!(decode_payout_evidence(duplicate.as_bytes()).is_err());
    assert_eq!(
        verify_payout_evidence_history([bytes.as_slice()], &ledger).unwrap(),
        1
    );
}
