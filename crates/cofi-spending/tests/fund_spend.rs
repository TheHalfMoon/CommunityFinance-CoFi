#![allow(clippy::unwrap_used)]

use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundId, Membership, MembershipId,
    MembershipRole, MembershipStatus, Organization, OrganizationId, Party, PartyId, PartyKind,
};
use cofi_governance::{
    GovernanceEngine, SpendingApproval, SpendingApprovalEventId, SpendingApprovalId,
    SpendingApprovalPolicy, SpendingApprovalPolicyId, SpendingProposal, SpendingProposalEventId,
    SpendingProposalId,
};
use cofi_ledger::{
    Account, AccountId, AccountKind, Currency, EntryMetadata, JournalEntry, JournalEntryId, Ledger,
    LedgerScopeId, LedgerStateError, Posting, Side,
};
use cofi_spending::{
    ApprovedFundSpendEvent, FundSpendBridge, FundSpendError, FundSpendEventId, FundSpendId,
    FundSpendOutcome,
};

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}
fn eur() -> Currency {
    Currency::new("EUR").unwrap()
}
fn org(v: &str) -> OrganizationId {
    OrganizationId::new(v).unwrap()
}
fn community(v: &str) -> CommunityId {
    CommunityId::new(v).unwrap()
}
fn fund(v: &str) -> FundId {
    FundId::new(v).unwrap()
}
fn party(v: &str) -> PartyId {
    PartyId::new(v).unwrap()
}
fn membership(v: &str) -> MembershipId {
    MembershipId::new(v).unwrap()
}
fn account(v: &str) -> AccountId {
    AccountId::new(v).unwrap()
}
fn scope(v: &str) -> LedgerScopeId {
    LedgerScopeId::new(v).unwrap()
}
fn policy_id(v: &str) -> SpendingApprovalPolicyId {
    SpendingApprovalPolicyId::new(v).unwrap()
}
fn proposal_id(v: &str) -> SpendingProposalId {
    SpendingProposalId::new(v).unwrap()
}
fn spend_event_id(v: &str) -> FundSpendEventId {
    FundSpendEventId::new(v).unwrap()
}
fn spend_id(v: &str) -> FundSpendId {
    FundSpendId::new(v).unwrap()
}

fn register_account(
    ledger: &mut Ledger,
    id: &str,
    organization: &str,
    kind: AccountKind,
    currency: Currency,
) {
    ledger
        .register_account(Account::new(
            account(id),
            scope(organization),
            kind,
            currency,
        ))
        .unwrap();
}
fn add_member(registry: &mut CommunityRegistry, id: &str, role: MembershipRole) {
    registry
        .register_party(Party::new(party(id), org("org-1"), PartyKind::Person))
        .unwrap();
    registry
        .register_membership(Membership::new(
            membership(&format!("membership-{id}")),
            party(id),
            community("community-1"),
            role,
            MembershipStatus::Active,
        ))
        .unwrap();
}

fn seed_fund(ledger: &mut Ledger, amount: i128) {
    let metadata = EntryMetadata::new(
        Some("seed-fund".to_owned()),
        Some("seed-fund-event".to_owned()),
    )
    .unwrap()
    .with_business_key(Some("seed-fund-business".to_owned()))
    .unwrap();
    let entry = JournalEntry::new(
        JournalEntryId::new("seed-fund-entry").unwrap(),
        vec![
            Posting::new(account("fund-account"), usd(), Side::Debit, amount).unwrap(),
            Posting::new(account("opening-equity"), usd(), Side::Credit, amount).unwrap(),
        ],
        500,
        500,
        metadata,
    )
    .unwrap();
    ledger.commit(entry).unwrap();
}

struct Fixture {
    registry: CommunityRegistry,
    governance: GovernanceEngine,
    ledger: Ledger,
}

