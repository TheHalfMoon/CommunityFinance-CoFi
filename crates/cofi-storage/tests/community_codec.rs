#![allow(clippy::unwrap_used)]

use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundId, Membership, MembershipId,
    MembershipRole, MembershipStatus, Organization, OrganizationId, Party, PartyId, PartyKind,
};
use cofi_ledger::{Account, AccountId, AccountKind, Currency, Ledger, LedgerScopeId};
use cofi_storage::community::{
    CommunityFact, decode_community_fact, encode_community_fact, replay_community_facts,
};

fn id_org(s: &str) -> OrganizationId {
    OrganizationId::new(s).unwrap()
}
fn id_party(s: &str) -> PartyId {
    PartyId::new(s).unwrap()
}
fn id_community(s: &str) -> CommunityId {
    CommunityId::new(s).unwrap()
}
fn id_membership(s: &str) -> MembershipId {
    MembershipId::new(s).unwrap()
}
fn id_fund(s: &str) -> FundId {
    FundId::new(s).unwrap()
}
fn currency(s: &str) -> Currency {
    Currency::new(s).unwrap()
}

fn ledger() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .register_account(Account::new(
            AccountId::new("asset-1").unwrap(),
            LedgerScopeId::new("org-a").unwrap(),
            AccountKind::Asset,
            currency("SAR"),
        ))
        .unwrap();
    ledger
}
fn facts() -> Vec<CommunityFact> {
    vec![
        CommunityFact::Organization(Organization::new(id_org("org-a"))),
        CommunityFact::Party(Party::new(
            id_party("party-a"),
            id_org("org-a"),
            PartyKind::Person,
        )),
        CommunityFact::Community(Community::new(id_community("community-a"), id_org("org-a"))),
        CommunityFact::Membership(Membership::new(
            id_membership("membership-a"),
            id_party("party-a"),
            id_community("community-a"),
            MembershipRole::Treasurer,
            MembershipStatus::Active,
        )),
        CommunityFact::Fund(Fund::new(
            id_fund("fund-a"),
            id_community("community-a"),
            AccountId::new("asset-1").unwrap(),
            currency("SAR"),
        )),
    ]
}

#[test]
fn checked_roundtrip_and_replay_rebuilds_all_five_registries() {
    let ledger = ledger();
    let facts = facts();
    let encoded = facts
        .iter()
        .map(encode_community_fact)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for (original, blob) in facts.iter().zip(&encoded) {
        assert_eq!(&decode_community_fact(blob).unwrap(), original);
    }

    let mut expected = CommunityRegistry::new();
    for item in &facts {
        match item {
            CommunityFact::Organization(v) => expected.register_organization(v.clone()).unwrap(),
            CommunityFact::Party(v) => expected.register_party(v.clone()).unwrap(),
            CommunityFact::Community(v) => expected.register_community(v.clone()).unwrap(),
            CommunityFact::Membership(v) => expected.register_membership(v.clone()).unwrap(),
            CommunityFact::Fund(v) => expected.register_fund(v.clone(), &ledger).unwrap(),
        }
    }
    let restored = replay_community_facts(encoded.iter().map(Vec::as_slice), &ledger).unwrap();
    assert_eq!(
        restored.organization(&id_org("org-a")),
        expected.organization(&id_org("org-a"))
    );
    assert_eq!(
        restored.party(&id_party("party-a")),
        expected.party(&id_party("party-a"))
    );
    assert_eq!(
        restored.community(&id_community("community-a")),
        expected.community(&id_community("community-a"))
    );
    assert_eq!(
        restored.membership(&id_membership("membership-a")),
        expected.membership(&id_membership("membership-a"))
    );
    assert_eq!(
        restored.membership_for(&id_community("community-a"), &id_party("party-a")),
        expected.membership_for(&id_community("community-a"), &id_party("party-a"))
    );
    assert_eq!(
        restored.fund(&id_fund("fund-a")),
        expected.fund(&id_fund("fund-a"))
    );
    assert_eq!(
        restored.fund_for_ledger_account(&AccountId::new("asset-1").unwrap()),
        expected.fund_for_ledger_account(&AccountId::new("asset-1").unwrap())
    );
    assert!(
        replay_community_facts(
            encoded.iter().chain(encoded.iter()).map(Vec::as_slice),
            &ledger
        )
        .is_ok()
    );
}

#[test]
fn missing_parent_and_fund_ledger_mismatch_fail_closed() {
    let all = facts()
        .iter()
        .map(encode_community_fact)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let ledger = ledger();
    assert!(replay_community_facts([all[1].as_slice()], &ledger).is_err());
    assert!(replay_community_facts([all[2].as_slice()], &ledger).is_err());
    assert!(replay_community_facts([all[4].as_slice()], &ledger).is_err());
    let mut bad = facts();
    bad[4] = CommunityFact::Fund(Fund::new(
        id_fund("fund-a"),
        id_community("community-a"),
        AccountId::new("asset-1").unwrap(),
        currency("USD"),
    ));
    let mut blobs = all[..4].to_vec();
    blobs.push(encode_community_fact(&bad[4]).unwrap());
    assert!(replay_community_facts(blobs.iter().map(Vec::as_slice), &ledger).is_err());
}

