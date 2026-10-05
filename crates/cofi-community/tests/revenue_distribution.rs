#![allow(clippy::unwrap_used)]

use cofi_community::{
    BasisPoints, Community, CommunityId, CommunityRegistry, Fund, FundAllocationAccounts,
    FundAllocationBridge, FundAllocationEvent, FundAllocationEventId, FundAllocationId, FundId,
    Organization, OrganizationId, RevenueDistributionBridge, RevenueDistributionError,
    RevenueDistributionEvent, RevenueDistributionEventId, RevenueDistributionId,
    RevenueDistributionOutcome, RevenueSplitLeg, RevenueSplitRule, RevenueSplitRuleId,
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

fn available(ledger: &Ledger, account: &str) -> i128 {
    let balance = ledger.balance(&account_id(account)).unwrap();
    i128::try_from(balance.debits()).unwrap() - i128::try_from(balance.credits()).unwrap()
}

#[test]
fn valid_split_posts_one_atomic_distribution() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let rule = rule(vec![leg("fund-alpha", 6_000), leg("fund-beta", 4_000)]);
    let outcome = RevenueDistributionBridge::new()
        .apply(
            &registry,
            &event("event-1", "distribution-1", 1_000),
            &rule,
            &mut ledger,
        )
        .unwrap();
    assert!(matches!(
        outcome,
        RevenueDistributionOutcome::Committed { .. }
    ));
    assert_eq!(available(&ledger, "source-account"), 0);
    assert_eq!(available(&ledger, "alpha-account"), 600);
    assert_eq!(available(&ledger, "beta-account"), 400);
}

#[test]
fn largest_remainder_and_input_order_are_deterministic() {
    let (registry_a, mut ledger_a) = base_registry_and_ledger(100);
    let (registry_b, mut ledger_b) = base_registry_and_ledger(100);
    let rule_a = rule(vec![
        leg("fund-gamma", 3_334),
        leg("fund-alpha", 3_333),
        leg("fund-beta", 3_333),
    ]);
    let rule_b = rule(vec![
        leg("fund-beta", 3_333),
        leg("fund-gamma", 3_334),
        leg("fund-alpha", 3_333),
    ]);
    let ev = event("rounding-event", "rounding-distribution", 100);
    RevenueDistributionBridge::new()
        .apply(&registry_a, &ev, &rule_a, &mut ledger_a)
        .unwrap();
    RevenueDistributionBridge::new()
        .apply(&registry_b, &ev, &rule_b, &mut ledger_b)
        .unwrap();
    assert_eq!(available(&ledger_a, "alpha-account"), 33);
    assert_eq!(available(&ledger_a, "beta-account"), 33);
    assert_eq!(available(&ledger_a, "gamma-account"), 34);
    assert_eq!(
        available(&ledger_a, "alpha-account"),
        available(&ledger_b, "alpha-account")
    );
    assert_eq!(
        available(&ledger_a, "beta-account"),
        available(&ledger_b, "beta-account")
    );
    assert_eq!(
        available(&ledger_a, "gamma-account"),
        available(&ledger_b, "gamma-account")
    );
}

#[test]
fn exact_replay_succeeds_after_full_source_depletion() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let rule = rule(vec![leg("fund-alpha", 5_000), leg("fund-beta", 5_000)]);
    let ev = event("replay-event", "replay-distribution", 1_000);
    let bridge = RevenueDistributionBridge::new();
    assert!(matches!(
        bridge.apply(&registry, &ev, &rule, &mut ledger).unwrap(),
        RevenueDistributionOutcome::Committed { .. }
    ));
    assert_eq!(available(&ledger, "source-account"), 0);
    assert!(matches!(
        bridge.apply(&registry, &ev, &rule, &mut ledger).unwrap(),
        RevenueDistributionOutcome::Replayed { .. }
    ));
    assert_eq!(ledger.entry_count(), 3);
}

