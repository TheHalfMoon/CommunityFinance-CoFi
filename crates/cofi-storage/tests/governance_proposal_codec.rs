#![allow(clippy::unwrap_used)]
use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundId, Membership, MembershipId,
    MembershipRole, MembershipStatus, Organization, OrganizationId, Party, PartyId, PartyKind,
};
use cofi_governance::{
    ProposalStatus, SpendingApprovalPolicy, SpendingApprovalPolicyId, SpendingProposal,
    SpendingProposalEventId, SpendingProposalId,
};
use cofi_ledger::{Account, AccountId, AccountKind, Currency, Ledger, LedgerScopeId};
use cofi_storage::governance_policy::encode_governance_policy;
use cofi_storage::governance_proposal::{
    decode_governance_proposal, encode_governance_proposal, replay_governance_proposals,
};
fn policy() -> SpendingApprovalPolicy {
    SpendingApprovalPolicy::new(
        SpendingApprovalPolicyId::new("policy-a").unwrap(),
        1,
        OrganizationId::new("org-a").unwrap(),
        CommunityId::new("community-a").unwrap(),
        FundId::new("fund-a").unwrap(),
        Currency::new("SAR").unwrap(),
        1000,
        1,
        vec![MembershipRole::Owner],
    )
    .unwrap()
}
fn original_proposal() -> SpendingProposal {
    SpendingProposal::new(
        SpendingProposalEventId::new("proposal-event-a").unwrap(),
        SpendingProposalId::new("proposal-a").unwrap(),
        SpendingApprovalPolicyId::new("policy-a").unwrap(),
        1,
        PartyId::new("requester").unwrap(),
        OrganizationId::new("org-a").unwrap(),
        CommunityId::new("community-a").unwrap(),
        FundId::new("fund-a").unwrap(),
        Currency::new("SAR").unwrap(),
        500,
        "vendor:invoice-7",
        1000,
        2000,
    )
    .unwrap()
}
fn original_community() -> CommunityRegistry {
    let mut ledger = Ledger::new();
    ledger
        .register_account(Account::new(
            AccountId::new("asset").unwrap(),
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
                AccountId::new("asset").unwrap(),
                Currency::new("SAR").unwrap(),
            ),
            &ledger,
        )
        .unwrap();
    registry
        .register_party(Party::new(
            PartyId::new("requester").unwrap(),
            OrganizationId::new("org-a").unwrap(),
            PartyKind::Person,
        ))
        .unwrap();
    registry
        .register_membership(Membership::new(
            MembershipId::new("member-requester").unwrap(),
            PartyId::new("requester").unwrap(),
            CommunityId::new("community-a").unwrap(),
            MembershipRole::Owner,
            MembershipStatus::Active,
        ))
        .unwrap();
    registry
}
#[test]
fn original_accepted_proposal_roundtrip_and_engine_status_rebuild() {
    let community = original_community();
    let policy_record = encode_governance_policy(&policy()).unwrap();
    let proposal = original_proposal();
    let original = encode_governance_proposal(&proposal).unwrap();
    assert_eq!(decode_governance_proposal(&original).unwrap(), proposal);
    let engine = replay_governance_proposals(
        [policy_record.as_slice()],
        [original.as_slice(), original.as_slice()],
        &community,
    )
    .unwrap();
    assert_eq!(engine.proposal_count(), 1);
    assert_eq!(
        engine.proposal_status(proposal.id()),
        Some(ProposalStatus::Pending)
    );
    assert!(engine.authorization(proposal.id()).is_none());
}
#[test]
fn missing_ancestor_or_changed_reused_proposal_source_fails_closed() {
    let community = original_community();
    let policy_record = encode_governance_policy(&policy()).unwrap();
    let source = encode_governance_proposal(&original_proposal()).unwrap();
    assert!(replay_governance_proposals([], [source.as_slice()], &community).is_err());
    let mut changed: serde_json::Value = serde_json::from_slice(&source).unwrap();
    changed["payload"]["amount_minor"] = serde_json::json!("501");
    let altered = serde_json::to_vec(&changed).unwrap();
    assert!(
        replay_governance_proposals(
            [policy_record.as_slice()],
            [source.as_slice(), altered.as_slice()],
            &community
        )
        .is_err()
    );
    assert!(
        replay_governance_proposals(
            [policy_record.as_slice()],
            [source.as_slice()],
            &CommunityRegistry::new()
        )
        .is_err()
    );
}
#[test]
fn invalid_timing_amount_member_policy_scope_and_schema_fail_closed() {
    let original = encode_governance_proposal(&original_proposal()).unwrap();
    let policy_record = encode_governance_policy(&policy()).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&original).unwrap();
    for (path, new_value) in [
        ("/schema_version", serde_json::json!(2)),
        ("/record_type", serde_json::json!("wrong")),
        ("/payload/id", serde_json::json!("")),
        ("/payload/amount_minor", serde_json::json!("0")),
        ("/payload/amount_minor", serde_json::json!("1.5")),
        (
            "/payload/amount_minor",
            serde_json::json!("170141183460469231731687303715884105728"),
        ),
        ("/payload/purpose_reference", serde_json::json!(" ")),
        ("/payload/created_at_unix_ms", serde_json::json!("2000")),
        ("/payload/expires_at_unix_ms", serde_json::json!("500")),
    ] {
        let mut altered = value.clone();
        *altered.pointer_mut(path).unwrap() = new_value;
        assert!(
            decode_governance_proposal(&serde_json::to_vec(&altered).unwrap()).is_err(),
            "{path}"
        );
    }
    for (path, new_value) in [
        ("/payload/policy_version", serde_json::json!(2)),
        ("/payload/requester_party_id", serde_json::json!("ghost")),
        (
            "/payload/community_id",
            serde_json::json!("other-community"),
        ),
        ("/payload/fund_id", serde_json::json!("other-fund")),
        ("/payload/organization_id", serde_json::json!("other-org")),
        ("/payload/currency", serde_json::json!("USD")),
        ("/payload/amount_minor", serde_json::json!("1001")),
    ] {
        let mut altered = value.clone();
        *altered.pointer_mut(path).unwrap() = new_value;
        let bytes = serde_json::to_vec(&altered).unwrap();
        assert!(
            replay_governance_proposals(
                [policy_record.as_slice()],
                [bytes.as_slice()],
                &original_community()
            )
            .is_err(),
            "{path}"
        );
    }
    let mut extraneous = value;
    extraneous["payload"]["unexpected"] = serde_json::json!(true);
    assert!(decode_governance_proposal(&serde_json::to_vec(&extraneous).unwrap()).is_err());
    assert!(decode_governance_proposal(&vec![b' '; 1024 * 1024 + 1]).is_err());
    let dupe = String::from_utf8(original).unwrap().replacen(
        r#""schema_version":1"#,
        r#""schema_version":1,"schema_version":1"#,
        1,
    );
    assert!(decode_governance_proposal(dupe.as_bytes()).is_err());
}
