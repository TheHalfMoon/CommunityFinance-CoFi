#![allow(clippy::unwrap_used)]

use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundId, Membership, MembershipId,
    MembershipRole, MembershipStatus, Organization, OrganizationId, Party, PartyId, PartyKind,
};
use cofi_governance::{
    ApprovalOutcome, GovernanceEngine, GovernanceError, ProposalStatus, ProposalSubmissionOutcome,
    SpendingApproval, SpendingApprovalEventId, SpendingApprovalId, SpendingApprovalPolicy,
    SpendingApprovalPolicyId, SpendingProposal, SpendingProposalEventId, SpendingProposalId,
};
use cofi_ledger::{Account, AccountId, AccountKind, Currency, Ledger, LedgerScopeId};

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}
fn eur() -> Currency {
    Currency::new("EUR").unwrap()
}
fn org(value: &str) -> OrganizationId {
    OrganizationId::new(value).unwrap()
}
fn community(value: &str) -> CommunityId {
    CommunityId::new(value).unwrap()
}
fn fund(value: &str) -> FundId {
    FundId::new(value).unwrap()
}
fn party(value: &str) -> PartyId {
    PartyId::new(value).unwrap()
}
fn membership(value: &str) -> MembershipId {
    MembershipId::new(value).unwrap()
}
fn account(value: &str) -> AccountId {
    AccountId::new(value).unwrap()
}
fn scope(value: &str) -> LedgerScopeId {
    LedgerScopeId::new(value).unwrap()
}
fn policy_id(value: &str) -> SpendingApprovalPolicyId {
    SpendingApprovalPolicyId::new(value).unwrap()
}
fn proposal_id(value: &str) -> SpendingProposalId {
    SpendingProposalId::new(value).unwrap()
}
fn proposal_event(value: &str) -> SpendingProposalEventId {
    SpendingProposalEventId::new(value).unwrap()
}
fn approval_id(value: &str) -> SpendingApprovalId {
    SpendingApprovalId::new(value).unwrap()
}
fn approval_event(value: &str) -> SpendingApprovalEventId {
    SpendingApprovalEventId::new(value).unwrap()
}

fn add_party(
    registry: &mut CommunityRegistry,
    id: &str,
    role: MembershipRole,
    status: MembershipStatus,
) {
    registry
        .register_party(Party::new(party(id), org("org-1"), PartyKind::Person))
        .unwrap();
    registry
        .register_membership(Membership::new(
            membership(&format!("membership-{id}")),
            party(id),
            community("community-1"),
            role,
            status,
        ))
        .unwrap();
}

fn fixture() -> (CommunityRegistry, Ledger) {
    let mut ledger = Ledger::new();
    ledger
        .register_account(Account::new(
            account("fund-account"),
            scope("org-1"),
            AccountKind::Asset,
            usd(),
        ))
        .unwrap();
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
    add_party(
        &mut registry,
        "owner",
        MembershipRole::Owner,
        MembershipStatus::Active,
    );
    add_party(
        &mut registry,
        "treasurer",
        MembershipRole::Treasurer,
        MembershipStatus::Active,
    );
    add_party(
        &mut registry,
        "member",
        MembershipRole::Member,
        MembershipStatus::Active,
    );
    add_party(
        &mut registry,
        "suspended",
        MembershipRole::Treasurer,
        MembershipStatus::Suspended,
    );
    (registry, ledger)
}

fn policy(version: u32) -> SpendingApprovalPolicy {
    SpendingApprovalPolicy::new(
        policy_id("policy-1"),
        version,
        org("org-1"),
        community("community-1"),
        fund("fund-1"),
        usd(),
        1_000,
        2,
        vec![MembershipRole::Treasurer, MembershipRole::Owner],
    )
    .unwrap()
}