#[test]
fn source_cannot_be_destination() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let rule = rule(vec![leg("fund-source", 5_000), leg("fund-alpha", 5_000)]);
    let before = ledger.entry_count();
    assert_eq!(
        RevenueDistributionBridge::new().apply(
            &registry,
            &event("source-dest", "source-dest", 100),
            &rule,
            &mut ledger,
        ),
        Err(RevenueDistributionError::SourceIsDestination(fund_id(
            "fund-source"
        )))
    );
    assert_eq!(ledger.entry_count(), before);
}

#[test]
fn insufficient_balance_fails_without_reserving_distribution_identity() {
    let (registry, mut ledger) = base_registry_and_ledger(100);
    let rule = rule(vec![leg("fund-alpha", 5_000), leg("fund-beta", 5_000)]);
    let ev = event("insufficient-event", "retryable-distribution", 200);
    let before = ledger.entry_count();
    assert!(matches!(
        RevenueDistributionBridge::new().apply(&registry, &ev, &rule, &mut ledger),
        Err(RevenueDistributionError::InsufficientSourceFund { .. })
    ));
    assert_eq!(ledger.entry_count(), before);

    seed_bank_cash_extra(&mut ledger, 100);
    allocate_extra(&registry, &mut ledger, 100);
    assert!(matches!(
        RevenueDistributionBridge::new()
            .apply(&registry, &ev, &rule, &mut ledger)
            .unwrap(),
        RevenueDistributionOutcome::Committed { .. }
    ));
}

fn seed_bank_cash_extra(ledger: &mut Ledger, amount: i128) {
    let metadata =
        EntryMetadata::new(Some("seed-extra".into()), Some("seed-extra".into())).unwrap();
    let entry = JournalEntry::new(
        JournalEntryId::new("seed:extra").unwrap(),
        vec![
            Posting::new(account_id("bank-cash"), usd(), Side::Debit, amount).unwrap(),
            Posting::new(account_id("opening-equity"), usd(), Side::Credit, amount).unwrap(),
        ],
        1_700_000_001_300,
        1_700_000_001_301,
        metadata,
    )
    .unwrap();
    ledger.commit(entry).unwrap();
}

fn allocate_extra(registry: &CommunityRegistry, ledger: &mut Ledger, amount: i128) {
    let event = FundAllocationEvent::new(
        FundAllocationEventId::new("distribution-extra-event").unwrap(),
        scope("org-1"),
        fund_id("fund-source"),
        FundAllocationId::new("distribution-extra").unwrap(),
        usd(),
        amount,
        1_700_000_001_310,
        1_700_000_001_311,
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

#[test]
fn conflicting_source_event_reuse_fails_closed() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let rule = rule(vec![leg("fund-alpha", 5_000), leg("fund-beta", 5_000)]);
    let first = event("same-event", "distribution-a", 200);
    RevenueDistributionBridge::new()
        .apply(&registry, &first, &rule, &mut ledger)
        .unwrap();
    let before = ledger.entry_count();
    let conflicting = event("same-event", "distribution-b", 200);
    assert!(
        RevenueDistributionBridge::new()
            .apply(&registry, &conflicting, &rule, &mut ledger)
            .is_err()
    );
    assert_eq!(ledger.entry_count(), before);
}

#[test]
fn same_distribution_id_cannot_create_second_history() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let rule = rule(vec![leg("fund-alpha", 5_000), leg("fund-beta", 5_000)]);
    let first = event("event-a", "same-distribution", 200);
    RevenueDistributionBridge::new()
        .apply(&registry, &first, &rule, &mut ledger)
        .unwrap();
    let before = ledger.entry_count();
    let second = event("event-b", "same-distribution", 200);
    assert!(
        RevenueDistributionBridge::new()
            .apply(&registry, &second, &rule, &mut ledger)
            .is_err()
    );
    assert_eq!(ledger.entry_count(), before);
}