fn fixture(seed_amount: i128, approvals: usize) -> Fixture {
    let mut ledger = Ledger::new();
    register_account(
        &mut ledger,
        "fund-account",
        "org-1",
        AccountKind::Asset,
        usd(),
    );
    register_account(
        &mut ledger,
        "expense-account",
        "org-1",
        AccountKind::Expense,
        usd(),
    );
    register_account(
        &mut ledger,
        "opening-equity",
        "org-1",
        AccountKind::Equity,
        usd(),
    );
    register_account(
        &mut ledger,
        "asset-not-expense",
        "org-1",
        AccountKind::Asset,
        usd(),
    );
    register_account(
        &mut ledger,
        "expense-other-scope",
        "org-2",
        AccountKind::Expense,
        usd(),
    );
    register_account(
        &mut ledger,
        "expense-eur",
        "org-1",
        AccountKind::Expense,
        eur(),
    );
    seed_fund(&mut ledger, seed_amount);

    let mut registry = CommunityRegistry::new();
    registry
        .register_organization(Organization::new(org("org-1")))
        .unwrap();
    registry
        .register_community(Community::new(community("community-1"), org("org-1")))
        .unwrap();
    registry
        .register_fund(
            Fund::new(
                fund("fund-1"),
                community("community-1"),
                account("fund-account"),
                usd(),
            ),
            &ledger,
        )
        .unwrap();
    add_member(&mut registry, "owner", MembershipRole::Owner);
    add_member(&mut registry, "treasurer", MembershipRole::Treasurer);

    let mut governance = GovernanceEngine::new();
    governance
        .register_policy(
            &registry,
            SpendingApprovalPolicy::new(
                policy_id("policy-1"),
                1,
                org("org-1"),
                community("community-1"),
                fund("fund-1"),
                usd(),
                1_000,
                2,
                vec![MembershipRole::Owner, MembershipRole::Treasurer],
            )
            .unwrap(),
        )
        .unwrap();
    governance
        .submit_proposal(
            &registry,
            SpendingProposal::new(
                SpendingProposalEventId::new("proposal-event-1").unwrap(),
                proposal_id("proposal-1"),
                policy_id("policy-1"),
                1,
                party("owner"),
                org("org-1"),
                community("community-1"),
                fund("fund-1"),
                usd(),
                100,
                "vendor:invoice-123",
                1_000,
                2_000,
            )
            .unwrap(),
        )
        .unwrap();

    if approvals >= 1 {
        governance
            .approve(
                &registry,
                SpendingApproval::new(
                    SpendingApprovalEventId::new("approval-event-owner").unwrap(),
                    SpendingApprovalId::new("approval-owner").unwrap(),
                    proposal_id("proposal-1"),
                    party("owner"),
                    1_500,
                ),
            )
            .unwrap();
    }
    if approvals >= 2 {
        governance
            .approve(
                &registry,
                SpendingApproval::new(
                    SpendingApprovalEventId::new("approval-event-treasurer").unwrap(),
                    SpendingApprovalId::new("approval-treasurer").unwrap(),
                    proposal_id("proposal-1"),
                    party("treasurer"),
                    1_400,
                ),
            )
            .unwrap();
    }

    Fixture {
        registry,
        governance,
        ledger,
    }
}

#[allow(clippy::too_many_arguments)]
fn spend_event(
    source: &str,
    spend: &str,
    organization: &str,
    community_id: &str,
    fund_id: &str,
    currency: Currency,
    amount: i128,
    purpose: &str,
    expense: &str,
    executed_at: i64,
) -> ApprovedFundSpendEvent {
    ApprovedFundSpendEvent::new(
        spend_event_id(source),
        spend_id(spend),
        proposal_id("proposal-1"),
        org(organization),
        community(community_id),
        fund(fund_id),
        currency,
        amount,
        purpose,
        account(expense),
        executed_at,
        executed_at,
    )
    .unwrap()
}
fn default_event(source: &str, spend: &str, executed_at: i64) -> ApprovedFundSpendEvent {
    spend_event(
        source,
        spend,
        "org-1",
        "community-1",
        "fund-1",
        usd(),
        100,
        "vendor:invoice-123",
        "expense-account",
        executed_at,
    )
}