fn proposal(event: &str, id: &str, requester: &str, amount: i128) -> SpendingProposal {
    SpendingProposal::new(
        proposal_event(event),
        proposal_id(id),
        policy_id("policy-1"),
        1,
        party(requester),
        org("org-1"),
        community("community-1"),
        fund("fund-1"),
        usd(),
        amount,
        "vendor:invoice-123",
        1_000,
        2_000,
    )
    .unwrap()
}

fn approval(
    event: &str,
    id: &str,
    proposal: &str,
    approver: &str,
    approved_at: i64,
) -> SpendingApproval {
    SpendingApproval::new(
        approval_event(event),
        approval_id(id),
        proposal_id(proposal),
        party(approver),
        approved_at,
    )
}

fn engine_with_policy(registry: &CommunityRegistry) -> GovernanceEngine {
    let mut engine = GovernanceEngine::new();
    engine.register_policy(registry, policy(1)).unwrap();
    engine
}
#[test]
fn policy_registration_is_versioned_and_canonical() {
    let (registry, _) = fixture();
    let mut engine = GovernanceEngine::new();
    let first = policy(1);
    assert_eq!(
        first.eligible_roles(),
        &[MembershipRole::Owner, MembershipRole::Treasurer]
    );
    engine.register_policy(&registry, first).unwrap();
    engine.register_policy(&registry, policy(2)).unwrap();
    assert!(matches!(
        engine.register_policy(&registry, policy(1)),
        Err(GovernanceError::DuplicatePolicyVersion { .. })
            | Err(GovernanceError::NonMonotonicPolicyVersion { .. })
    ));
}

#[test]
fn invalid_policy_parameters_fail_closed() {
    assert_eq!(
        SpendingApprovalPolicy::new(
            policy_id("bad"),
            0,
            org("org-1"),
            community("community-1"),
            fund("fund-1"),
            usd(),
            1_000,
            1,
            vec![MembershipRole::Owner],
        ),
        Err(GovernanceError::InvalidPolicyVersion)
    );
    assert!(matches!(
        SpendingApprovalPolicy::new(
            policy_id("bad"),
            1,
            org("org-1"),
            community("community-1"),
            fund("fund-1"),
            usd(),
            0,
            1,
            vec![MembershipRole::Owner],
        ),
        Err(GovernanceError::InvalidPolicyMaxAmount(0))
    ));
    assert_eq!(
        SpendingApprovalPolicy::new(
            policy_id("bad"),
            1,
            org("org-1"),
            community("community-1"),
            fund("fund-1"),
            usd(),
            1_000,
            0,
            vec![MembershipRole::Owner],
        ),
        Err(GovernanceError::InvalidApprovalQuorum)
    );
    assert_eq!(
        SpendingApprovalPolicy::new(
            policy_id("bad"),
            1,
            org("org-1"),
            community("community-1"),
            fund("fund-1"),
            usd(),
            1_000,
            1,
            vec![],
        ),
        Err(GovernanceError::EmptyEligibleRoles)
    );
}

#[test]
fn policy_boundary_mismatch_is_rejected() {
    let (registry, _) = fixture();
    let mut engine = GovernanceEngine::new();
    let bad_currency = SpendingApprovalPolicy::new(
        policy_id("currency"),
        1,
        org("org-1"),
        community("community-1"),
        fund("fund-1"),
        eur(),
        1_000,
        1,
        vec![MembershipRole::Owner],
    )
    .unwrap();
    assert!(matches!(
        engine.register_policy(&registry, bad_currency),
        Err(GovernanceError::CurrencyMismatch { .. })
    ));
    let missing_fund = SpendingApprovalPolicy::new(
        policy_id("missing"),
        1,
        org("org-1"),
        community("community-1"),
        fund("missing"),
        usd(),
        1_000,
        1,
        vec![MembershipRole::Owner],
    )
    .unwrap();
    assert_eq!(
        engine.register_policy(&registry, missing_fund),
        Err(GovernanceError::UnknownFund(fund("missing")))
    );
}

