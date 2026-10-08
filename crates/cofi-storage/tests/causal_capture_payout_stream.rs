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
use cofi_storage::payout_evidence::encode_payout_evidence;

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

use cofi_payout_authorization::{AuthorizedPayoutRegistry, AuthorizedPayoutRequest};
use cofi_storage::causal_capture_payout_stream::{
    CausalPaymentFlow, rebuild_causal_capture_payout_stream,
};
use cofi_storage::payout_evidence::decode_payout_evidence;
use serde_json::Value;

fn genesis_two_tenants() -> Ledger {
    let mut ledger = Ledger::new();
    for (tenant, suffix) in [("org-1", ""), ("org-2", "-2")] {
        let scope = LedgerScopeId::new(tenant).unwrap();
        for (name, kind) in [
            ("accounts-receivable", AccountKind::Asset),
            ("revenue", AccountKind::Revenue),
            ("processor-clearing", AccountKind::Asset),
            ("bank-cash", AccountKind::Asset),
            ("processor-fee-expense", AccountKind::Expense),
        ] {
            ledger
                .register_account(Account::new(
                    AccountId::new(format!("{name}{suffix}")).unwrap(),
                    scope.clone(),
                    kind,
                    Currency::new("SAR").unwrap(),
                ))
                .unwrap();
        }
    }
    ledger
}
fn other_tenant(receipt: &[u8]) -> Vec<u8> {
    fn rewrite(v: &mut Value) {
        match v {
            Value::String(s) => {
                for (old, new) in [
                    ("org-1", "org-2"),
                    ("invoice-1", "invoice-2"),
                    ("customer-1", "customer-2"),
                    ("subject-1", "subject-2"),
                    ("subscription-1", "subscription-2"),
                    ("payment-1", "payment-2"),
                    ("payout-1", "payout-2"),
                    ("rating-1", "rating-2"),
                    ("rate-1", "rate-2"),
                    ("sub-event", "second-sub-event"),
                    ("draft-event-1", "draft-event-2"),
                    ("finalize-event-1", "finalize-event-2"),
                    ("captured-event", "second-captured-event"),
                    ("processor-transaction-1", "processor-transaction-2"),
                    ("payout-source-1", "payout-source-2"),
                    ("bank-ref-1", "bank-ref-2"),
                    ("source-0", "second-source-0"),
                    ("source-1", "second-source-1"),
                    ("source-2", "second-source-2"),
                    ("accounts-receivable", "accounts-receivable-2"),
                    ("processor-clearing", "processor-clearing-2"),
                    ("bank-cash", "bank-cash-2"),
                    ("processor-fee-expense", "processor-fee-expense-2"),
                    ("revenue", "revenue-2"),
                ] {
                    if s.contains(old) {
                        *s = s.replace(old, new);
                    }
                }
            }
            Value::Array(xs) => xs.iter_mut().for_each(rewrite),
            Value::Object(xs) => xs.values_mut().for_each(rewrite),
            _ => {}
        }
    }
    let mut v: Value = serde_json::from_slice(receipt).unwrap();
    rewrite(&mut v);
    serde_json::to_vec(&v).unwrap()
}
fn input() -> (Vec<Vec<u8>>, Vec<BillingLedgerAccounts>, Ledger, Ledger) {
    let (event, payout_accounts, p24, _, _) = payout_fixture();
    let original = encode_payout_evidence(&event, &payout_accounts, &p24).unwrap();
    let other = other_tenant(&original);
    decode_payout_evidence(&other).unwrap();
    let receipts = vec![original, other];
    let billing = vec![
        BillingLedgerAccounts::new(
            AccountId::new("accounts-receivable").unwrap(),
            AccountId::new("revenue").unwrap(),
        )
        .unwrap(),
        BillingLedgerAccounts::new(
            AccountId::new("accounts-receivable-2").unwrap(),
            AccountId::new("revenue-2").unwrap(),
        )
        .unwrap(),
    ];
    let genesis = genesis_two_tenants();
    let mut reference = genesis.clone();
    let mut captures = AuthorizedCaptureRegistry::new();
    let mut payouts = AuthorizedPayoutRegistry::new();
    for (receipt, billing) in receipts.iter().zip(&billing) {
        let source = decode_payout_evidence(receipt).unwrap();
        let capture = source.capture().request();
        let invoice = capture
            .authorized_finalization()
            .finalized()
            .to_billing_event();
        BillingLedgerBridge::new()
            .apply(&invoice, billing, &mut reference)
            .unwrap();
        let auth = captures
            .capture(capture.clone(), source.capture().accounts(), &mut reference)
            .unwrap()
            .authorization()
            .clone();
        let ev = source.event();
        let payout = AuthorizedPayoutRequest::new(
            ev.source_event_id().clone(),
            ev.payout_id().clone(),
            ev.bank_transaction_reference().clone(),
            auth,
            ev.processor_fee_minor(),
            ev.net_amount_minor(),
            ev.paid_at_unix_ms(),
            ev.observed_at_unix_ms(),
        );
        payouts
            .apply(payout, source.accounts(), &mut reference)
            .unwrap();
    }
    (receipts, billing, genesis, reference)
}
#[test]
fn two_real_original_authorization_registries_rebuilt_across_two_tenants() {
    let (receipts, billing, genesis, reference) = input();
    let flows = [
        CausalPaymentFlow {
            sequence: 1,
            p25_receipt: &receipts[0],
            billing_accounts: &billing[0],
        },
        CausalPaymentFlow {
            sequence: 2,
            p25_receipt: &receipts[1],
            billing_accounts: &billing[1],
        },
    ];
    let restored = rebuild_causal_capture_payout_stream(&flows, &genesis, &reference).unwrap();
    assert_eq!(restored.captures().authorization_count(), 2);
    assert_eq!(restored.payouts().authorization_count(), 2);
    assert_eq!(restored.ledger().entry_count(), 6);
    assert_eq!(restored.flow_count(), 2);
    assert_eq!(genesis.entry_count(), 0);
}
#[test]
fn sequence_duplicates_and_incomplete_original_reference_fail_closed() {
    let (receipts, billing, genesis, reference) = input();
    for (a, b) in [(2, 3), (1, 1), (2, 1), (1, 3)] {
        let flows = [
            CausalPaymentFlow {
                sequence: a,
                p25_receipt: &receipts[0],
                billing_accounts: &billing[0],
            },
            CausalPaymentFlow {
                sequence: b,
                p25_receipt: &receipts[1],
                billing_accounts: &billing[1],
            },
        ];
        assert!(rebuild_causal_capture_payout_stream(&flows, &genesis, &reference).is_err());
    }
    let duplicate = [
        CausalPaymentFlow {
            sequence: 1,
            p25_receipt: &receipts[0],
            billing_accounts: &billing[0],
        },
        CausalPaymentFlow {
            sequence: 2,
            p25_receipt: &receipts[0],
            billing_accounts: &billing[0],
        },
    ];
    assert!(rebuild_causal_capture_payout_stream(&duplicate, &genesis, &reference).is_err());
    let incomplete = [CausalPaymentFlow {
        sequence: 1,
        p25_receipt: &receipts[0],
        billing_accounts: &billing[0],
    }];
    assert!(rebuild_causal_capture_payout_stream(&incomplete, &genesis, &reference).is_err());
    let all = [
        CausalPaymentFlow {
            sequence: 1,
            p25_receipt: &receipts[0],
            billing_accounts: &billing[0],
        },
        CausalPaymentFlow {
            sequence: 2,
            p25_receipt: &receipts[1],
            billing_accounts: &billing[1],
        },
    ];
    assert!(rebuild_causal_capture_payout_stream(&all, &reference, &reference).is_err());
}