#[test]
fn identity_reuse_and_cross_organization_conflicts_fail_closed() {
    let mut base = facts();
    let original = base
        .iter()
        .map(encode_community_fact)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut conflicting = base[..4].to_vec();
    conflicting.push(CommunityFact::Membership(Membership::new(
        id_membership("membership-a"),
        id_party("party-a"),
        id_community("community-a"),
        MembershipRole::Owner,
        MembershipStatus::Suspended,
    )));
    let conflict = encode_community_fact(conflicting.last().unwrap()).unwrap();
    let mut bytes = original[..4].to_vec();
    bytes.push(conflict);
    assert!(replay_community_facts(bytes.iter().map(Vec::as_slice), &ledger()).is_err());

    base.insert(
        1,
        CommunityFact::Organization(Organization::new(id_org("org-b"))),
    );
    base.insert(
        2,
        CommunityFact::Party(Party::new(
            id_party("party-b"),
            id_org("org-b"),
            PartyKind::Organization,
        )),
    );
    base.push(CommunityFact::Membership(Membership::new(
        id_membership("member-b"),
        id_party("party-b"),
        id_community("community-a"),
        MembershipRole::Member,
        MembershipStatus::Active,
    )));
    let bytes = base
        .iter()
        .map(encode_community_fact)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(replay_community_facts(bytes.iter().map(Vec::as_slice), &ledger()).is_err());
}

#[test]
fn invalid_type_version_enum_extra_fields_and_oversize_fail_closed() {
    let blob = encode_community_fact(&facts()[3]).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&blob).unwrap();
    for (pointer, v) in [
        ("/schema_version", serde_json::json!(2)),
        ("/record_type", serde_json::json!("untrusted.kind")),
        ("/payload/role", serde_json::json!("root")),
        ("/payload/status", serde_json::json!("owner")),
        ("/payload/party_id", serde_json::json!(" ")),
    ] {
        let mut modified = original.clone();
        *modified.pointer_mut(pointer).unwrap() = v;
        assert!(
            decode_community_fact(&serde_json::to_vec(&modified).unwrap()).is_err(),
            "{pointer}"
        );
    }
    let mut extra = original;
    extra["payload"]["forged"] = serde_json::json!("value");
    assert!(decode_community_fact(&serde_json::to_vec(&extra).unwrap()).is_err());
    let overlong = vec![b' '; 1024 * 1024 + 1];
    assert!(decode_community_fact(&overlong).is_err());
    // A duplicated top-level key must not be silently last-write-wins.
    let raw = String::from_utf8(blob).unwrap().replacen(
        r#""schema_version":1"#,
        r#""schema_version":1,"schema_version":1"#,
        1,
    );
    assert!(decode_community_fact(raw.as_bytes()).is_err());
}

#[test]
fn duplicate_membership_pair_and_fund_ledger_account_bindings_are_rejected() {
    let mut all = facts();
    all.push(CommunityFact::Membership(Membership::new(
        id_membership("membership-2"),
        id_party("party-a"),
        id_community("community-a"),
        MembershipRole::Member,
        MembershipStatus::Active,
    )));
    let blobs = all
        .iter()
        .map(encode_community_fact)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(replay_community_facts(blobs.iter().map(Vec::as_slice), &ledger()).is_err());

    let mut all = facts();
    all.push(CommunityFact::Fund(Fund::new(
        id_fund("fund-2"),
        id_community("community-a"),
        AccountId::new("asset-1").unwrap(),
        currency("SAR"),
    )));
    let blobs = all
        .iter()
        .map(encode_community_fact)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(replay_community_facts(blobs.iter().map(Vec::as_slice), &ledger()).is_err());
}

#[test]
fn ledger_scope_and_account_kind_are_checked_on_fund_recovery() {
    let all = facts()
        .iter()
        .map(encode_community_fact)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for (scope, kind) in [
        ("another-tenant", AccountKind::Asset),
        ("org-a", AccountKind::Liability),
    ] {
        let mut wrong_ledger = Ledger::new();
        wrong_ledger
            .register_account(Account::new(
                AccountId::new("asset-1").unwrap(),
                LedgerScopeId::new(scope).unwrap(),
                kind,
                currency("SAR"),
            ))
            .unwrap();
        assert!(replay_community_facts(all.iter().map(Vec::as_slice), &wrong_ledger).is_err());
    }
}