#[test]
fn active_member_submits_and_exact_replay_is_idempotent() {
    let (registry, _) = fixture();
    let mut engine = engine_with_policy(&registry);
    let p = proposal("proposal-event-1", "proposal-1", "owner", 500);
    assert_eq!(
        engine.submit_proposal(&registry, p.clone()).unwrap(),
        ProposalSubmissionOutcome::Submitted
    );
    assert_eq!(
        engine.submit_proposal(&registry, p).unwrap(),
        ProposalSubmissionOutcome::Replayed
    );
    assert_eq!(engine.proposal_count(), 1);
    assert_eq!(
        engine.proposal_status(&proposal_id("proposal-1")),
        Some(ProposalStatus::Pending)
    );
}
#[test]
fn requester_membership_and_policy_limits_are_enforced() {
    let (registry, _) = fixture();
    let mut engine = engine_with_policy(&registry);
    assert!(matches!(
        engine.submit_proposal(
            &registry,
            proposal("suspended-event", "suspended-proposal", "suspended", 100)
        ),
        Err(GovernanceError::InactiveMembership { .. })
    ));
    let nonmember = SpendingProposal::new(
        proposal_event("nonmember-event"),
        proposal_id("nonmember-proposal"),
        policy_id("policy-1"),
        1,
        party("ghost"),
        org("org-1"),
        community("community-1"),
        fund("fund-1"),
        usd(),
        100,
        "purpose",
        1_000,
        2_000,
    )
    .unwrap();
    assert!(matches!(
        engine.submit_proposal(&registry, nonmember),
        Err(GovernanceError::MissingActiveMembership { .. })
    ));
    assert_eq!(
        engine.submit_proposal(
            &registry,
            proposal("over-event", "over-proposal", "owner", 1_001)
        ),
        Err(GovernanceError::ProposalAmountExceedsPolicy {
            amount: 1_001,
            maximum: 1_000
        })
    );
    assert_eq!(engine.proposal_count(), 0);
}
#[test]
fn proposal_constructor_rejects_invalid_payload() {
    assert_eq!(
        SpendingProposal::new(
            proposal_event("bad-amount"),
            proposal_id("bad-amount"),
            policy_id("policy-1"),
            1,
            party("owner"),
            org("org-1"),
            community("community-1"),
            fund("fund-1"),
            usd(),
            0,
            "purpose",
            1_000,
            2_000,
        ),
        Err(GovernanceError::InvalidProposalAmount(0))
    );
    assert_eq!(
        SpendingProposal::new(
            proposal_event("bad-purpose"),
            proposal_id("bad-purpose"),
            policy_id("policy-1"),
            1,
            party("owner"),
            org("org-1"),
            community("community-1"),
            fund("fund-1"),
            usd(),
            100,
            "   ",
            1_000,
            2_000,
        ),
        Err(GovernanceError::EmptyPurposeReference)
    );
    assert!(matches!(
        SpendingProposal::new(
            proposal_event("bad-expiry"),
            proposal_id("bad-expiry"),
            policy_id("policy-1"),
            1,
            party("owner"),
            org("org-1"),
            community("community-1"),
            fund("fund-1"),
            usd(),
            100,
            "purpose",
            2_000,
            2_000,
        ),
        Err(GovernanceError::InvalidProposalExpiry { .. })
    ));
}
#[test]
fn proposal_boundary_and_identity_conflicts_fail_closed() {
    let (registry, _) = fixture();
    let mut engine = engine_with_policy(&registry);
    let first = proposal("shared-event", "proposal-a", "owner", 100);
    engine.submit_proposal(&registry, first).unwrap();
    let conflicting_event = proposal("shared-event", "proposal-b", "owner", 100);
    assert!(matches!(
        engine.submit_proposal(&registry, conflicting_event),
        Err(GovernanceError::ProposalEventConflict(_))
    ));
    let conflicting_id = SpendingProposal::new(
        proposal_event("new-event"),
        proposal_id("proposal-a"),
        policy_id("policy-1"),
        1,
        party("owner"),
        org("org-1"),
        community("community-1"),
        fund("fund-1"),
        usd(),
        200,
        "other-purpose",
        1_000,
        2_000,
    )
    .unwrap();
    assert!(matches!(
        engine.submit_proposal(&registry, conflicting_id),
        Err(GovernanceError::ProposalIdConflict(_))
    ));
    assert_eq!(engine.proposal_count(), 1);
}
#[test]
fn eligible_approvals_reach_quorum_and_create_authorization() {
    let (registry, ledger) = fixture();
    let initial_entries = ledger.entry_count();
    let mut engine = engine_with_policy(&registry);
    engine
        .submit_proposal(
            &registry,
            proposal("proposal-event", "proposal-1", "owner", 750),
        )
        .unwrap();
    assert_eq!(
        engine
            .approve(
                &registry,
                approval(
                    "approval-event-1",
                    "approval-1",
                    "proposal-1",
                    "treasurer",
                    1_500
                )
            )
            .unwrap(),
        ApprovalOutcome::Recorded {
            status: ProposalStatus::Pending
        }
    );
    assert_eq!(
        engine
            .approve(
                &registry,
                approval(
                    "approval-event-2",
                    "approval-2",
                    "proposal-1",
                    "owner",
                    1_600
                )
            )
            .unwrap(),
        ApprovalOutcome::Recorded {
            status: ProposalStatus::Approved
        }
    );
    let authorization = engine.authorization(&proposal_id("proposal-1")).unwrap();
    assert_eq!(authorization.policy_id().as_str(), "policy-1");
    assert_eq!(authorization.policy_version(), 1);
    assert_eq!(authorization.amount_minor(), 750);
    assert_eq!(authorization.purpose_reference(), "vendor:invoice-123");
    assert_eq!(
        authorization.approver_party_ids(),
        &[party("owner"), party("treasurer")]
    );
    assert_eq!(ledger.entry_count(), initial_entries);
}
#[test]
fn approval_role_membership_time_and_duplicate_party_are_enforced() {
    let (registry, _) = fixture();
    let mut engine = engine_with_policy(&registry);
    engine
        .submit_proposal(
            &registry,
            proposal("proposal-event", "proposal-1", "owner", 500),
        )
        .unwrap();
    assert!(matches!(
        engine.approve(
            &registry,
            approval(
                "member-event",
                "member-approval",
                "proposal-1",
                "member",
                1_500
            )
        ),
        Err(GovernanceError::IneligibleApprovalRole { .. })
    ));
    assert!(matches!(
        engine.approve(
            &registry,
            approval(
                "suspended-event",
                "suspended-approval",
                "proposal-1",
                "suspended",
                1_500
            )
        ),
        Err(GovernanceError::InactiveMembership { .. })
    ));
    assert_eq!(
        engine.approve(
            &registry,
            approval("early-event", "early-approval", "proposal-1", "owner", 999)
        ),
        Err(GovernanceError::ApprovalBeforeProposalCreation)
    );
    assert_eq!(
        engine.approve(
            &registry,
            approval("late-event", "late-approval", "proposal-1", "owner", 2_001)
        ),
        Err(GovernanceError::ApprovalAfterProposalExpiry)
    );
    engine
        .approve(
            &registry,
            approval(
                "owner-event",
                "owner-approval",
                "proposal-1",
                "owner",
                1_500,
            ),
        )
        .unwrap();
    assert!(matches!(
        engine.approve(
            &registry,
            approval(
                "owner-event-2",
                "owner-approval-2",
                "proposal-1",
                "owner",
                1_600
            )
        ),
        Err(GovernanceError::DuplicatePartyApproval { .. })
    ));
    assert_eq!(engine.approval_count(), 1);
    assert_eq!(
        engine.proposal_status(&proposal_id("proposal-1")),
        Some(ProposalStatus::Pending)
    );
}