#[test]
fn altered_capture_payout_source_and_cross_tenant_account_binding_fail_closed() {
    let (receipts, billing, genesis, reference) = input();
    for (path, value) in [
        ("/payload/processor_fee_minor", serde_json::json!("3")),
        ("/payload/paid_at_unix_ms", serde_json::json!("1")),
        (
            "/payload/p24_capture/payload/payment_id",
            serde_json::json!("forged"),
        ),
        ("/payload/organization_scope", serde_json::json!("org-1")),
        (
            "/payload/bank_cash_account_id",
            serde_json::json!("bank-cash"),
        ),
    ] {
        let mut altered: Value = serde_json::from_slice(&receipts[1]).unwrap();
        *altered.pointer_mut(path).unwrap() = value;
        let broken = serde_json::to_vec(&altered).unwrap();
        let flows = [
            CausalPaymentFlow {
                sequence: 1,
                p25_receipt: &receipts[0],
                billing_accounts: &billing[0],
            },
            CausalPaymentFlow {
                sequence: 2,
                p25_receipt: &broken,
                billing_accounts: &billing[1],
            },
        ];
        assert!(
            rebuild_causal_capture_payout_stream(&flows, &genesis, &reference).is_err(),
            "{path}"
        );
    }
    let flows = [
        CausalPaymentFlow {
            sequence: 1,
            p25_receipt: &receipts[0],
            billing_accounts: &billing[1],
        },
        CausalPaymentFlow {
            sequence: 2,
            p25_receipt: &receipts[1],
            billing_accounts: &billing[0],
        },
    ];
    assert!(rebuild_causal_capture_payout_stream(&flows, &genesis, &reference).is_err());
}
