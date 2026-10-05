#![allow(clippy::unwrap_used)]

use cofi_billing::{
    BillingApplyOutcome, BillingCustomerId, BillingEventId, BillingInvoiceId,
    BillingLedgerAccounts, BillingLedgerBridge,
};
use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundAllocationAccounts, FundAllocationBridge,
    FundAllocationError, FundAllocationEvent, FundAllocationEventId, FundAllocationId, FundId,
    Organization, OrganizationId,
};
use cofi_finalization_authorization::{
    AuthorizedFinalization, AuthorizedFinalizationRegistry, AuthorizedFinalizationRequest,
};
use cofi_fund_allocation_authorization::{
    AuthorizedFundAllocationError, AuthorizedFundAllocationOutcome,
    AuthorizedFundAllocationRegistry, AuthorizedFundAllocationRequest,
};
use cofi_invoice_authorization::{
    AuthorizedDraft, AuthorizedDraftRegistry, AuthorizedDraftRequest,
};
use cofi_ledger::{
    Account, AccountId, AccountKind, CommitOutcome, Currency, EntryMetadata, JournalEntry, Ledger,
    LedgerScopeId, Posting, Side,
};
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageAggregate,
    UsageEvent, UsageEventId, UsageValue, WindowSize,
};
use cofi_payment_authorization::{
    AuthorizedCapture, AuthorizedCaptureRegistry, AuthorizedCaptureRequest,
};
use cofi_payments::{
    BankTransactionReference, ConnectorTransactionId, PaymentEventId, PaymentId,
    PaymentLedgerAccounts, PayoutLedgerAccounts, ProcessorPayoutEventId, ProcessorPayoutId,
};
use cofi_payout_authorization::{
    AuthorizedPayout, AuthorizedPayoutRegistry, AuthorizedPayoutRequest,
};
use cofi_rating::{RatePlan, RatingEventId, RatingPlanId};
use cofi_rating_authorization::{AuthorizedRatingRegistry, AuthorizedRatingRequest};
use cofi_subscriptions::{
    SubscriptionEventId, SubscriptionId, SubscriptionRegistry, SubscriptionRequest,
};

const DAY: i64 = 86_400_000;
const PAYOUT_AT: i64 = DAY + 400;

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}

fn scope() -> LedgerScopeId {
    LedgerScopeId::new("org-1").unwrap()
}

fn account_id(value: &str) -> AccountId {
    AccountId::new(value).unwrap()
}

fn org_id(value: &str) -> OrganizationId {
    OrganizationId::new(value).unwrap()
}

fn community_id(value: &str) -> CommunityId {
    CommunityId::new(value).unwrap()
}

