#![allow(clippy::unwrap_used)]

use cofi_community::{
    Community, CommunityError, CommunityId, CommunityRegistry, Fund, FundId, Membership,
    MembershipId, MembershipRole, MembershipStatus, Organization, OrganizationId, Party, PartyId,
    PartyKind,
};
use cofi_ledger::{Account, AccountId, AccountKind, Currency, Ledger, LedgerScopeId};

fn org(value: &str) -> OrganizationId {
    OrganizationId::new(value).unwrap()
}

fn party(value: &str) -> PartyId {
    PartyId::new(value).unwrap()
}

fn community(value: &str) -> CommunityId {
    CommunityId::new(value).unwrap()
}

fn membership(value: &str) -> MembershipId {
    MembershipId::new(value).unwrap()
}

fn fund(value: &str) -> FundId {
    FundId::new(value).unwrap()
}
fn usd() -> Currency {
    Currency::new("USD").unwrap()
}

fn base_registry() -> CommunityRegistry {
    let mut registry = CommunityRegistry::new();
    registry
        .register_organization(Organization::new(org("org-1")))
        .unwrap();
    registry
        .register_party(Party::new(
            party("party-1"),
            org("org-1"),
            PartyKind::Person,
        ))
        .unwrap();
    registry
        .register_community(Community::new(community("community-1"), org("org-1")))
        .unwrap();
    registry
}

fn ledger_with_accounts(names: &[&str]) -> Ledger {
    let mut ledger = Ledger::new();
    for name in names {
        ledger
            .register_account(Account::new(
                AccountId::new(*name).unwrap(),
                LedgerScopeId::new("org-1").unwrap(),
                AccountKind::Asset,
                usd(),
            ))
            .unwrap();
    }
    ledger
}
#[test]
fn duplicate_membership_id_is_rejected_without_new_pair() {
    let mut registry = base_registry();
    registry
        .register_party(Party::new(
            party("party-2"),
            org("org-1"),
            PartyKind::Person,
        ))
        .unwrap();
    registry
        .register_membership(Membership::new(
            membership("membership-1"),
            party("party-1"),
            community("community-1"),
            MembershipRole::Owner,
            MembershipStatus::Active,
        ))
        .unwrap();
    let duplicate = Membership::new(
        membership("membership-1"),
        party("party-2"),
        community("community-1"),
        MembershipRole::Member,
        MembershipStatus::Active,
    );
    assert_eq!(
        registry.register_membership(duplicate),
        Err(CommunityError::DuplicateMembershipId(membership(
            "membership-1"
        )))
    );
    assert!(
        registry
            .membership_for(&community("community-1"), &party("party-2"))
            .is_none()
    );
}
#[test]
fn membership_unknown_party_or_community_is_rejected_without_mutation() {
    let mut registry = base_registry();
    let unknown_party = Membership::new(
        membership("membership-party"),
        party("missing-party"),
        community("community-1"),
        MembershipRole::Member,
        MembershipStatus::Active,
    );
    assert_eq!(
        registry.register_membership(unknown_party),
        Err(CommunityError::UnknownParty(party("missing-party")))
    );

    let unknown_community = Membership::new(
        membership("membership-community"),
        party("party-1"),
        community("missing-community"),
        MembershipRole::Member,
        MembershipStatus::Active,
    );
    assert_eq!(
        registry.register_membership(unknown_community),
        Err(CommunityError::UnknownCommunity(community(
            "missing-community"
        )))
    );
    assert!(
        registry
            .membership(&membership("membership-party"))
            .is_none()
    );
    assert!(
        registry
            .membership(&membership("membership-community"))
            .is_none()
    );
}
#[test]
fn duplicate_fund_id_is_rejected_without_binding_second_account() {
    let mut registry = base_registry();
    let ledger = ledger_with_accounts(&["fund-a", "fund-b"]);
    registry
        .register_fund(
            Fund::new(
                fund("fund-1"),
                community("community-1"),
                AccountId::new("fund-a").unwrap(),
                usd(),
            ),
            &ledger,
        )
        .unwrap();
    let duplicate = Fund::new(
        fund("fund-1"),
        community("community-1"),
        AccountId::new("fund-b").unwrap(),
        usd(),
    );
    assert_eq!(
        registry.register_fund(duplicate, &ledger),
        Err(CommunityError::DuplicateFund(fund("fund-1")))
    );
    assert!(
        registry
            .fund_for_ledger_account(&AccountId::new("fund-b").unwrap())
            .is_none()
    );
}
#[test]
fn fund_unknown_community_is_rejected_without_binding_account() {
    let mut registry = base_registry();
    let ledger = ledger_with_accounts(&["fund-a"]);
    let candidate = Fund::new(
        fund("fund-1"),
        community("missing-community"),
        AccountId::new("fund-a").unwrap(),
        usd(),
    );
    assert_eq!(
        registry.register_fund(candidate, &ledger),
        Err(CommunityError::UnknownCommunity(community(
            "missing-community"
        )))
    );
    assert!(registry.fund(&fund("fund-1")).is_none());
    assert!(
        registry
            .fund_for_ledger_account(&AccountId::new("fund-a").unwrap())
            .is_none()
    );
}

#[test]
fn relationship_lookups_are_deterministic() {
    let mut registry = base_registry();
    let member = Membership::new(
        membership("membership-1"),
        party("party-1"),
        community("community-1"),
        MembershipRole::Treasurer,
        MembershipStatus::Active,
    );
    registry.register_membership(member.clone()).unwrap();
    let ledger = ledger_with_accounts(&["fund-a"]);
    let shared_fund = Fund::new(
        fund("fund-1"),
        community("community-1"),
        AccountId::new("fund-a").unwrap(),
        usd(),
    );
    registry
        .register_fund(shared_fund.clone(), &ledger)
        .unwrap();

    assert_eq!(
        registry.membership_for(&community("community-1"), &party("party-1")),
        Some(&member)
    );
    assert_eq!(
        registry.fund_for_ledger_account(&AccountId::new("fund-a").unwrap()),
        Some(&shared_fund)
    );
}
