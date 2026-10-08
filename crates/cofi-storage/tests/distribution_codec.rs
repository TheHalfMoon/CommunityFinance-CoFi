#![allow(clippy::unwrap_used)]

use cofi_community::{
    BasisPoints, Community, CommunityId, CommunityRegistry, Fund, FundAllocationAccounts,
    FundAllocationBridge, FundAllocationEvent, FundAllocationEventId, FundAllocationId, FundId,
    Organization, OrganizationId, RevenueDistributionBridge, RevenueDistributionEvent,
    RevenueDistributionEventId, RevenueDistributionId, RevenueSplitLeg, RevenueSplitRule,
    RevenueSplitRuleId,
};
use cofi_ledger::{
    Account, AccountId, AccountKind, Currency, EntryMetadata, JournalEntry, JournalEntryId, Ledger,
    LedgerScopeId, Posting, Side,
};

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}
fn scope(value: &str) -> LedgerScopeId {
    LedgerScopeId::new(value).unwrap()
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
fn rule_id(value: &str) -> RevenueSplitRuleId {
    RevenueSplitRuleId::new(value).unwrap()
}
fn distribution_id(value: &str) -> RevenueDistributionId {
    RevenueDistributionId::new(value).unwrap()
}
fn event_id(value: &str) -> RevenueDistributionEventId {
    RevenueDistributionEventId::new(value).unwrap()
}

fn register_account(
    ledger: &mut Ledger,
    id: &str,
    scope_id: &str,
    kind: AccountKind,
    currency: Currency,
) {
    ledger
        .register_account(Account::new(
            account_id(id),
            scope(scope_id),
            kind,
            currency,
        ))
        .unwrap();
}

fn seed_bank_cash(ledger: &mut Ledger, amount: i128) {
    let metadata = EntryMetadata::new(
        Some("seed-distribution-bank".to_owned()),
        Some("seed-distribution-bank".to_owned()),
    )
    .unwrap();
    let entry = JournalEntry::new(
        JournalEntryId::new("seed:distribution-bank").unwrap(),
        vec![
            Posting::new(account_id("bank-cash"), usd(), Side::Debit, amount).unwrap(),
            Posting::new(account_id("opening-equity"), usd(), Side::Credit, amount).unwrap(),
        ],
        1_700_000_001_000,
        1_700_000_001_001,
        metadata,
    )
    .unwrap();
    ledger.commit(entry).unwrap();
}

fn seed_source_fund(registry: &CommunityRegistry, ledger: &mut Ledger, amount: i128) {
    let event = FundAllocationEvent::new(
        FundAllocationEventId::new("distribution-seed-event").unwrap(),
        scope("org-1"),
        fund_id("fund-source"),
        FundAllocationId::new("distribution-seed").unwrap(),
        usd(),
        amount,
        1_700_000_001_100,
        1_700_000_001_101,
    );
    FundAllocationBridge::new()
        .apply(
            registry,
            &event,
            &FundAllocationAccounts::new(account_id("bank-cash")),
            ledger,
        )
        .unwrap();
}

fn register_fund(
    registry: &mut CommunityRegistry,
    ledger: &Ledger,
    fund: &str,
    community: &str,
    account: &str,
) {
    registry
        .register_fund(
            Fund::new(
                fund_id(fund),
                community_id(community),
                account_id(account),
                usd(),
            ),
            ledger,
        )
        .unwrap();
}

fn base_registry_and_ledger(source_amount: i128) -> (CommunityRegistry, Ledger) {
    let mut ledger = Ledger::new();
    register_account(&mut ledger, "bank-cash", "org-1", AccountKind::Asset, usd());
    register_account(
        &mut ledger,
        "opening-equity",
        "org-1",
        AccountKind::Equity,
        usd(),
    );
    for account in [
        "source-account",
        "alpha-account",
        "beta-account",
        "gamma-account",
    ] {
        register_account(&mut ledger, account, "org-1", AccountKind::Asset, usd());
    }

    let mut registry = CommunityRegistry::new();
    registry
        .register_organization(Organization::new(org_id("org-1")))
        .unwrap();
    for community in [
        "community-source",
        "community-alpha",
        "community-beta",
        "community-gamma",
    ] {
        registry
            .register_community(Community::new(community_id(community), org_id("org-1")))
            .unwrap();
    }
    register_fund(
        &mut registry,
        &ledger,
        "fund-source",
        "community-source",
        "source-account",
    );
    register_fund(
        &mut registry,
        &ledger,
        "fund-alpha",
        "community-alpha",
        "alpha-account",
    );
    register_fund(
        &mut registry,
        &ledger,
        "fund-beta",
        "community-beta",
        "beta-account",
    );
    register_fund(
        &mut registry,
        &ledger,
        "fund-gamma",
        "community-gamma",
        "gamma-account",
    );

    seed_bank_cash(&mut ledger, source_amount);
    seed_source_fund(&registry, &mut ledger, source_amount);
    (registry, ledger)
}

fn leg(fund: &str, bps: u16) -> RevenueSplitLeg {
    RevenueSplitLeg::new(fund_id(fund), BasisPoints::new(bps).unwrap())
}

fn rule(legs: Vec<RevenueSplitLeg>) -> RevenueSplitRule {
    RevenueSplitRule::new(rule_id("rule-1"), 1, legs).unwrap()
}

fn event(event: &str, distribution: &str, amount: i128) -> RevenueDistributionEvent {
    RevenueDistributionEvent::new(
        event_id(event),
        scope("org-1"),
        fund_id("fund-source"),
        distribution_id(distribution),
        rule_id("rule-1"),
        1,
        usd(),
        amount,
        1_700_000_001_200,
        1_700_000_001_201,
    )
}

use cofi_storage::distribution::{
    DistributionFact, decode_distribution, encode_distribution, verify_distribution_history,
    verify_distribution_journal,
};

fn accepted_fixture() -> (CommunityRegistry, Ledger, Ledger, DistributionFact) {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let prior = ledger.clone();
    let rule = rule(vec![leg("fund-alpha", 6_000), leg("fund-beta", 4_000)]);
    let source = event("distribute-source-1", "distribution-one", 1_000);
    RevenueDistributionBridge::new()
        .apply(&registry, &source, &rule, &mut ledger)
        .unwrap();
    (registry, ledger, prior, DistributionFact::new(source, rule))
}

#[test]
fn original_distribution_rule_and_journal_roundtrip_without_mutating_ledger() {
    let (registry, ledger, prior, fact) = accepted_fixture();
    let original_count = ledger.entry_count();
    let bytes = encode_distribution(&fact).unwrap();
    let restored = decode_distribution(&bytes).unwrap();
    assert_eq!(fact, restored);
    assert_eq!(
        verify_distribution_journal(&restored, &registry, &ledger)
            .unwrap()
            .as_str(),
        "community:revenue-distribution:distribution-one"
    );
    assert_eq!(
        verify_distribution_history([bytes.as_slice(), bytes.as_slice()], &registry, &ledger)
            .unwrap(),
        1
    );
    assert_eq!(ledger.entry_count(), original_count);
    assert!(verify_distribution_journal(&restored, &registry, &prior).is_err());
    assert_eq!(prior.entry_count() + 1, ledger.entry_count());
}
#[test]
fn altered_rule_version_or_split_or_original_source_fails_against_journal() {
    let (registry, ledger, _, fact) = accepted_fixture();
    let bytes = encode_distribution(&fact).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (pointer, new_value) in [
        ("/payload/rule/payload/version", serde_json::json!(2)),
        (
            "/payload/rule/payload/legs/0/basis_points",
            serde_json::json!(7000),
        ),
        ("/payload/event/rule_version", serde_json::json!(2)),
        (
            "/payload/event/source_event_id",
            serde_json::json!("changed"),
        ),
        (
            "/payload/event/distribution_id",
            serde_json::json!("changed"),
        ),
        ("/payload/event/amount_minor", serde_json::json!("999")),
        (
            "/payload/event/effective_at_unix_ms",
            serde_json::json!("999"),
        ),
        (
            "/payload/event/observed_at_unix_ms",
            serde_json::json!("999"),
        ),
        ("/payload/event/currency", serde_json::json!("EUR")),
        (
            "/payload/event/organization_scope",
            serde_json::json!("other-org"),
        ),
    ] {
        let mut altered = original.clone();
        *altered.pointer_mut(pointer).unwrap() = new_value;
        let blob = serde_json::to_vec(&altered).unwrap();
        if let Ok(decoded) = decode_distribution(&blob) {
            assert!(
                verify_distribution_journal(&decoded, &registry, &ledger).is_err(),
                "{pointer}"
            );
        }
    }
    let mut same = original.clone();
    same["payload"]["rule"]["payload"]["legs"][0]["basis_points"] = serde_json::json!(7000);
    same["payload"]["rule"]["payload"]["legs"][1]["basis_points"] = serde_json::json!(3000);
    let rewritten = serde_json::to_vec(&same).unwrap();
    let changed = decode_distribution(&rewritten).unwrap();
    assert!(verify_distribution_journal(&changed, &registry, &ledger).is_err());
    assert!(
        verify_distribution_history([bytes.as_slice(), rewritten.as_slice()], &registry, &ledger)
            .is_err()
    );
}
#[test]
fn malformed_or_untrusted_distribution_data_fails_closed() {
    let (registry, ledger, _, fact) = accepted_fixture();
    let bytes = encode_distribution(&fact).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (pointer, value) in [
        ("/schema_version", serde_json::json!(2)),
        ("/record_type", serde_json::json!("wrong")),
        ("/payload/event/amount_minor", serde_json::json!("-1")),
        ("/payload/event/amount_minor", serde_json::json!("2.5")),
        ("/payload/event/source_event_id", serde_json::json!("")),
        ("/payload/rule/payload/legs", serde_json::json!([])),
    ] {
        let mut modified = original.clone();
        *modified.pointer_mut(pointer).unwrap() = value;
        assert!(
            decode_distribution(&serde_json::to_vec(&modified).unwrap()).is_err(),
            "{pointer}"
        );
    }
    let mut extra = original;
    extra["payload"]["extra"] = serde_json::json!(true);
    assert!(decode_distribution(&serde_json::to_vec(&extra).unwrap()).is_err());
    assert!(decode_distribution(&vec![b' '; 1024 * 1024 + 1]).is_err());
    let duplicate = String::from_utf8(bytes.clone()).unwrap().replacen(
        r#""schema_version":1"#,
        r#""schema_version":1,"schema_version":1"#,
        1,
    );
    assert!(decode_distribution(duplicate.as_bytes()).is_err());
    assert_eq!(
        verify_distribution_history([bytes.as_slice()], &registry, &ledger).unwrap(),
        1
    );
}