fn fund_id(value: &str) -> FundId {
    FundId::new(value).unwrap()
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

fn authorized_draft(invoice_id: &str) -> AuthorizedDraft {
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
                customer("customer-1"),
                subject(&subject_name),
                plan(&plan_id, &meter_name, 1),
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
                customer("customer-1"),
                count_aggregate(&meter_name, &subject_name, 3),
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

fn authorized_finalization(invoice_id: &str) -> AuthorizedFinalization {
    let draft = authorized_draft(invoice_id);
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

fn register_account(ledger: &mut Ledger, id: &str, kind: AccountKind) {
    ledger
        .register_account(Account::new(account_id(id), scope(), kind, usd()))
        .unwrap();
}

fn ledger_and_accounts() -> (
    Ledger,
    BillingLedgerAccounts,
    PaymentLedgerAccounts,
    PayoutLedgerAccounts,
) {
    let mut ledger = Ledger::new();
    register_account(&mut ledger, "accounts-receivable", AccountKind::Asset);
    register_account(&mut ledger, "revenue", AccountKind::Revenue);
    register_account(&mut ledger, "processor-clearing", AccountKind::Asset);
    register_account(&mut ledger, "bank-cash", AccountKind::Asset);
    register_account(&mut ledger, "processor-fee-expense", AccountKind::Expense);
    register_account(&mut ledger, "fund-asset", AccountKind::Asset);
    register_account(&mut ledger, "fund-asset-2", AccountKind::Asset);
    register_account(&mut ledger, "other-asset", AccountKind::Asset);

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

fn authorized_capture(invoice_id: &str) -> (AuthorizedCapture, Ledger, PayoutLedgerAccounts) {
    let finalization = authorized_finalization(invoice_id);
    let (mut ledger, billing_accounts, payment_accounts, payout_accounts) = ledger_and_accounts();
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
                DAY + 300,
                DAY + 350,
            ),
            &payment_accounts,
            &mut ledger,
        )
        .unwrap()
        .authorization()
        .clone();

    (capture, ledger, payout_accounts)
}

fn authorized_payout(invoice_id: &str) -> (AuthorizedPayout, Ledger) {
    let (capture, mut ledger, payout_accounts) = authorized_capture(invoice_id);
    assert_eq!(capture.payment_event().amount_captured_minor(), 20);
    let mut registry = AuthorizedPayoutRegistry::new();
    let payout = registry
        .apply(
            AuthorizedPayoutRequest::new(
                ProcessorPayoutEventId::new(format!("payout-event-{invoice_id}")).unwrap(),
                ProcessorPayoutId::new(format!("payout-{invoice_id}")).unwrap(),
                BankTransactionReference::new(format!("bank-ref-{invoice_id}")).unwrap(),
                capture,
                5,
                15,
                PAYOUT_AT,
                PAYOUT_AT + 50,
            ),
            &payout_accounts,
            &mut ledger,
        )
        .unwrap()
        .authorization()
        .clone();
    (payout, ledger)
}

fn community_registry(ledger: &Ledger) -> CommunityRegistry {
    let mut registry = CommunityRegistry::new();
    registry
        .register_organization(Organization::new(org_id("org-1")))
        .unwrap();
    registry
        .register_community(Community::new(community_id("community-1"), org_id("org-1")))
        .unwrap();
    registry
        .register_fund(
            Fund::new(
                fund_id("fund-1"),
                community_id("community-1"),
                account_id("fund-asset"),
                usd(),
            ),
            ledger,
        )
        .unwrap();
    registry
        .register_fund(
            Fund::new(
                fund_id("fund-2"),
                community_id("community-1"),
                account_id("fund-asset-2"),
                usd(),
            ),
            ledger,
        )
        .unwrap();
    registry
}

fn request(
    event_id: &str,
    allocation_id: &str,
    fund: &str,
    payout: AuthorizedPayout,
    effective_at: i64,
    observed_at: i64,
) -> AuthorizedFundAllocationRequest {
    AuthorizedFundAllocationRequest::new(
        FundAllocationEventId::new(event_id).unwrap(),
        FundAllocationId::new(allocation_id).unwrap(),
        fund_id(fund),
        payout,
        effective_at,
        observed_at,
    )
}

#[test]
fn successful_full_allocation_derives_scope_currency_bank_and_net_amount() {
    let (payout, mut ledger) = authorized_payout("success");
    let community = community_registry(&ledger);
    let mut registry = AuthorizedFundAllocationRegistry::new();
    let outcome = registry
        .apply(
            &community,
            request(
                "allocation-event-success",
                "allocation-success",
                "fund-1",
                payout.clone(),
                PAYOUT_AT + 100,
                PAYOUT_AT + 200,
            ),
            &mut ledger,
        )
        .unwrap();

    assert!(matches!(
        outcome,
        AuthorizedFundAllocationOutcome::Created { .. }
    ));
    let authorization = outcome.authorization();
    assert_eq!(authorization.authorized_payout(), &payout);
    assert_eq!(
        authorization.source_cash_account_id(),
        &account_id("bank-cash")
    );
    assert_eq!(
        authorization.allocation_event().organization_scope(),
        &scope()
    );
    assert_eq!(authorization.allocation_event().currency(), usd());
    assert_eq!(authorization.allocation_event().amount_minor(), 15);
    assert_eq!(
        authorization.allocation_event().fund_id(),
        &fund_id("fund-1")
    );
    assert_eq!(
        authorization.allocation_event().allocation_id().as_str(),
        "allocation-success"
    );
    assert_eq!(
        authorization
            .authorized_payout()
            .authorized_capture()
            .authorized_finalization()
            .authorized_draft()
            .authorizations()[0]
            .subscription()
            .id()
            .as_str(),
        "subscription-success"
    );

    let entry = ledger.entry(authorization.journal_entry_id()).unwrap();
    assert_eq!(entry.postings().len(), 2);
    assert!(entry.postings().iter().any(|posting| {
        posting.account_id() == &account_id("fund-asset")
            && posting.side() == Side::Debit
            && posting.amount().value() == 15
    }));
    assert!(entry.postings().iter().any(|posting| {
        posting.account_id() == &account_id("bank-cash")
            && posting.side() == Side::Credit
            && posting.amount().value() == 15
    }));
    assert_eq!(ledger.entry_count(), 4);
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.payout_allocation_count(), 1);
}

#[test]
fn exact_replay_is_historical_and_does_not_touch_current_state() {
    let (payout, mut ledger) = authorized_payout("replay");
    let community = community_registry(&ledger);
    let request = request(
        "allocation-event-replay",
        "allocation-replay",
        "fund-1",
        payout,
        PAYOUT_AT + 100,
        PAYOUT_AT + 200,
    );
    let mut registry = AuthorizedFundAllocationRegistry::new();
    let created = registry
        .apply(&community, request.clone(), &mut ledger)
        .unwrap();

    let empty_community = CommunityRegistry::new();
    let mut empty_ledger = Ledger::new();
    let replay = registry
        .apply(&empty_community, request, &mut empty_ledger)
        .unwrap();
    assert!(matches!(
        replay,
        AuthorizedFundAllocationOutcome::Replayed { .. }
    ));
    assert_eq!(replay.authorization(), created.authorization());
    assert_eq!(empty_ledger.entry_count(), 0);
}

#[test]
fn conflicting_source_event_fails_before_second_allocation() {
    let (payout, mut ledger) = authorized_payout("conflict");
    let community = community_registry(&ledger);
    let mut registry = AuthorizedFundAllocationRegistry::new();
    registry
        .apply(
            &community,
            request(
                "allocation-event-conflict",
                "allocation-first",
                "fund-1",
                payout.clone(),
                PAYOUT_AT + 100,
                PAYOUT_AT + 200,
            ),
            &mut ledger,
        )
        .unwrap();
    let before = ledger.entry_count();

    let error = registry
        .apply(
            &community,
            request(
                "allocation-event-conflict",
                "allocation-second",
                "fund-2",
                payout,
                PAYOUT_AT + 100,
                PAYOUT_AT + 200,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundAllocationError::SourceEventConflict(_)
    ));
    assert_eq!(ledger.entry_count(), before);
}

#[test]
fn one_p25_payout_cannot_create_two_full_allocation_histories() {
    let (payout, mut ledger) = authorized_payout("single-history");
    let community = community_registry(&ledger);
    let mut registry = AuthorizedFundAllocationRegistry::new();
    registry
        .apply(
            &community,
            request(
                "allocation-event-one",
                "allocation-one",
                "fund-1",
                payout.clone(),
                PAYOUT_AT + 100,
                PAYOUT_AT + 200,
            ),
            &mut ledger,
        )
        .unwrap();
    let before = ledger.entry_count();

    let error = registry
        .apply(
            &community,
            request(
                "allocation-event-two",
                "allocation-two",
                "fund-2",
                payout,
                PAYOUT_AT + 300,
                PAYOUT_AT + 400,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundAllocationError::PayoutAlreadyAllocated { .. }
    ));
    assert_eq!(ledger.entry_count(), before);
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.payout_allocation_count(), 1);
}

