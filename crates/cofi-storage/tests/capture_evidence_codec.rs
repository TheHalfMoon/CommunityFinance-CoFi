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
use cofi_storage::authorized_capture_evidence::{
    decode_capture_evidence, encode_capture_evidence, verify_capture_evidence_history,
    verify_capture_evidence_journal,
};

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

#[test]
fn checked_p23_ancestry_p24_source_and_original_journal_match_without_posting() {
    let (request, p23, accounts, ledger, prior) = evidence_fixture();
    let n = ledger.entry_count();
    let bytes = encode_capture_evidence(&request, &p23, &accounts).unwrap();
    let source = decode_capture_evidence(&bytes).unwrap();
    assert_eq!(source.request(), &request);
    assert_eq!(
        verify_capture_evidence_journal(&source, &ledger)
            .unwrap()
            .as_str(),
        "payments:payment:payment-1:charged"
    );
    assert_eq!(
        verify_capture_evidence_history([bytes.as_slice(), bytes.as_slice()], &ledger).unwrap(),
        1
    );
    assert_eq!(ledger.entry_count(), n);
    assert!(verify_capture_evidence_journal(&source, &prior).is_err());
}
#[test]
fn corrupt_original_p23_and_p24_source_values_are_rejected() {
    let (request, p23, accounts, ledger, _) = evidence_fixture();
    let bytes = encode_capture_evidence(&request, &p23, &accounts).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (pointer, value) in [
        ("/payload/payment_id", serde_json::json!("another-payment")),
        (
            "/payload/connector_transaction_id",
            serde_json::json!("another-connector"),
        ),
        ("/payload/source_event_id", serde_json::json!("other-event")),
        ("/payload/captured_at_unix_ms", serde_json::json!("1")),
        ("/payload/observed_at_unix_ms", serde_json::json!("1")),
        (
            "/payload/receivable_account_id",
            serde_json::json!("processor-clearing"),
        ),
        (
            "/payload/authorized_finalization/payload/expected_total_minor",
            serde_json::json!("9000"),
        ),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        let data = serde_json::to_vec(&changed).unwrap();
        if let Ok(decoded) = decode_capture_evidence(&data) {
            assert!(
                verify_capture_evidence_journal(&decoded, &ledger).is_err(),
                "{pointer}"
            );
        }
    }
    let mut another = original.clone();
    another["payload"]["captured_at_unix_ms"] = serde_json::json!((5 * DAY + 1).to_string());
    let another = serde_json::to_vec(&another).unwrap();
    assert!(
        verify_capture_evidence_history([bytes.as_slice(), another.as_slice()], &ledger).is_err()
    );
}

#[test]
fn unknown_schema_extra_fields_overflow_and_untrusted_account_fail_closed() {
    let (request, p23, accounts, ledger, _) = evidence_fixture();
    let bytes = encode_capture_evidence(&request, &p23, &accounts).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (p, v) in [
        ("/schema_version", serde_json::json!(2)),
        ("/record_type", serde_json::json!("unknown")),
        ("/payload/payment_id", serde_json::json!("")),
        ("/payload/observed_at_unix_ms", serde_json::json!("1.2")),
        (
            "/payload/observed_at_unix_ms",
            serde_json::json!("9223372036854775808"),
        ),
        (
            "/payload/processor_clearing_account_id",
            serde_json::json!(""),
        ),
    ] {
        let mut x = original.clone();
        *x.pointer_mut(p).unwrap() = v;
        assert!(
            decode_capture_evidence(&serde_json::to_vec(&x).unwrap()).is_err(),
            "{p}"
        );
    }
    let mut extra = original.clone();
    extra["payload"]["forged"] = serde_json::json!(true);
    assert!(decode_capture_evidence(&serde_json::to_vec(&extra).unwrap()).is_err());
    assert!(decode_capture_evidence(&vec![b' '; 1024 * 1024 + 1]).is_err());
    let text = String::from_utf8(bytes.clone()).unwrap();
    let duplicate = text.replacen(
        r#""schema_version":1"#,
        r#""schema_version":1,"schema_version":1"#,
        1,
    );
    assert!(decode_capture_evidence(duplicate.as_bytes()).is_err());
    assert_eq!(
        verify_capture_evidence_history([bytes.as_slice()], &ledger).unwrap(),
        1
    );
}
