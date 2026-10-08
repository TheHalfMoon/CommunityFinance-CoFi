#![allow(clippy::unwrap_used)]

use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundId, Membership, MembershipId,
    MembershipRole, MembershipStatus, Organization, OrganizationId, Party, PartyId, PartyKind,
};
use cofi_governance::{
    ProposalSubmissionOutcome, SpendingApprovalPolicy, SpendingApprovalPolicyId, SpendingProposal,
    SpendingProposalEventId, SpendingProposalId,
};
use cofi_ledger::{Account, AccountId, AccountKind, Currency, Ledger, LedgerScopeId};
use cofi_storage::governance_policy::{
    decode_governance_policy, encode_governance_policy, replay_governance_policies,
};

fn fixture() -> CommunityRegistry {
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
fn policy(version: u32, amount: i128) -> SpendingApprovalPolicy {
    SpendingApprovalPolicy::new(
        SpendingApprovalPolicyId::new("policy-a").unwrap(),
        version,
        OrganizationId::new("org-a").unwrap(),
        CommunityId::new("community-a").unwrap(),
        FundId::new("fund-a").unwrap(),
        Currency::new("SAR").unwrap(),
        amount,
        1,
        vec![MembershipRole::Treasurer, MembershipRole::Owner],
    )
    .unwrap()
}
#[test]
fn checked_policy_snapshot_and_original_registration_parity() {
    let registry = fixture();
    let a = policy(1, 1000);
    assert_eq!(
        a.eligible_roles(),
        &[MembershipRole::Owner, MembershipRole::Treasurer]
    );
    let b = policy(2, 2000);
    let ea = encode_governance_policy(&a).unwrap();
    let eb = encode_governance_policy(&b).unwrap();
    assert_eq!(decode_governance_policy(&ea).unwrap(), a);
    assert_eq!(decode_governance_policy(&eb).unwrap(), b);
    assert_eq!(
        encode_governance_policy(&decode_governance_policy(&ea).unwrap()).unwrap(),
        ea
    );
    let mut engine =
        replay_governance_policies([ea.as_slice(), eb.as_slice(), ea.as_slice()], &registry)
            .unwrap();
    assert!(engine.register_policy(&registry, policy(1, 1000)).is_err());
    assert!(engine.register_policy(&registry, policy(3, 2500)).is_ok());
    let proposal = SpendingProposal::new(
        SpendingProposalEventId::new("proposal-event").unwrap(),
        SpendingProposalId::new("proposal-id").unwrap(),
        SpendingApprovalPolicyId::new("policy-a").unwrap(),
        2,
        PartyId::new("requester").unwrap(),
        OrganizationId::new("org-a").unwrap(),
        CommunityId::new("community-a").unwrap(),
        FundId::new("fund-a").unwrap(),
        Currency::new("SAR").unwrap(),
        1500,
        "invoice-original",
        1000,
        2000,
    )
    .unwrap();
    assert_eq!(
        engine.submit_proposal(&registry, proposal).unwrap(),
        ProposalSubmissionOutcome::Submitted
    );
}
#[test]
fn nonmonotonic_changed_duplicate_and_missing_parent_fail_closed() {
    let registry = fixture();
    let p1 = encode_governance_policy(&policy(1, 1000)).unwrap();
    let p2 = encode_governance_policy(&policy(2, 2000)).unwrap();
    let altered = encode_governance_policy(&policy(1, 999)).unwrap();
    assert!(replay_governance_policies([p2.as_slice(), p1.as_slice()], &registry).is_err());
    assert!(replay_governance_policies([p1.as_slice(), altered.as_slice()], &registry).is_err());
    assert!(replay_governance_policies([p1.as_slice()], &CommunityRegistry::new()).is_err());
    assert!(replay_governance_policies([p1.as_slice(), p2.as_slice()], &registry).is_ok());
}
#[test]
fn changed_scope_role_amount_quorum_version_or_json_fail_closed() {
    let policy = policy(1, 1000);
    let raw = encode_governance_policy(&policy).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    for (path, v) in [
        ("/schema_version", serde_json::json!(2)),
        ("/record_type", serde_json::json!("not.policy")),
        ("/payload/id", serde_json::json!("")),
        ("/payload/version", serde_json::json!(0)),
        ("/payload/max_amount_minor", serde_json::json!("0")),
        ("/payload/max_amount_minor", serde_json::json!("-1")),
        ("/payload/max_amount_minor", serde_json::json!("1.5")),
        (
            "/payload/max_amount_minor",
            serde_json::json!("170141183460469231731687303715884105728"),
        ),
        ("/payload/required_approvals", serde_json::json!(0)),
        ("/payload/eligible_roles/0", serde_json::json!("superuser")),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(path).unwrap() = v;
        assert!(
            decode_governance_policy(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{path}"
        );
    }
    for (path, v) in [
        ("/payload/organization_id", serde_json::json!("another-org")),
        (
            "/payload/community_id",
            serde_json::json!("missing-community"),
        ),
        ("/payload/fund_id", serde_json::json!("missing-fund")),
        ("/payload/currency", serde_json::json!("EUR")),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(path).unwrap() = v;
        let bytes = serde_json::to_vec(&value).unwrap();
        let r = replay_governance_policies([bytes.as_slice()], &fixture());
        assert!(r.is_err(), "{path}");
    }
    for roles in [
        serde_json::json!(["treasurer", "owner"]),
        serde_json::json!(["owner", "owner", "treasurer"]),
    ] {
        let mut value = original.clone();
        value["payload"]["eligible_roles"] = roles;
        assert!(decode_governance_policy(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let mut extra = original;
    extra["payload"]["injected"] = serde_json::json!(true);
    assert!(decode_governance_policy(&serde_json::to_vec(&extra).unwrap()).is_err());
    assert!(decode_governance_policy(&vec![b' '; 1024 * 1024 + 1]).is_err());
    let duplicate = String::from_utf8(raw).unwrap().replacen(
        r#""schema_version":1"#,
        r#""schema_version":1,"schema_version":1"#,
        1,
    );
    assert!(decode_governance_policy(duplicate.as_bytes()).is_err());
}