#[test]
fn p07_failure_reserves_nothing_and_corrected_retry_succeeds() {
    let (payout, mut ledger) = authorized_payout("retry");
    let community = community_registry(&ledger);
    let mut registry = AuthorizedFundAllocationRegistry::new();

    let error = registry
        .apply(
            &community,
            request(
                "allocation-event-retry",
                "allocation-retry",
                "missing-fund",
                payout.clone(),
                PAYOUT_AT + 100,
                PAYOUT_AT + 200,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundAllocationError::Allocation(FundAllocationError::UnknownFund(_))
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.payout_allocation_count(), 0);
    assert_eq!(ledger.entry_count(), 3);

    let corrected = registry
        .apply(
            &community,
            request(
                "allocation-event-retry",
                "allocation-retry",
                "fund-1",
                payout,
                PAYOUT_AT + 100,
                PAYOUT_AT + 200,
            ),
            &mut ledger,
        )
        .unwrap();
    assert!(matches!(
        corrected,
        AuthorizedFundAllocationOutcome::Created { .. }
    ));
}

#[test]
fn allocation_cannot_precede_canonical_payout() {
    let (payout, mut ledger) = authorized_payout("time");
    let community = community_registry(&ledger);
    let mut registry = AuthorizedFundAllocationRegistry::new();

    let error = registry
        .apply(
            &community,
            request(
                "allocation-event-time",
                "allocation-time",
                "fund-1",
                payout.clone(),
                PAYOUT_AT - 1,
                PAYOUT_AT + 10,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundAllocationError::AllocationPrecedesPayout { .. }
    ));
    assert_eq!(registry.authorization_count(), 0);

    registry
        .apply(
            &community,
            request(
                "allocation-event-time",
                "allocation-time",
                "fund-1",
                payout,
                PAYOUT_AT,
                PAYOUT_AT + 10,
            ),
            &mut ledger,
        )
        .unwrap();
}

#[test]
fn observation_cannot_precede_allocation_effective_time() {
    let (payout, mut ledger) = authorized_payout("observation-time");
    let community = community_registry(&ledger);
    let mut registry = AuthorizedFundAllocationRegistry::new();
    let effective = PAYOUT_AT + 100;

    let error = registry
        .apply(
            &community,
            request(
                "allocation-event-observation",
                "allocation-observation",
                "fund-1",
                payout.clone(),
                effective,
                effective - 1,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundAllocationError::ObservationPrecedesAllocation { .. }
    ));
    assert_eq!(registry.authorization_count(), 0);

    registry
        .apply(
            &community,
            request(
                "allocation-event-observation",
                "allocation-observation",
                "fund-1",
                payout,
                effective,
                effective,
            ),
            &mut ledger,
        )
        .unwrap();
}

#[test]
fn missing_canonical_payout_journal_fails_closed() {
    let (payout, _) = authorized_payout("missing-journal");
    let (mut ledger, _, _, _) = ledger_and_accounts();
    let community = community_registry(&ledger);
    let mut registry = AuthorizedFundAllocationRegistry::new();

    let error = registry
        .apply(
            &community,
            request(
                "allocation-event-missing",
                "allocation-missing",
                "fund-1",
                payout,
                PAYOUT_AT + 100,
                PAYOUT_AT + 200,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundAllocationError::MissingCanonicalPayoutJournal(_)
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(ledger.entry_count(), 0);
}

#[test]
fn structurally_inconsistent_payout_journal_fails_closed() {
    let (payout, _) = authorized_payout("malformed-journal");
    let mut ledger = Ledger::new();
    register_account(&mut ledger, "processor-clearing", AccountKind::Asset);
    register_account(&mut ledger, "bad-expense", AccountKind::Expense);
    register_account(&mut ledger, "fund-asset", AccountKind::Asset);
    register_account(&mut ledger, "fund-asset-2", AccountKind::Asset);
    let malformed = JournalEntry::new(
        payout.journal_entry_id().clone(),
        vec![
            Posting::new(account_id("bad-expense"), usd(), Side::Debit, 15).unwrap(),
            Posting::new(account_id("processor-clearing"), usd(), Side::Credit, 15).unwrap(),
        ],
        PAYOUT_AT,
        PAYOUT_AT + 50,
        EntryMetadata::new(
            Some("malformed-payout".to_owned()),
            Some("malformed-payout-event".to_owned()),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(ledger.commit(malformed).unwrap(), CommitOutcome::Committed);
    let community = community_registry(&ledger);
    let mut registry = AuthorizedFundAllocationRegistry::new();

    let error = registry
        .apply(
            &community,
            request(
                "allocation-event-malformed",
                "allocation-malformed",
                "fund-1",
                payout,
                PAYOUT_AT + 100,
                PAYOUT_AT + 200,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundAllocationError::MissingCanonicalSourceCashDebit(_)
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(ledger.entry_count(), 1);
}

#[test]
fn insufficient_derived_bank_cash_propagates_from_p07_without_reservation() {
    let (payout, mut ledger) = authorized_payout("insufficient");
    let spend = JournalEntry::new(
        cofi_ledger::JournalEntryId::new("test:bank-reduction").unwrap(),
        vec![
            Posting::new(account_id("other-asset"), usd(), Side::Debit, 1).unwrap(),
            Posting::new(account_id("bank-cash"), usd(), Side::Credit, 1).unwrap(),
        ],
        PAYOUT_AT + 1,
        PAYOUT_AT + 2,
        EntryMetadata::new(
            Some("test-bank-reduction".to_owned()),
            Some("test-bank-reduction-event".to_owned()),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(ledger.commit(spend).unwrap(), CommitOutcome::Committed);
    let community = community_registry(&ledger);
    let mut registry = AuthorizedFundAllocationRegistry::new();

    let error = registry
        .apply(
            &community,
            request(
                "allocation-event-insufficient",
                "allocation-insufficient",
                "fund-1",
                payout,
                PAYOUT_AT + 100,
                PAYOUT_AT + 200,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundAllocationError::Allocation(FundAllocationError::InsufficientSourceCash {
            available: 14,
            requested: 15
        })
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.payout_allocation_count(), 0);
}

#[test]
fn orphaned_p07_replay_without_p26_history_is_invariant_violation() {
    let (payout, mut ledger) = authorized_payout("orphaned-replay");
    let community = community_registry(&ledger);
    let direct_event = FundAllocationEvent::new(
        FundAllocationEventId::new("allocation-event-orphaned").unwrap(),
        scope(),
        fund_id("fund-1"),
        FundAllocationId::new("allocation-orphaned").unwrap(),
        usd(),
        15,
        PAYOUT_AT + 100,
        PAYOUT_AT + 200,
    );
    FundAllocationBridge::new()
        .apply(
            &community,
            &direct_event,
            &FundAllocationAccounts::new(account_id("bank-cash")),
            &mut ledger,
        )
        .unwrap();
    let before = ledger.entry_count();
    let mut registry = AuthorizedFundAllocationRegistry::new();

    let error = registry
        .apply(
            &community,
            request(
                "allocation-event-orphaned",
                "allocation-orphaned",
                "fund-1",
                payout,
                PAYOUT_AT + 100,
                PAYOUT_AT + 200,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundAllocationError::UnexpectedInternalReplay { .. }
    ));
    assert_eq!(ledger.entry_count(), before);
    assert_eq!(registry.authorization_count(), 0);
}
