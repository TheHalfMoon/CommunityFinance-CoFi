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

fn genesis_and_billing() -> (Ledger, BillingLedgerAccounts) {
    let scope = LedgerScopeId::new("org-1").unwrap();
    let currency = Currency::new("SAR").unwrap();
    let mut ledger = Ledger::new();
    for (name, kind) in [
        ("accounts-receivable", AccountKind::Asset),
        ("revenue", AccountKind::Revenue),
        ("processor-clearing", AccountKind::Asset),
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
    let billing = BillingLedgerAccounts::new(
        AccountId::new("accounts-receivable").unwrap(),
        AccountId::new("revenue").unwrap(),
    )
    .unwrap();
    (ledger, billing)
}

use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundAllocationAccounts, FundAllocationBridge,
    FundAllocationEvent, FundAllocationEventId, FundAllocationId, FundAllocationOutcome, FundId,
    Organization, OrganizationId,
};
use cofi_storage::causal_allocation::rebuild_causal_allocation;
use cofi_storage::fund_movement::{FundMovementFact, encode_fund_movement};

fn allocation_fixture() -> (
    Vec<u8>,
    Vec<u8>,
    CommunityRegistry,
    BillingLedgerAccounts,
    Ledger,
    Ledger,
    Ledger,
) {
    let (payout_event, payout_accounts, p24, mut paid, _) = payout_fixture();
    let p25 = encode_payout_evidence(&payout_event, &payout_accounts, &p24).unwrap();
    let (mut genesis, billing) = genesis_and_billing();
    let scope = LedgerScopeId::new("org-1").unwrap();
    let c = Currency::new("SAR").unwrap();
    for ledger in [&mut genesis, &mut paid] {
        ledger
            .register_account(Account::new(
                AccountId::new("fund-asset").unwrap(),
                scope.clone(),
                AccountKind::Asset,
                c,
            ))
            .unwrap();
    }
    let mut community = CommunityRegistry::new();
    community
        .register_organization(Organization::new(OrganizationId::new("org-1").unwrap()))
        .unwrap();
    community
        .register_community(Community::new(
            CommunityId::new("community-1").unwrap(),
            OrganizationId::new("org-1").unwrap(),
        ))
        .unwrap();
    community
        .register_fund(
            Fund::new(
                FundId::new("fund-1").unwrap(),
                CommunityId::new("community-1").unwrap(),
                AccountId::new("fund-asset").unwrap(),
                c,
            ),
            &paid,
        )
        .unwrap();
    let before = paid.clone();
    let e = FundAllocationEvent::new(
        FundAllocationEventId::new("alloc-event-1").unwrap(),
        scope,
        FundId::new("fund-1").unwrap(),
        FundAllocationId::new("allocation-1").unwrap(),
        c,
        payout_event.net_amount_minor(),
        7 * DAY,
        7 * DAY + 100,
    );
    let fact = FundMovementFact::Allocation {
        event: e.clone(),
        source_cash: AccountId::new("bank-cash").unwrap(),
    };
    let bytes = encode_fund_movement(&fact).unwrap();
    assert!(matches!(
        FundAllocationBridge::new()
            .apply(
                &community,
                &e,
                &FundAllocationAccounts::new(AccountId::new("bank-cash").unwrap()),
                &mut paid
            )
            .unwrap(),
        FundAllocationOutcome::Committed { .. }
    ));
    (p25, bytes, community, billing, genesis, before, paid)
}
#[test]
fn causal_p23_p24_p25_p26_rebuilds_original_allocation_and_consumed_payout() {
    let (p25, p26, community, billing, genesis, before, after) = allocation_fixture();
    let replay =
        rebuild_causal_allocation(&p25, &p26, &billing, &community, &genesis, &before, &after)
            .unwrap();
    assert_eq!(replay.captures().authorization_count(), 1);
    assert_eq!(replay.payouts().authorization_count(), 1);
    assert_eq!(replay.allocations().authorization_count(), 1);
    assert_eq!(replay.allocations().payout_allocation_count(), 1);
    assert_eq!(replay.ledger().entry_count(), 4);
    assert_eq!(genesis.entry_count(), 0);
    assert_eq!(after.entry_count(), 4);
}
#[test]
fn bad_allocation_source_fund_scope_cash_and_timing_refuse_causal_replay() {
    let (p25, p26, community, billing, genesis, before, after) = allocation_fixture();
    let original: serde_json::Value = serde_json::from_slice(&p26).unwrap();
    for (pointer, value) in [
        ("/payload/source_event_id", serde_json::json!("forged")),
        ("/payload/fund_id", serde_json::json!("other-fund")),
        ("/payload/amount_minor", serde_json::json!("999")),
        (
            "/payload/source_cash",
            serde_json::json!("accounts-receivable"),
        ),
        (
            "/payload/organization_scope",
            serde_json::json!("other-tenant"),
        ),
        ("/payload/effective_at_unix_ms", serde_json::json!("1")),
        ("/payload/observed_at_unix_ms", serde_json::json!("1")),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(
            rebuild_causal_allocation(
                &p25,
                &serde_json::to_vec(&bad).unwrap(),
                &billing,
                &community,
                &genesis,
                &before,
                &after
            )
            .is_err(),
            "{pointer}"
        );
    }
}
#[test]
fn missing_historical_payout_or_extra_unknown_allocation_entry_rejects() {
    let (p25, p26, community, billing, genesis, before, after) = allocation_fixture();
    assert!(
        rebuild_causal_allocation(&p25, &p26, &billing, &community, &genesis, &after, &after)
            .is_err()
    );
    assert!(
        rebuild_causal_allocation(&p25, &p26, &billing, &community, &genesis, &before, &before)
            .is_err()
    );
    assert!(
        rebuild_causal_allocation(
            &p25,
            &p26,
            &billing,
            &CommunityRegistry::new(),
            &genesis,
            &before,
            &after
        )
        .is_err()
    );
}

#[test]
fn payout_authorization_consumption_index_blocks_second_fund_allocation() {
    use cofi_fund_allocation_authorization::{
        AuthorizedFundAllocationError, AuthorizedFundAllocationRequest,
    };
    use cofi_storage::payout_evidence::decode_payout_evidence;
    let (p25, p26, community, billing, genesis, before, after) = allocation_fixture();
    let rebuilt =
        rebuild_causal_allocation(&p25, &p26, &billing, &community, &genesis, &before, &after)
            .unwrap();
    let checked = decode_payout_evidence(&p25).unwrap();
    let original = rebuilt
        .payouts()
        .authorization_for_event(checked.event().source_event_id())
        .unwrap()
        .clone();
    let mut index = rebuilt.allocations().clone();
    let mut ledger = rebuilt.ledger().clone();
    let before_count = ledger.entry_count();
    let request = AuthorizedFundAllocationRequest::new(
        FundAllocationEventId::new("allocation-event-second").unwrap(),
        FundAllocationId::new("allocation-second").unwrap(),
        FundId::new("fund-1").unwrap(),
        original,
        8 * DAY,
        8 * DAY + 100,
    );
    let error = index.apply(&community, request, &mut ledger).unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundAllocationError::PayoutAlreadyAllocated { .. }
    ));
    assert_eq!(index.authorization_count(), 1);
    assert_eq!(index.payout_allocation_count(), 1);
    assert_eq!(ledger.entry_count(), before_count);
}
