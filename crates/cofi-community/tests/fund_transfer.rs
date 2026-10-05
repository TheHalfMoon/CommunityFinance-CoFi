#![allow(clippy::unwrap_used)]

use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundAllocationAccounts, FundAllocationBridge,
    FundAllocationEvent, FundAllocationEventId, FundAllocationId, FundId, FundTransferBridge,
    FundTransferError, FundTransferEvent, FundTransferEventId, FundTransferId, FundTransferOutcome,
    Organization, OrganizationId,
};
use cofi_ledger::{
    Account, AccountId, AccountKind, Currency, EntryMetadata, JournalEntry, JournalEntryId, Ledger,
    LedgerScopeId, Posting, Side,
};

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}

fn eur() -> Currency {
    Currency::new("EUR").unwrap()
}

fn scope(value: &str) -> LedgerScopeId {
    LedgerScopeId::new(value).unwrap()
}

fn account_id(value: &str) -> AccountId {
    AccountId::new(value).unwrap()
}

fn organization_id(value: &str) -> OrganizationId {
    OrganizationId::new(value).unwrap()
}

fn community_id(value: &str) -> CommunityId {
    CommunityId::new(value).unwrap()
}

fn fund_id(value: &str) -> FundId {
    FundId::new(value).unwrap()
}

fn transfer_id(value: &str) -> FundTransferId {
    FundTransferId::new(value).unwrap()
}

fn transfer_event_id(value: &str) -> FundTransferEventId {
    FundTransferEventId::new(value).unwrap()
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
    let metadata =
        EntryMetadata::new(Some("seed-bank".to_owned()), Some("seed-bank".to_owned())).unwrap();
    let entry = JournalEntry::new(
        JournalEntryId::new("seed:bank-cash").unwrap(),
        vec![
            Posting::new(account_id("bank-cash"), usd(), Side::Debit, amount).unwrap(),
            Posting::new(account_id("opening-equity"), usd(), Side::Credit, amount).unwrap(),
        ],
        1_700_000_000_000,
        1_700_000_000_001,
        metadata,
    )
    .unwrap();
    ledger.commit(entry).unwrap();
}

fn allocate_to_source_fund(registry: &CommunityRegistry, ledger: &mut Ledger, amount: i128) {
    let event = FundAllocationEvent::new(
        FundAllocationEventId::new("allocation-event-source").unwrap(),
        scope("org-1"),
        fund_id("fund-source"),
        FundAllocationId::new("allocation-source").unwrap(),
        usd(),
        amount,
        1_700_000_000_100,
        1_700_000_000_101,
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
    register_account(
        &mut ledger,
        "fund-source-account",
        "org-1",
        AccountKind::Asset,
        usd(),
    );
    register_account(
        &mut ledger,
        "fund-destination-account",
        "org-1",
        AccountKind::Asset,
        usd(),
    );

    let mut registry = CommunityRegistry::new();
    registry
        .register_organization(Organization::new(organization_id("org-1")))
        .unwrap();
    registry
        .register_community(Community::new(
            community_id("community-source"),
            organization_id("org-1"),
        ))
        .unwrap();
    registry
        .register_community(Community::new(
            community_id("community-destination"),
            organization_id("org-1"),
        ))
        .unwrap();
    registry
        .register_fund(
            Fund::new(
                fund_id("fund-source"),
                community_id("community-source"),
                account_id("fund-source-account"),
                usd(),
            ),
            &ledger,
        )
        .unwrap();
    registry
        .register_fund(
            Fund::new(
                fund_id("fund-destination"),
                community_id("community-destination"),
                account_id("fund-destination-account"),
                usd(),
            ),
            &ledger,
        )
        .unwrap();

    seed_bank_cash(&mut ledger, source_amount);
    allocate_to_source_fund(&registry, &mut ledger, source_amount);
    (registry, ledger)
}

fn transfer_event(
    event_id: &str,
    transfer: &str,
    source: &str,
    destination: &str,
    amount: i128,
) -> FundTransferEvent {
    FundTransferEvent::new(
        transfer_event_id(event_id),
        scope("org-1"),
        fund_id(source),
        fund_id(destination),
        transfer_id(transfer),
        usd(),
        amount,
        1_700_000_000_200,
        1_700_000_000_201,
    )
}

#[test]
fn valid_transfer_moves_value_between_funds() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let event = transfer_event(
        "transfer-event-1",
        "transfer-1",
        "fund-source",
        "fund-destination",
        400,
    );
    let outcome = FundTransferBridge::new()
        .apply(&registry, &event, &mut ledger)
        .unwrap();
    assert!(matches!(outcome, FundTransferOutcome::Committed { .. }));

    let source = ledger.balance(&account_id("fund-source-account")).unwrap();
    let destination = ledger
        .balance(&account_id("fund-destination-account"))
        .unwrap();
    assert_eq!(source.debits(), 1_000);
    assert_eq!(source.credits(), 400);
    assert_eq!(destination.debits(), 400);
    assert_eq!(destination.credits(), 0);
}