#[test]
fn approved_authorization_posts_exact_fund_expense_entry() {
    let mut fixture = fixture(1_000, 2);
    let authorization = fixture
        .governance
        .authorization(&proposal_id("proposal-1"))
        .unwrap();
    assert_eq!(authorization.approved_at_unix_ms(), 1_500);
    let event = default_event("spend-event-1", "spend-1", 1_500);
    let outcome = FundSpendBridge::new()
        .apply(
            &fixture.registry,
            &fixture.governance,
            &event,
            &mut fixture.ledger,
        )
        .unwrap();
    assert!(matches!(outcome, FundSpendOutcome::Committed { .. }));
    assert_eq!(fixture.ledger.entry_count(), 2);
    let fund_balance = fixture.ledger.balance(&account("fund-account")).unwrap();
    let expense_balance = fixture.ledger.balance(&account("expense-account")).unwrap();
    assert_eq!(fund_balance.debits(), 1_000);
    assert_eq!(fund_balance.credits(), 100);
    assert_eq!(expense_balance.debits(), 100);
    assert_eq!(expense_balance.credits(), 0);
}

#[test]
fn pending_or_unknown_authorization_fails_without_ledger_effect() {
    let mut fixture = fixture(1_000, 1);
    let before = fixture.ledger.entry_count();
    let event = default_event("spend-event-1", "spend-1", 1_500);
    let error = FundSpendBridge::new()
        .apply(
            &fixture.registry,
            &fixture.governance,
            &event,
            &mut fixture.ledger,
        )
        .unwrap_err();
    assert!(matches!(error, FundSpendError::UnknownAuthorization(_)));
    assert_eq!(fixture.ledger.entry_count(), before);

    let unknown = ApprovedFundSpendEvent::new(
        spend_event_id("spend-event-unknown"),
        spend_id("spend-unknown"),
        proposal_id("missing-proposal"),
        org("org-1"),
        community("community-1"),
        fund("fund-1"),
        usd(),
        100,
        "vendor:invoice-123",
        account("expense-account"),
        1_500,
        1_500,
    )
    .unwrap();
    assert!(matches!(
        FundSpendBridge::new().apply(
            &fixture.registry,
            &fixture.governance,
            &unknown,
            &mut fixture.ledger,
        ),
        Err(FundSpendError::UnknownAuthorization(_))
    ));
    assert_eq!(fixture.ledger.entry_count(), before);
}

#[test]
fn authorization_snapshot_mismatches_fail_closed() {
    let cases = [
        spend_event(
            "e1",
            "s1",
            "org-x",
            "community-1",
            "fund-1",
            usd(),
            100,
            "vendor:invoice-123",
            "expense-account",
            1_500,
        ),
        spend_event(
            "e2",
            "s2",
            "org-1",
            "community-x",
            "fund-1",
            usd(),
            100,
            "vendor:invoice-123",
            "expense-account",
            1_500,
        ),
        spend_event(
            "e3",
            "s3",
            "org-1",
            "community-1",
            "fund-x",
            usd(),
            100,
            "vendor:invoice-123",
            "expense-account",
            1_500,
        ),
        spend_event(
            "e4",
            "s4",
            "org-1",
            "community-1",
            "fund-1",
            eur(),
            100,
            "vendor:invoice-123",
            "expense-account",
            1_500,
        ),
        spend_event(
            "e5",
            "s5",
            "org-1",
            "community-1",
            "fund-1",
            usd(),
            101,
            "vendor:invoice-123",
            "expense-account",
            1_500,
        ),
        spend_event(
            "e6",
            "s6",
            "org-1",
            "community-1",
            "fund-1",
            usd(),
            100,
            "vendor:other",
            "expense-account",
            1_500,
        ),
    ];
    let mut fixture = fixture(1_000, 2);
    let before = fixture.ledger.entry_count();
    for event in cases {
        let error = FundSpendBridge::new()
            .apply(
                &fixture.registry,
                &fixture.governance,
                &event,
                &mut fixture.ledger,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            FundSpendError::AuthorizationSnapshotMismatch(_)
        ));
        assert_eq!(fixture.ledger.entry_count(), before);
    }
}

#[test]
fn execution_cannot_precede_quorum_readiness() {
    let mut fixture = fixture(1_000, 2);
    let event = default_event("spend-event-1", "spend-1", 1_499);
    let error = FundSpendBridge::new()
        .apply(
            &fixture.registry,
            &fixture.governance,
            &event,
            &mut fixture.ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        FundSpendError::ExecutionBeforeApproval {
            approved_at_unix_ms: 1_500,
            executed_at_unix_ms: 1_499,
        }
    ));
    assert_eq!(fixture.ledger.entry_count(), 1);
}

