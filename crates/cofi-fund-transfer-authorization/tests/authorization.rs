#![allow(clippy::unwrap_used)]

use cofi_billing::{
    BillingApplyOutcome, BillingCustomerId, BillingEventId, BillingInvoiceId,
    BillingLedgerAccounts, BillingLedgerBridge,
};
use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundAllocationEventId, FundAllocationId,
    FundId, FundTransferBridge, FundTransferError, FundTransferEvent, FundTransferEventId,
    FundTransferId, FundTransferOutcome, Organization, OrganizationId,
};
use cofi_finalization_authorization::{
    AuthorizedFinalization, AuthorizedFinalizationRegistry, AuthorizedFinalizationRequest,
};
use cofi_fund_allocation_authorization::{
    AuthorizedFundAllocation, AuthorizedFundAllocationRegistry, AuthorizedFundAllocationRequest,
};
use cofi_fund_transfer_authorization::{
    AuthorizedFundTransferError, AuthorizedFundTransferOutcome, AuthorizedFundTransferRegistry,
    AuthorizedFundTransferRequest,
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

fn allocation_request(
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

const ALLOCATION_AT: i64 = PAYOUT_AT + 100;
const TRANSFER_AT: i64 = ALLOCATION_AT + 100;

fn authorized_allocation(
    invoice_id: &str,
) -> (AuthorizedFundAllocation, CommunityRegistry, Ledger) {
    let (payout, mut ledger) = authorized_payout(invoice_id);
    let community = community_registry(&ledger);
    let mut registry = AuthorizedFundAllocationRegistry::new();
    let authorization = registry
        .apply(
            &community,
            allocation_request(
                &format!("allocation-event-{invoice_id}"),
                &format!("allocation-{invoice_id}"),
                "fund-1",
                payout,
                ALLOCATION_AT,
                ALLOCATION_AT + 50,
            ),
            &mut ledger,
        )
        .unwrap()
        .authorization()
        .clone();
    (authorization, community, ledger)
}

fn transfer_request(
    event_id: &str,
    transfer_id: &str,
    destination_fund: &str,
    allocation: AuthorizedFundAllocation,
    effective_at: i64,
    observed_at: i64,
) -> AuthorizedFundTransferRequest {
    AuthorizedFundTransferRequest::new(
        FundTransferEventId::new(event_id).unwrap(),
        FundTransferId::new(transfer_id).unwrap(),
        fund_id(destination_fund),
        allocation,
        effective_at,
        observed_at,
    )
}

#[test]
fn successful_full_transfer_derives_source_scope_currency_amount_and_preserves_lineage() {
    let (allocation, community, mut ledger) = authorized_allocation("success");
    let mut registry = AuthorizedFundTransferRegistry::new();
    let outcome = registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-success",
                "transfer-success",
                "fund-2",
                allocation.clone(),
                TRANSFER_AT,
                TRANSFER_AT + 50,
            ),
            &mut ledger,
        )
        .unwrap();

    assert!(matches!(
        outcome,
        AuthorizedFundTransferOutcome::Created { .. }
    ));
    let authorization = outcome.authorization();
    assert_eq!(authorization.authorized_allocation(), &allocation);
    assert_eq!(
        authorization.transfer_event().organization_scope(),
        &scope()
    );
    assert_eq!(
        authorization.transfer_event().source_fund_id(),
        &fund_id("fund-1")
    );
    assert_eq!(
        authorization.transfer_event().destination_fund_id(),
        &fund_id("fund-2")
    );
    assert_eq!(authorization.transfer_event().currency(), usd());
    assert_eq!(authorization.transfer_event().amount_minor(), 15);
    assert_eq!(
        authorization.transfer_event().transfer_id().as_str(),
        "transfer-success"
    );
    assert_eq!(
        authorization
            .authorized_allocation()
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
        posting.account_id() == &account_id("fund-asset-2")
            && posting.side() == Side::Debit
            && posting.amount().value() == 15
    }));
    assert!(entry.postings().iter().any(|posting| {
        posting.account_id() == &account_id("fund-asset")
            && posting.side() == Side::Credit
            && posting.amount().value() == 15
    }));
    assert_eq!(ledger.entry_count(), 5);
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.allocation_transfer_count(), 1);
}

#[test]
fn exact_replay_is_historical_and_does_not_touch_current_state() {
    let (allocation, community, mut ledger) = authorized_allocation("replay");
    let request = transfer_request(
        "transfer-event-replay",
        "transfer-replay",
        "fund-2",
        allocation,
        TRANSFER_AT,
        TRANSFER_AT + 50,
    );
    let mut registry = AuthorizedFundTransferRegistry::new();
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
        AuthorizedFundTransferOutcome::Replayed { .. }
    ));
    assert_eq!(replay.authorization(), created.authorization());
    assert_eq!(empty_ledger.entry_count(), 0);
}