#[test]
fn exact_approval_replay_works_after_terminal_approval() {
    let (registry, _) = fixture();
    let mut engine = engine_with_policy(&registry);
    engine
        .submit_proposal(
            &registry,
            proposal("proposal-event", "proposal-1", "owner", 500),
        )
        .unwrap();
    let owner = approval(
        "owner-event",
        "owner-approval",
        "proposal-1",
        "owner",
        1_500,
    );
    let treasurer = approval(
        "treasurer-event",
        "treasurer-approval",
        "proposal-1",
        "treasurer",
        1_600,
    );
    engine.approve(&registry, owner.clone()).unwrap();
    engine.approve(&registry, treasurer).unwrap();
    assert_eq!(
        engine.approve(&registry, owner).unwrap(),
        ApprovalOutcome::Replayed {
            status: ProposalStatus::Approved
        }
    );
    assert!(matches!(
        engine.approve(
            &registry,
            approval("late-new-event", "late-new", "proposal-1", "member", 1_700)
        ),
        Err(GovernanceError::ProposalAlreadyApproved(_))
    ));
    assert_eq!(engine.approval_count(), 2);
}

#[test]
fn approval_event_and_identity_conflicts_fail_closed() {
    let (registry, _) = fixture();
    let mut engine = engine_with_policy(&registry);
    engine
        .submit_proposal(
            &registry,
            proposal("proposal-event", "proposal-1", "owner", 500),
        )
        .unwrap();
    engine
        .approve(
            &registry,
            approval(
                "shared-approval-event",
                "approval-a",
                "proposal-1",
                "owner",
                1_500,
            ),
        )
        .unwrap();
    let before = engine.approval_count();
    assert!(matches!(
        engine.approve(
            &registry,
            approval(
                "shared-approval-event",
                "approval-b",
                "proposal-1",
                "treasurer",
                1_600
            ),
        ),
        Err(GovernanceError::ApprovalEventConflict(_))
    ));
    assert_eq!(engine.approval_count(), before);
}