#[test]
fn full_transfer_replay_succeeds_after_source_reaches_zero() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let event = transfer_event(
        "transfer-event-2",
        "transfer-2",
        "fund-source",
        "fund-destination",
        1_000,
    );
    let bridge = FundTransferBridge::new();
    let first = bridge.apply(&registry, &event, &mut ledger).unwrap();
    let count = ledger.entry_count();
    let replay = bridge.apply(&registry, &event, &mut ledger).unwrap();
    assert!(matches!(first, FundTransferOutcome::Committed { .. }));
    assert!(matches!(replay, FundTransferOutcome::Replayed { .. }));
    assert_eq!(ledger.entry_count(), count);

    let source = ledger.balance(&account_id("fund-source-account")).unwrap();
    assert_eq!(source.debits(), source.credits());
}

#[test]
fn unknown_and_same_fund_fail_closed() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let count = ledger.entry_count();
    let unknown = transfer_event(
        "transfer-event-3",
        "transfer-3",
        "fund-source",
        "missing-fund",
        100,
    );
    assert!(matches!(
        FundTransferBridge::new().apply(&registry, &unknown, &mut ledger),
        Err(FundTransferError::UnknownFund(_))
    ));
    let same = transfer_event(
        "transfer-event-4",
        "transfer-4",
        "fund-source",
        "fund-source",
        100,
    );
    assert!(matches!(
        FundTransferBridge::new().apply(&registry, &same, &mut ledger),
        Err(FundTransferError::SameFund(_))
    ));
    assert_eq!(ledger.entry_count(), count);
}

#[test]
fn scope_and_currency_mismatches_fail_closed() {
    let (mut registry, mut ledger) = base_registry_and_ledger(1_000);
    register_account(
        &mut ledger,
        "fund-org2-account",
        "org-2",
        AccountKind::Asset,
        usd(),
    );
    register_account(
        &mut ledger,
        "fund-eur-account",
        "org-1",
        AccountKind::Asset,
        eur(),
    );
    registry
        .register_organization(Organization::new(organization_id("org-2")))
        .unwrap();
    registry
        .register_community(Community::new(
            community_id("community-org2"),
            organization_id("org-2"),
        ))
        .unwrap();
    registry
        .register_community(Community::new(
            community_id("community-eur"),
            organization_id("org-1"),
        ))
        .unwrap();
    registry
        .register_fund(
            Fund::new(
                fund_id("fund-org2"),
                community_id("community-org2"),
                account_id("fund-org2-account"),
                usd(),
            ),
            &ledger,
        )
        .unwrap();
    registry
        .register_fund(
            Fund::new(
                fund_id("fund-eur"),
                community_id("community-eur"),
                account_id("fund-eur-account"),
                eur(),
            ),
            &ledger,
        )
        .unwrap();
    let cross_scope = transfer_event(
        "transfer-event-5",
        "transfer-5",
        "fund-source",
        "fund-org2",
        100,
    );
    assert!(matches!(
        FundTransferBridge::new().apply(&registry, &cross_scope, &mut ledger),
        Err(FundTransferError::OrganizationScopeMismatch { .. })
    ));
    let cross_currency = transfer_event(
        "transfer-event-6",
        "transfer-6",
        "fund-source",
        "fund-eur",
        100,
    );
    assert!(matches!(
        FundTransferBridge::new().apply(&registry, &cross_currency, &mut ledger),
        Err(FundTransferError::FundCurrencyMismatch { .. })
    ));
}