#[test]
fn conflicting_source_event_fails_before_second_transfer() {
    let (allocation, community, mut ledger) = authorized_allocation("conflict");
    let mut registry = AuthorizedFundTransferRegistry::new();
    registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-conflict",
                "transfer-first",
                "fund-2",
                allocation.clone(),
                TRANSFER_AT,
                TRANSFER_AT + 50,
            ),
            &mut ledger,
        )
        .unwrap();
    let before = ledger.entry_count();

    let error = registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-conflict",
                "transfer-second",
                "fund-2",
                allocation,
                TRANSFER_AT + 1,
                TRANSFER_AT + 51,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundTransferError::SourceEventConflict(_)
    ));
    assert_eq!(ledger.entry_count(), before);
}

#[test]
fn one_p26_allocation_cannot_create_two_full_transfer_histories() {
    let (allocation, community, mut ledger) = authorized_allocation("single-history");
    let mut registry = AuthorizedFundTransferRegistry::new();
    registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-one",
                "transfer-one",
                "fund-2",
                allocation.clone(),
                TRANSFER_AT,
                TRANSFER_AT + 50,
            ),
            &mut ledger,
        )
        .unwrap();
    let before = ledger.entry_count();

    let error = registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-two",
                "transfer-two",
                "fund-2",
                allocation,
                TRANSFER_AT + 1,
                TRANSFER_AT + 51,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundTransferError::AllocationAlreadyTransferred { .. }
    ));
    assert_eq!(ledger.entry_count(), before);
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.allocation_transfer_count(), 1);
}

#[test]
fn unknown_destination_reserves_nothing_and_corrected_retry_succeeds() {
    let (allocation, community, mut ledger) = authorized_allocation("retry-destination");
    let mut registry = AuthorizedFundTransferRegistry::new();
    let before = ledger.entry_count();

    let error = registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-retry-destination",
                "transfer-retry-destination",
                "missing-fund",
                allocation.clone(),
                TRANSFER_AT,
                TRANSFER_AT + 50,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundTransferError::Transfer(FundTransferError::UnknownFund(_))
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.allocation_transfer_count(), 0);
    assert_eq!(ledger.entry_count(), before);

    let corrected = registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-retry-destination",
                "transfer-retry-destination",
                "fund-2",
                allocation,
                TRANSFER_AT,
                TRANSFER_AT + 50,
            ),
            &mut ledger,
        )
        .unwrap();
    assert!(matches!(
        corrected,
        AuthorizedFundTransferOutcome::Created { .. }
    ));
}

#[test]
fn same_source_and_destination_propagates_p08_failure_without_reservation() {
    let (allocation, community, mut ledger) = authorized_allocation("same-fund");
    let mut registry = AuthorizedFundTransferRegistry::new();
    let before = ledger.entry_count();
    let error = registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-same-fund",
                "transfer-same-fund",
                "fund-1",
                allocation,
                TRANSFER_AT,
                TRANSFER_AT + 50,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundTransferError::Transfer(FundTransferError::SameFund(_))
    ));
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.allocation_transfer_count(), 0);
    assert_eq!(ledger.entry_count(), before);
}

#[test]
fn transfer_before_allocation_reserves_nothing_and_corrected_retry_succeeds() {
    let (allocation, community, mut ledger) = authorized_allocation("time");
    let mut registry = AuthorizedFundTransferRegistry::new();
    let error = registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-time",
                "transfer-time",
                "fund-2",
                allocation.clone(),
                ALLOCATION_AT - 1,
                TRANSFER_AT,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundTransferError::TransferPrecedesAllocation { .. }
    ));
    assert_eq!(registry.authorization_count(), 0);

    registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-time",
                "transfer-time",
                "fund-2",
                allocation,
                ALLOCATION_AT,
                TRANSFER_AT,
            ),
            &mut ledger,
        )
        .unwrap();
}

#[test]
fn observation_before_transfer_reserves_nothing_and_corrected_retry_succeeds() {
    let (allocation, community, mut ledger) = authorized_allocation("observation");
    let mut registry = AuthorizedFundTransferRegistry::new();
    let error = registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-observation",
                "transfer-observation",
                "fund-2",
                allocation.clone(),
                TRANSFER_AT,
                TRANSFER_AT - 1,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundTransferError::ObservationPrecedesTransfer { .. }
    ));
    assert_eq!(registry.authorization_count(), 0);

    registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-observation",
                "transfer-observation",
                "fund-2",
                allocation,
                TRANSFER_AT,
                TRANSFER_AT + 1,
            ),
            &mut ledger,
        )
        .unwrap();
}

