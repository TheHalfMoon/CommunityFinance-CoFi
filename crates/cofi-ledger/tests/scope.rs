#![allow(clippy::unwrap_used)]

use cofi_ledger::{
    Account, AccountId, AccountKind, Currency, EntryMetadata, JournalEntry, JournalEntryId, Ledger,
    LedgerScopeId, LedgerStateError, Posting, Side,
};

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}

fn account(id: &str, scope: &str) -> Account {
    Account::new(
        AccountId::new(id).unwrap(),
        LedgerScopeId::new(scope).unwrap(),
        AccountKind::Asset,
        usd(),
    )
}

#[test]
fn cross_scope_entry_is_rejected_without_partial_effect() {
    let mut ledger = Ledger::new();
    ledger.register_account(account("cash-a", "org-a")).unwrap();
    ledger.register_account(account("cash-b", "org-b")).unwrap();
    let entry = JournalEntry::new(
        JournalEntryId::new("entry-1").unwrap(),
        vec![
            Posting::new(AccountId::new("cash-a").unwrap(), usd(), Side::Debit, 100).unwrap(),
            Posting::new(AccountId::new("cash-b").unwrap(), usd(), Side::Credit, 100).unwrap(),
        ],
        1_700_000_000_000,
        1_700_000_000_100,
        EntryMetadata::new(Some("corr-1".into()), Some("idem-1".into())).unwrap(),
    )
    .unwrap();

    assert_eq!(
        ledger.commit(entry),
        Err(LedgerStateError::CrossScopeEntry {
            expected: LedgerScopeId::new("org-a").unwrap(),
            actual: LedgerScopeId::new("org-b").unwrap(),
        })
    );
    assert_eq!(ledger.entry_count(), 0);
    assert_eq!(
        ledger
            .balance(&AccountId::new("cash-a").unwrap())
            .unwrap()
            .debits(),
        0
    );
    assert_eq!(
        ledger
            .balance(&AccountId::new("cash-b").unwrap())
            .unwrap()
            .credits(),
        0
    );
}