#[test]
fn expense_account_boundaries_fail_without_mutation() {
    for expense in [
        "missing-expense",
        "asset-not-expense",
        "expense-other-scope",
        "expense-eur",
    ] {
        let mut fixture = fixture(1_000, 2);
        let event = ApprovedFundSpendEvent::new(
            spend_event_id("spend-event-1"),
            spend_id("spend-1"),
            proposal_id("proposal-1"),
            org("org-1"),
            community("community-1"),
            fund("fund-1"),
            usd(),
            100,
            "vendor:invoice-123",
            account(expense),
            1_500,
            1_500,
        )
        .unwrap();
        let before = fixture.ledger.entry_count();
        assert!(
            FundSpendBridge::new()
                .apply(
                    &fixture.registry,
                    &fixture.governance,
                    &event,
                    &mut fixture.ledger,
                )
                .is_err()
        );
        assert_eq!(fixture.ledger.entry_count(), before);
    }
}

#[test]
fn same_fund_and_expense_account_fails_closed() {
    let mut fixture = fixture(1_000, 2);
    let event = spend_event(
        "spend-event-1",
        "spend-1",
        "org-1",
        "community-1",
        "fund-1",
        usd(),
        100,
        "vendor:invoice-123",
        "fund-account",
        1_500,
    );
    assert!(matches!(
        FundSpendBridge::new().apply(
            &fixture.registry,
            &fixture.governance,
            &event,
            &mut fixture.ledger,
        ),
        Err(FundSpendError::SamePostingAccount(_))
    ));
    assert_eq!(fixture.ledger.entry_count(), 1);
}

#[test]
fn insufficient_balance_does_not_reserve_identity_and_retry_succeeds() {
    let mut fixture = fixture(50, 2);
    let event = default_event("spend-event-1", "spend-1", 1_500);
    assert!(matches!(
        FundSpendBridge::new().apply(
            &fixture.registry,
            &fixture.governance,
            &event,
            &mut fixture.ledger,
        ),
        Err(FundSpendError::InsufficientFundBalance { .. })
    ));
    assert_eq!(fixture.ledger.entry_count(), 1);
    let metadata = EntryMetadata::new(Some("top-up".to_owned()), Some("top-up-event".to_owned()))
        .unwrap()
        .with_business_key(Some("top-up-business".to_owned()))
        .unwrap();
    fixture
        .ledger
        .commit(
            JournalEntry::new(
                JournalEntryId::new("top-up-entry").unwrap(),
                vec![
                    Posting::new(account("fund-account"), usd(), Side::Debit, 100).unwrap(),
                    Posting::new(account("opening-equity"), usd(), Side::Credit, 100).unwrap(),
                ],
                1_400,
                1_400,
                metadata,
            )
            .unwrap(),
        )
        .unwrap();
    let outcome = FundSpendBridge::new()
        .apply(
            &fixture.registry,
            &fixture.governance,
            &event,
            &mut fixture.ledger,
        )
        .unwrap();
    assert!(matches!(outcome, FundSpendOutcome::Committed { .. }));
}

#[test]
fn exact_replay_succeeds_after_full_fund_depletion() {
    let mut fixture = fixture(100, 2);
    let event = default_event("spend-event-1", "spend-1", 1_500);
    let first = FundSpendBridge::new()
        .apply(
            &fixture.registry,
            &fixture.governance,
            &event,
            &mut fixture.ledger,
        )
        .unwrap();
    assert!(matches!(first, FundSpendOutcome::Committed { .. }));
    let second = FundSpendBridge::new()
        .apply(
            &fixture.registry,
            &fixture.governance,
            &event,
            &mut fixture.ledger,
        )
        .unwrap();
    assert!(matches!(second, FundSpendOutcome::Replayed { .. }));
    let balance = fixture.ledger.balance(&account("fund-account")).unwrap();
    assert_eq!(balance.debits(), 100);
    assert_eq!(balance.credits(), 100);
    assert_eq!(fixture.ledger.entry_count(), 2);
}