#[test]
fn event_rule_identity_must_match_rule() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let rule = rule(vec![leg("fund-alpha", 5_000), leg("fund-beta", 5_000)]);
    let mismatched = RevenueDistributionEvent::new(
        event_id("rule-mismatch"),
        scope("org-1"),
        fund_id("fund-source"),
        distribution_id("rule-mismatch"),
        rule_id("other-rule"),
        1,
        usd(),
        100,
        1_700_000_001_200,
        1_700_000_001_201,
    );
    assert_eq!(
        RevenueDistributionBridge::new().apply(&registry, &mismatched, &rule, &mut ledger),
        Err(RevenueDistributionError::RuleIdentityMismatch)
    );
}

#[test]
fn unknown_fund_and_scope_currency_mismatches_fail_closed() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let unknown_rule = rule(vec![leg("fund-alpha", 5_000), leg("missing-fund", 5_000)]);
    assert!(matches!(
        RevenueDistributionBridge::new().apply(
            &registry,
            &event("unknown", "unknown", 100),
            &unknown_rule,
            &mut ledger,
        ),
        Err(RevenueDistributionError::UnknownFund(_))
    ));

    let good_rule = rule(vec![leg("fund-alpha", 5_000), leg("fund-beta", 5_000)]);
    let bad_scope = RevenueDistributionEvent::new(
        event_id("bad-scope"),
        scope("org-2"),
        fund_id("fund-source"),
        distribution_id("bad-scope"),
        rule_id("rule-1"),
        1,
        usd(),
        100,
        1_700_000_001_200,
        1_700_000_001_201,
    );
    assert!(matches!(
        RevenueDistributionBridge::new().apply(&registry, &bad_scope, &good_rule, &mut ledger),
        Err(RevenueDistributionError::OrganizationScopeMismatch { .. })
    ));

    let eur = Currency::new("EUR").unwrap();
    let bad_currency = RevenueDistributionEvent::new(
        event_id("bad-currency"),
        scope("org-1"),
        fund_id("fund-source"),
        distribution_id("bad-currency"),
        rule_id("rule-1"),
        1,
        eur,
        100,
        1_700_000_001_200,
        1_700_000_001_201,
    );
    assert!(matches!(
        RevenueDistributionBridge::new().apply(&registry, &bad_currency, &good_rule, &mut ledger),
        Err(RevenueDistributionError::FundCurrencyMismatch { .. })
    ));
}

#[test]
fn zero_and_negative_amounts_fail_closed() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let rule = rule(vec![leg("fund-alpha", 5_000), leg("fund-beta", 5_000)]);
    let before = ledger.entry_count();
    for amount in [0, -1] {
        assert_eq!(
            RevenueDistributionBridge::new().apply(
                &registry,
                &event("invalid-amount", "invalid-amount", amount),
                &rule,
                &mut ledger,
            ),
            Err(RevenueDistributionError::InvalidAmount(amount))
        );
    }
    assert_eq!(ledger.entry_count(), before);
}

#[test]
fn negative_source_balance_fails_closed() {
    let (registry, mut ledger) = base_registry_and_ledger(100);
    let metadata = EntryMetadata::new(
        Some("drive-source-negative".into()),
        Some("drive-source-negative".into()),
    )
    .unwrap();
    let entry = JournalEntry::new(
        JournalEntryId::new("test:drive-source-negative").unwrap(),
        vec![
            Posting::new(account_id("alpha-account"), usd(), Side::Debit, 200).unwrap(),
            Posting::new(account_id("source-account"), usd(), Side::Credit, 200).unwrap(),
        ],
        1_700_000_001_400,
        1_700_000_001_401,
        metadata,
    )
    .unwrap();
    ledger.commit(entry).unwrap();
    let rule = rule(vec![leg("fund-alpha", 5_000), leg("fund-beta", 5_000)]);
    let before = ledger.entry_count();
    assert!(matches!(
        RevenueDistributionBridge::new().apply(
            &registry,
            &event("negative-source", "negative-source", 10),
            &rule,
            &mut ledger,
        ),
        Err(RevenueDistributionError::SourceFundNegativeBalance { .. })
    ));
    assert_eq!(ledger.entry_count(), before);
}
