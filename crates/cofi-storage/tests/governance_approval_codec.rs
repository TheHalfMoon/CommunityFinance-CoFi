#![allow(clippy::unwrap_used)]

use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundId, Membership, MembershipId,
    MembershipRole, MembershipStatus, Organization, OrganizationId, Party, PartyId, PartyKind,
};
use cofi_governance::{
    ApprovalOutcome, GovernanceEngine, ProposalStatus, SpendingApproval, SpendingApprovalEventId,
    SpendingApprovalId, SpendingApprovalPolicy, SpendingApprovalPolicyId, SpendingProposal,
    SpendingProposalEventId, SpendingProposalId,
};
use cofi_ledger::{Account, AccountId, AccountKind, Currency, Ledger, LedgerScopeId};
use cofi_storage::governance_approval::{
    decode_governance_approval, encode_governance_approval, replay_governance_approvals,
};
use cofi_storage::governance_policy::encode_governance_policy;
use cofi_storage::governance_proposal::encode_governance_proposal;
fn community() -> CommunityRegistry {
    let mut ledger = Ledger::new();
    ledger
        .register_account(Account::new(
            AccountId::new("fund-asset").unwrap(),
            LedgerScopeId::new("org-a").unwrap(),
            AccountKind::Asset,
            Currency::new("SAR").unwrap(),
        ))
        .unwrap();
    let mut registry = CommunityRegistry::new();
    registry
        .register_organization(Organization::new(OrganizationId::new("org-a").unwrap()))
        .unwrap();
    registry
        .register_community(Community::new(
            CommunityId::new("community-a").unwrap(),
            OrganizationId::new("org-a").unwrap(),
        ))
        .unwrap();
    registry
        .register_fund(
            Fund::new(
                FundId::new("fund-a").unwrap(),
                CommunityId::new("community-a").unwrap(),
                AccountId::new("fund-asset").unwrap(),
                Currency::new("SAR").unwrap(),
            ),
            &ledger,
        )
        .unwrap();
    for (name, role, status) in [
        ("owner", MembershipRole::Owner, MembershipStatus::Active),
        (
            "treasurer",
            MembershipRole::Treasurer,
            MembershipStatus::Active,
        ),
        (
            "suspended",
            MembershipRole::Treasurer,
            MembershipStatus::Suspended,
        ),
        ("member", MembershipRole::Member, MembershipStatus::Active),
    ] {
        registry
            .register_party(Party::new(
                PartyId::new(name).unwrap(),
                OrganizationId::new("org-a").unwrap(),
                PartyKind::Person,
            ))
            .unwrap();
        registry
            .register_membership(Membership::new(
                MembershipId::new(format!("membership-{name}")).unwrap(),
                PartyId::new(name).unwrap(),
                CommunityId::new("community-a").unwrap(),
                role,
                status,
            ))
            .unwrap();
    }
    registry
}
fn policy() -> SpendingApprovalPolicy {
    SpendingApprovalPolicy::new(
        SpendingApprovalPolicyId::new("policy-a").unwrap(),
        1,
        OrganizationId::new("org-a").unwrap(),
        CommunityId::new("community-a").unwrap(),
        FundId::new("fund-a").unwrap(),
        Currency::new("SAR").unwrap(),
        1000,
        2,
        vec![MembershipRole::Treasurer, MembershipRole::Owner],
    )
    .unwrap()
}
fn proposal() -> SpendingProposal {
    SpendingProposal::new(
        SpendingProposalEventId::new("proposal-event").unwrap(),
        SpendingProposalId::new("proposal-a").unwrap(),
        SpendingApprovalPolicyId::new("policy-a").unwrap(),
        1,
        PartyId::new("owner").unwrap(),
        OrganizationId::new("org-a").unwrap(),
        CommunityId::new("community-a").unwrap(),
        FundId::new("fund-a").unwrap(),
        Currency::new("SAR").unwrap(),
        750,
        "vendor:invoice-7",
        1000,
        2000,
    )
    .unwrap()
}
fn vote(event: &str, id: &str, approver: &str, time: i64) -> SpendingApproval {
    SpendingApproval::new(
        SpendingApprovalEventId::new(event).unwrap(),
        SpendingApprovalId::new(id).unwrap(),
        SpendingProposalId::new("proposal-a").unwrap(),
        PartyId::new(approver).unwrap(),
        time,
    )
}
fn parents() -> (Vec<u8>, Vec<u8>) {
    (
        encode_governance_policy(&policy()).unwrap(),
        encode_governance_proposal(&proposal()).unwrap(),
    )
}
#[test]
fn exact_original_vote_replay_rebuilds_pending_and_then_quorum_approval() {
    let registry = community();
    let (p, q) = parents();
    let v1 = vote("event-1", "vote-1", "owner", 1500);
    let v2 = vote("event-2", "vote-2", "treasurer", 1400);
    let e1 = encode_governance_approval(&v1).unwrap();
    let e2 = encode_governance_approval(&v2).unwrap();
    assert_eq!(decode_governance_approval(&e1).unwrap(), v1);
    let pending =
        replay_governance_approvals([p.as_slice()], [q.as_slice()], [e1.as_slice()], &registry)
            .unwrap();
    assert_eq!(
        pending.proposal_status(proposal().id()),
        Some(ProposalStatus::Pending)
    );
    assert!(pending.authorization(proposal().id()).is_none());
    let replayed = replay_governance_approvals(
        [p.as_slice()],
        [q.as_slice()],
        [e1.as_slice(), e2.as_slice(), e1.as_slice()],
        &registry,
    )
    .unwrap();
    assert_eq!(replayed.approval_count(), 2);
    assert_eq!(
        replayed.proposal_status(proposal().id()),
        Some(ProposalStatus::Approved)
    );
    let auth = replayed.authorization(proposal().id()).unwrap();
    assert_eq!(
        auth.approver_party_ids(),
        &[
            PartyId::new("owner").unwrap(),
            PartyId::new("treasurer").unwrap()
        ]
    );
    assert_eq!(auth.approved_at_unix_ms(), 1500);
    assert_eq!(auth.amount_minor(), 750);
    let mut reference = GovernanceEngine::new();
    reference.register_policy(&registry, policy()).unwrap();
    reference.submit_proposal(&registry, proposal()).unwrap();
    assert!(matches!(
        reference.approve(&registry, v1).unwrap(),
        ApprovalOutcome::Recorded {
            status: ProposalStatus::Pending
        }
    ));
    assert!(matches!(
        reference.approve(&registry, v2).unwrap(),
        ApprovalOutcome::Recorded {
            status: ProposalStatus::Approved
        }
    ));
    assert_eq!(
        replayed.authorization(proposal().id()),
        reference.authorization(proposal().id())
    );
}
#[test]
fn changed_duplicate_or_ineligible_approver_and_missing_ancestor_fail_closed() {
    let registry = community();
    let (p, q) = parents();
    let first = encode_governance_approval(&vote("event-1", "vote-1", "owner", 1500)).unwrap();
    let second = encode_governance_approval(&vote("event-2", "vote-2", "treasurer", 1400)).unwrap();
    assert!(
        replay_governance_approvals([p.as_slice()], [], [first.as_slice()], &registry).is_err()
    );
    assert!(
        replay_governance_approvals([], [q.as_slice()], [first.as_slice()], &registry).is_err()
    );
    for alt in [
        vote("event-1", "vote-changed", "treasurer", 1500),
        vote("event-changed", "vote-1", "treasurer", 1500),
        vote("event-2", "vote-2", "owner", 1400),
        vote("event-2", "vote-2", "member", 1400),
        vote("event-2", "vote-2", "suspended", 1400),
        vote("event-2", "vote-2", "treasurer", 900),
    ] {
        let modified = encode_governance_approval(&alt).unwrap();
        assert!(
            replay_governance_approvals(
                [p.as_slice()],
                [q.as_slice()],
                [first.as_slice(), modified.as_slice()],
                &registry
            )
            .is_err()
        );
    }
    let after = encode_governance_approval(&vote("event-3", "vote-3", "member", 1600)).unwrap();
    assert!(
        replay_governance_approvals(
            [p.as_slice()],
            [q.as_slice()],
            [first.as_slice(), second.as_slice(), after.as_slice()],
            &registry
        )
        .is_err()
    );
}
#[test]
fn invalid_source_fields_version_times_and_size_fail_closed() {
    let original = encode_governance_approval(&vote("event-1", "vote-1", "owner", 1500)).unwrap();
    let src: serde_json::Value = serde_json::from_slice(&original).unwrap();
    for (path, value) in [
        ("/schema_version", serde_json::json!(2)),
        ("/record_type", serde_json::json!("invalid")),
        ("/payload/source_event_id", serde_json::json!(" ")),
        ("/payload/id", serde_json::json!("")),
        ("/payload/approver_party_id", serde_json::json!("")),
        ("/payload/approved_at_unix_ms", serde_json::json!("1.5")),
        (
            "/payload/approved_at_unix_ms",
            serde_json::json!("9223372036854775808"),
        ),
    ] {
        let mut changed = src.clone();
        *changed.pointer_mut(path).unwrap() = value;
        assert!(
            decode_governance_approval(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{path}"
        );
    }
    let mut extra = src;
    extra["payload"]["forged"] = serde_json::json!(true);
    assert!(decode_governance_approval(&serde_json::to_vec(&extra).unwrap()).is_err());
    assert!(decode_governance_approval(&vec![b' '; 1024 * 1024 + 1]).is_err());
    let dup = String::from_utf8(original).unwrap().replacen(
        r#""schema_version":1"#,
        r#""schema_version":1,"schema_version":1"#,
        1,
    );
    assert!(decode_governance_approval(dup.as_bytes()).is_err());
}