#[test]
fn insufficient_transfer_does_not_reserve_identity() {
    let (registry, mut ledger) = base_registry_and_ledger(100);
    let rejected = transfer_event(
        "transfer-event-7",
        "transfer-7",
        "fund-source",
        "fund-destination",
        101,
    );
    let count = ledger.entry_count();
    assert!(matches!(
        FundTransferBridge::new().apply(&registry, &rejected, &mut ledger),
        Err(FundTransferError::InsufficientSourceFund { .. })
    ));
    assert_eq!(ledger.entry_count(), count);
    let valid = transfer_event(
        "transfer-event-7",
        "transfer-7",
        "fund-source",
        "fund-destination",
        100,
    );
    assert!(matches!(
        FundTransferBridge::new()
            .apply(&registry, &valid, &mut ledger)
            .unwrap(),
        FundTransferOutcome::Committed { .. }
    ));
}

#[test]
fn zero_and_negative_amounts_fail_closed() {
    let (registry, mut ledger) = base_registry_and_ledger(100);
    let count = ledger.entry_count();
    for (id, amount) in [("zero", 0), ("negative", -1)] {
        let event = transfer_event(id, id, "fund-source", "fund-destination", amount);
        assert!(matches!(
            FundTransferBridge::new().apply(&registry, &event, &mut ledger),
            Err(FundTransferError::InvalidAmount(_))
        ));
    }
    assert_eq!(ledger.entry_count(), count);
}

#[test]
fn conflicting_source_event_reuse_fails_closed() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let first = transfer_event(
        "shared-event",
        "transfer-8",
        "fund-source",
        "fund-destination",
        100,
    );
    FundTransferBridge::new()
        .apply(&registry, &first, &mut ledger)
        .unwrap();
    let count = ledger.entry_count();
    let conflict = transfer_event(
        "shared-event",
        "transfer-9",
        "fund-source",
        "fund-destination",
        100,
    );
    assert!(matches!(
        FundTransferBridge::new().apply(&registry, &conflict, &mut ledger),
        Err(FundTransferError::LedgerCommit(_))
    ));
    assert_eq!(ledger.entry_count(), count);
}

#[test]
fn same_transfer_id_cannot_create_second_history() {
    let (registry, mut ledger) = base_registry_and_ledger(1_000);
    let first = transfer_event(
        "transfer-event-10",
        "same-transfer",
        "fund-source",
        "fund-destination",
        100,
    );
    FundTransferBridge::new()
        .apply(&registry, &first, &mut ledger)
        .unwrap();
    let count = ledger.entry_count();
    let duplicate = transfer_event(
        "transfer-event-11",
        "same-transfer",
        "fund-source",
        "fund-destination",
        100,
    );
    assert!(matches!(
        FundTransferBridge::new().apply(&registry, &duplicate, &mut ledger),
        Err(FundTransferError::LedgerCommit(_))
    ));
    assert_eq!(ledger.entry_count(), count);
}

#[test]
fn negative_source_balance_fails_closed() {
    let (registry, mut ledger) = base_registry_and_ledger(100);
    let full = transfer_event(
        "transfer-event-12",
        "transfer-12",
        "fund-source",
        "fund-destination",
        100,
    );
    FundTransferBridge::new()
        .apply(&registry, &full, &mut ledger)
        .unwrap();
    let metadata = EntryMetadata::new(
        Some("negative-source".into()),
        Some("negative-source".into()),
    )
    .unwrap();
    let entry = JournalEntry::new(
        JournalEntryId::new("test:negative-source").unwrap(),
        vec![
            Posting::new(account_id("opening-equity"), usd(), Side::Debit, 1).unwrap(),
            Posting::new(account_id("fund-source-account"), usd(), Side::Credit, 1).unwrap(),
        ],
        1_700_000_000_300,
        1_700_000_000_301,
        metadata,
    )
    .unwrap();
    ledger.commit(entry).unwrap();
    let count = ledger.entry_count();
    let event = transfer_event(
        "transfer-event-13",
        "transfer-13",
        "fund-source",
        "fund-destination",
        1,
    );
    assert!(matches!(
        FundTransferBridge::new().apply(&registry, &event, &mut ledger),
        Err(FundTransferError::SourceFundNegativeBalance { .. })
    ));
    assert_eq!(ledger.entry_count(), count);
}