#[test]
fn same_authorization_cannot_create_second_financial_history() {
    let mut fixture = fixture(1_000, 2);
    let first = default_event("spend-event-1", "spend-1", 1_500);
    FundSpendBridge::new()
        .apply(
            &fixture.registry,
            &fixture.governance,
            &first,
            &mut fixture.ledger,
        )
        .unwrap();
    let second = default_event("spend-event-2", "spend-2", 1_501);
    let error = FundSpendBridge::new()
        .apply(
            &fixture.registry,
            &fixture.governance,
            &second,
            &mut fixture.ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        FundSpendError::LedgerCommit(LedgerStateError::BusinessKeyConflict(_))
    ));
    assert_eq!(fixture.ledger.entry_count(), 2);
}

#[test]
fn conflicting_spend_id_fails_before_second_commit() {
    let mut fixture = fixture(1_000, 2);
    let first = default_event("spend-event-1", "spend-1", 1_500);
    FundSpendBridge::new()
        .apply(
            &fixture.registry,
            &fixture.governance,
            &first,
            &mut fixture.ledger,
        )
        .unwrap();
    let conflict = default_event("spend-event-2", "spend-1", 1_501);
    assert!(matches!(
        FundSpendBridge::new().apply(
            &fixture.registry,
            &fixture.governance,
            &conflict,
            &mut fixture.ledger,
        ),
        Err(FundSpendError::SpendIdConflict(_))
    ));
    assert_eq!(fixture.ledger.entry_count(), 2);
}

#[test]
fn conflicting_source_event_reuse_fails_closed() {
    let mut fixture = fixture(1_000, 2);
    let first = default_event("shared-spend-event", "spend-1", 1_500);
    FundSpendBridge::new()
        .apply(
            &fixture.registry,
            &fixture.governance,
            &first,
            &mut fixture.ledger,
        )
        .unwrap();
    let conflict = default_event("shared-spend-event", "spend-2", 1_501);
    let error = FundSpendBridge::new()
        .apply(
            &fixture.registry,
            &fixture.governance,
            &conflict,
            &mut fixture.ledger,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        FundSpendError::LedgerCommit(LedgerStateError::IdempotencyConflict(_))
    ));
    assert_eq!(fixture.ledger.entry_count(), 2);
}

#[test]
fn invalid_event_payload_is_rejected_at_construction() {
    assert!(matches!(
        ApprovedFundSpendEvent::new(
            spend_event_id("event-zero"),
            spend_id("spend-zero"),
            proposal_id("proposal-1"),
            org("org-1"),
            community("community-1"),
            fund("fund-1"),
            usd(),
            0,
            "vendor:invoice-123",
            account("expense-account"),
            1_500,
            1_500,
        ),
        Err(FundSpendError::InvalidAmount(0))
    ));
    assert!(matches!(
        ApprovedFundSpendEvent::new(
            spend_event_id("event-empty-purpose"),
            spend_id("spend-empty-purpose"),
            proposal_id("proposal-1"),
            org("org-1"),
            community("community-1"),
            fund("fund-1"),
            usd(),
            100,
            "   ",
            account("expense-account"),
            1_500,
            1_500,
        ),
        Err(FundSpendError::EmptyPurposeReference)
    ));
}
#[test]
fn negative_effective_fund_balance_fails_closed() {
    let mut fixture = fixture(100, 2);
    let metadata = EntryMetadata::new(
        Some("prior-loss".to_owned()),
        Some("prior-loss-event".to_owned()),
    )
    .unwrap()
    .with_business_key(Some("prior-loss-business".to_owned()))
    .unwrap();
    fixture
        .ledger
        .commit(
            JournalEntry::new(
                JournalEntryId::new("prior-loss-entry").unwrap(),
                vec![
                    Posting::new(account("expense-account"), usd(), Side::Debit, 200).unwrap(),
                    Posting::new(account("fund-account"), usd(), Side::Credit, 200).unwrap(),
                ],
                1_200,
                1_200,
                metadata,
            )
            .unwrap(),
        )
        .unwrap();
    let before = fixture.ledger.entry_count();
    let event = default_event("spend-event-1", "spend-1", 1_500);
    assert!(matches!(
        FundSpendBridge::new().apply(
            &fixture.registry,
            &fixture.governance,
            &event,
            &mut fixture.ledger,
        ),
        Err(FundSpendError::FundNegativeBalance { .. })
    ));
    assert_eq!(fixture.ledger.entry_count(), before);
}