#[test]
fn missing_canonical_allocation_journal_fails_before_p08_mutation() {
    let (allocation, _, _) = authorized_allocation("missing-journal");
    let (mut ledger, _, _, _) = ledger_and_accounts();
    let community = community_registry(&ledger);
    let mut registry = AuthorizedFundTransferRegistry::new();
    let before = ledger.entry_count();

    let error = registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-missing-journal",
                "transfer-missing-journal",
                "fund-2",
                allocation,
                TRANSFER_AT,
                TRANSFER_AT + 50,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundTransferError::MissingCanonicalAllocationJournal(_)
    ));
    assert_eq!(ledger.entry_count(), before);
    assert_eq!(registry.authorization_count(), 0);
}

#[test]
fn structurally_inconsistent_allocation_journal_fails_closed() {
    let (allocation, _, _) = authorized_allocation("malformed-journal");
    let (mut ledger, _, _, _) = ledger_and_accounts();
    let community = community_registry(&ledger);
    let metadata = EntryMetadata::new(
        Some("malformed-allocation-correlation".to_owned()),
        Some("malformed-allocation-source".to_owned()),
    )
    .unwrap()
    .with_business_key(Some("malformed-allocation-business".to_owned()))
    .unwrap();
    let malformed = JournalEntry::new(
        allocation.journal_entry_id().clone(),
        vec![
            Posting::new(account_id("other-asset"), usd(), Side::Debit, 15).unwrap(),
            Posting::new(account_id("bank-cash"), usd(), Side::Credit, 15).unwrap(),
        ],
        ALLOCATION_AT,
        ALLOCATION_AT + 50,
        metadata,
    )
    .unwrap();
    assert!(matches!(
        ledger.commit(malformed).unwrap(),
        CommitOutcome::Committed
    ));

    let mut registry = AuthorizedFundTransferRegistry::new();
    let before = ledger.entry_count();
    let error = registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-malformed-journal",
                "transfer-malformed-journal",
                "fund-2",
                allocation,
                TRANSFER_AT,
                TRANSFER_AT + 50,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundTransferError::MissingCanonicalSourceFundDebit { .. }
    ));
    assert_eq!(ledger.entry_count(), before);
    assert_eq!(registry.authorization_count(), 0);
}

#[test]
fn insufficient_source_fund_propagates_without_reserving_p27_identity() {
    let (allocation, community, mut ledger) = authorized_allocation("insufficient");
    let drain_event = FundTransferEvent::new(
        FundTransferEventId::new("pre-drain-event").unwrap(),
        scope(),
        fund_id("fund-1"),
        fund_id("fund-2"),
        FundTransferId::new("pre-drain-transfer").unwrap(),
        usd(),
        1,
        ALLOCATION_AT + 1,
        ALLOCATION_AT + 2,
    );
    assert!(matches!(
        FundTransferBridge::new()
            .apply(&community, &drain_event, &mut ledger)
            .unwrap(),
        FundTransferOutcome::Committed { .. }
    ));
    let before = ledger.entry_count();

    let mut registry = AuthorizedFundTransferRegistry::new();
    let error = registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-insufficient",
                "transfer-insufficient",
                "fund-2",
                allocation,
                TRANSFER_AT,
                TRANSFER_AT + 50,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundTransferError::Transfer(FundTransferError::InsufficientSourceFund {
            available: 14,
            requested: 15
        })
    ));
    assert_eq!(ledger.entry_count(), before);
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.allocation_transfer_count(), 0);
}

#[test]
fn orphaned_canonical_p08_replay_is_an_invariant_violation() {
    let (allocation, community, mut ledger) = authorized_allocation("orphan");
    let direct_event = FundTransferEvent::new(
        FundTransferEventId::new("transfer-event-orphan").unwrap(),
        allocation.allocation_event().organization_scope().clone(),
        allocation.allocation_event().fund_id().clone(),
        fund_id("fund-2"),
        FundTransferId::new("transfer-orphan").unwrap(),
        allocation.allocation_event().currency(),
        allocation.allocation_event().amount_minor(),
        TRANSFER_AT,
        TRANSFER_AT + 50,
    );
    assert!(matches!(
        FundTransferBridge::new()
            .apply(&community, &direct_event, &mut ledger)
            .unwrap(),
        FundTransferOutcome::Committed { .. }
    ));
    let before = ledger.entry_count();

    let mut registry = AuthorizedFundTransferRegistry::new();
    let error = registry
        .apply(
            &community,
            transfer_request(
                "transfer-event-orphan",
                "transfer-orphan",
                "fund-2",
                allocation,
                TRANSFER_AT,
                TRANSFER_AT + 50,
            ),
            &mut ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedFundTransferError::UnexpectedInternalReplay { .. }
    ));
    assert_eq!(ledger.entry_count(), before);
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.allocation_transfer_count(), 0);
}
