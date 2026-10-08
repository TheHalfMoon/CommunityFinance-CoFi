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

use cofi_community::{
    FundTransferBridge, FundTransferEvent, FundTransferEventId, FundTransferId, FundTransferOutcome,
};
use cofi_fund_transfer_authorization::{
    AuthorizedFundTransferError, AuthorizedFundTransferRequest,
};
use cofi_storage::causal_transfer::rebuild_causal_transfer;

#[allow(clippy::type_complexity)]
fn transfer_fixture() -> (
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    CommunityRegistry,
    BillingLedgerAccounts,
    Ledger,
    Ledger,
    Ledger,
    Ledger,
) {
    let (p25, p26, mut community, billing, mut genesis, mut after_p25, mut after_p26) =
        allocation_fixture();
    let scope = LedgerScopeId::new("org-1").unwrap();
    let currency = Currency::new("SAR").unwrap();
    for ledger in [&mut genesis, &mut after_p25, &mut after_p26] {
        ledger
            .register_account(Account::new(
                AccountId::new("fund-asset-2").unwrap(),
                scope.clone(),
                AccountKind::Asset,
                currency,
            ))
            .unwrap();
    }
    community
        .register_fund(
            Fund::new(
                FundId::new("fund-2").unwrap(),
                CommunityId::new("community-1").unwrap(),
                AccountId::new("fund-asset-2").unwrap(),
                currency,
            ),
            &after_p26,
        )
        .unwrap();
    let after_allocation = after_p26.clone();
    let transfer = FundTransferEvent::new(
        FundTransferEventId::new("transfer-event-1").unwrap(),
        scope,
        FundId::new("fund-1").unwrap(),
        FundId::new("fund-2").unwrap(),
        FundTransferId::new("transfer-1").unwrap(),
        currency,
        18,
        8 * DAY,
        8 * DAY + 100,
    );
    let p27 = encode_fund_movement(&FundMovementFact::Transfer(transfer.clone())).unwrap();
    assert!(matches!(
        FundTransferBridge::new()
            .apply(&community, &transfer, &mut after_p26)
            .unwrap(),
        FundTransferOutcome::Committed { .. }
    ));
    (
        p25,
        p26,
        p27,
        community,
        billing,
        genesis,
        after_p25,
        after_allocation,
        after_p26,
    )
}

#[test]
fn reconstructs_original_p23_to_p27_consumed_allocation_and_five_journals() {
    let (p25, p26, p27, community, billing, genesis, after25, after26, after27) =
        transfer_fixture();
    let restored = rebuild_causal_transfer(
        &p25, &p26, &p27, &billing, &community, &genesis, &after25, &after26, &after27,
    )
    .unwrap();
    assert_eq!(restored.captures().authorization_count(), 1);
    assert_eq!(restored.payouts().authorization_count(), 1);
    assert_eq!(restored.allocations().payout_allocation_count(), 1);
    assert_eq!(restored.transfers().authorization_count(), 1);
    assert_eq!(restored.transfers().allocation_transfer_count(), 1);
    assert_eq!(restored.ledger().entry_count(), 5);
    assert_eq!(genesis.entry_count(), 0);
}
#[test]
fn corrupted_transfer_source_or_missing_predecessor_refuses_causal_replay() {
    let (p25, p26, p27, community, billing, genesis, after25, after26, after27) =
        transfer_fixture();
    let original: serde_json::Value = serde_json::from_slice(&p27).unwrap();
    for (path, value) in [
        ("/payload/source_event_id", serde_json::json!("changed")),
        ("/payload/source_fund_id", serde_json::json!("fund-2")),
        ("/payload/destination_fund_id", serde_json::json!("unknown")),
        ("/payload/amount_minor", serde_json::json!("999")),
        (
            "/payload/organization_scope",
            serde_json::json!("other-org"),
        ),
        ("/payload/effective_at_unix_ms", serde_json::json!("1")),
        ("/payload/observed_at_unix_ms", serde_json::json!("1")),
    ] {
        let mut corrupted = original.clone();
        *corrupted.pointer_mut(path).unwrap() = value;
        let data = serde_json::to_vec(&corrupted).unwrap();
        assert!(
            rebuild_causal_transfer(
                &p25, &p26, &data, &billing, &community, &genesis, &after25, &after26, &after27
            )
            .is_err(),
            "{path}"
        );
    }
    assert!(
        rebuild_causal_transfer(
            &p25, &p26, &p27, &billing, &community, &genesis, &after25, &after27, &after26
        )
        .is_err()
    );
    assert!(
        rebuild_causal_transfer(
            &p25, &p26, &p27, &billing, &community, &genesis, &after25, &after27, &after27
        )
        .is_err()
    );
}
#[test]
fn allocated_fund_authority_cannot_be_consumed_for_second_transfer() {
    let (p25, p26, p27, community, billing, genesis, after25, after26, after27) =
        transfer_fixture();
    let recovered = rebuild_causal_transfer(
        &p25, &p26, &p27, &billing, &community, &genesis, &after25, &after26, &after27,
    )
    .unwrap();
    let receipt = cofi_storage::fund_movement::decode_fund_movement(&p26).unwrap();
    let event = match receipt {
        FundMovementFact::Allocation { event, .. } => Some(event),
        FundMovementFact::Transfer(_) => None,
    }
    .unwrap();
    let allocation = recovered
        .allocations()
        .authorization_for_event(event.source_event_id())
        .unwrap()
        .clone();
    let mut registry = recovered.transfers().clone();
    let mut ledger = recovered.ledger().clone();
    let before_count = ledger.entry_count();
    let request = AuthorizedFundTransferRequest::new(
        FundTransferEventId::new("transfer-event-second").unwrap(),
        FundTransferId::new("transfer-second").unwrap(),
        FundId::new("fund-2").unwrap(),
        allocation,
        9 * DAY,
        9 * DAY + 100,
    );
    let error = registry
        .apply(&community, request, &mut ledger)
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundTransferError::AllocationAlreadyTransferred { .. }
    ));
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.allocation_transfer_count(), 1);
    assert_eq!(ledger.entry_count(), before_count);
}