#[test]
fn policy_version_cannot_move_backward() {
    let (registry, _) = fixture();
    let mut engine = GovernanceEngine::new();
    engine.register_policy(&registry, policy(1)).unwrap();
    engine.register_policy(&registry, policy(3)).unwrap();
    assert_eq!(
        engine.register_policy(&registry, policy(2)),
        Err(GovernanceError::NonMonotonicPolicyVersion {
            policy_id: policy_id("policy-1"),
            highest: 3,
            attempted: 2,
        })
    );
}

#[test]
fn proposal_must_match_exact_policy_boundary() {
    let (registry, _) = fixture();
    let mut engine = engine_with_policy(&registry);
    let bad_currency = SpendingProposal::new(
        proposal_event("bad-currency"),
        proposal_id("bad-currency"),
        policy_id("policy-1"),
        1,
        party("owner"),
        org("org-1"),
        community("community-1"),
        fund("fund-1"),
        eur(),
        100,
        "purpose",
        1_000,
        2_000,
    )
    .unwrap();
    assert_eq!(
        engine.submit_proposal(&registry, bad_currency),
        Err(GovernanceError::ProposalPolicyBoundaryMismatch)
    );
}

#[test]
fn approval_id_reuse_with_different_payload_fails_closed() {
    let (registry, _) = fixture();
    let mut engine = engine_with_policy(&registry);
    engine
        .submit_proposal(
            &registry,
            proposal("proposal-event", "proposal-1", "owner", 500),
        )
        .unwrap();
    engine
        .approve(
            &registry,
            approval("event-a", "shared-approval", "proposal-1", "owner", 1_500),
        )
        .unwrap();
    let before = engine.approval_count();
    assert!(matches!(
        engine.approve(
            &registry,
            approval(
                "event-b",
                "shared-approval",
                "proposal-1",
                "treasurer",
                1_600
            ),
        ),
        Err(GovernanceError::ApprovalIdConflict(_))
    ));
    assert_eq!(engine.approval_count(), before);
}
