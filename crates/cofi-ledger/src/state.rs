use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::{AccountId, Currency, JournalEntry, JournalEntryId, LedgerScopeId, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccountKind {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
}

impl AccountKind {
    #[must_use]
    pub const fn normal_side(self) -> Side {
        match self {
            Self::Asset | Self::Expense => Side::Debit,
            Self::Liability | Self::Equity | Self::Revenue => Side::Credit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    id: AccountId,
    scope_id: LedgerScopeId,
    kind: AccountKind,
    currency: Currency,
}
impl Account {
    #[must_use]
    pub const fn new(
        id: AccountId,
        scope_id: LedgerScopeId,
        kind: AccountKind,
        currency: Currency,
    ) -> Self {
        Self {
            id,
            scope_id,
            kind,
            currency,
        }
    }

    #[must_use]
    pub fn id(&self) -> &AccountId {
        &self.id
    }

    #[must_use]
    pub const fn scope_id(&self) -> &LedgerScopeId {
        &self.scope_id
    }

    #[must_use]
    pub const fn kind(&self) -> AccountKind {
        self.kind
    }

    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AccountBalance {
    debits: u128,
    credits: u128,
}

impl AccountBalance {
    #[must_use]
    pub const fn debits(self) -> u128 {
        self.debits
    }

    #[must_use]
    pub const fn credits(self) -> u128 {
        self.credits
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitOutcome {
    Committed,
    Replayed,
}

#[derive(Debug, Clone, Default)]
pub struct Ledger {
    accounts: BTreeMap<AccountId, Account>,
    entries: BTreeMap<JournalEntryId, JournalEntry>,
    idempotency: BTreeMap<String, JournalEntryId>,
    business_keys: BTreeMap<String, JournalEntryId>,
    balances: BTreeMap<AccountId, AccountBalance>,
}

impl Ledger {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_account(&mut self, account: Account) -> Result<(), LedgerStateError> {
        if self.accounts.contains_key(account.id()) {
            return Err(LedgerStateError::DuplicateAccount(account.id().clone()));
        }

        self.balances
            .insert(account.id().clone(), AccountBalance::default());
        self.accounts.insert(account.id().clone(), account);
        Ok(())
    }

    #[must_use]
    pub fn account(&self, id: &AccountId) -> Option<&Account> {
        self.accounts.get(id)
    }

    /// Read-only, deterministic ledger account inventory in identifier order.
    /// Used to qualify the *entire* supplied historical account snapshot,
    /// including accounts not touched by any journal in the current flow.
    pub fn accounts(&self) -> impl ExactSizeIterator<Item = &Account> {
        self.accounts.values()
    }
    #[must_use]
    pub fn entry(&self, id: &JournalEntryId) -> Option<&JournalEntry> {
        self.entries.get(id)
    }

    #[must_use]
    pub fn balance(&self, id: &AccountId) -> Option<AccountBalance> {
        self.balances.get(id).copied()
    }

    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn commit(&mut self, entry: JournalEntry) -> Result<CommitOutcome, LedgerStateError> {
        let idempotency_key = entry
            .metadata()
            .idempotency_key()
            .ok_or(LedgerStateError::MissingIdempotencyKey)?;

        if let Some(existing_id) = self.idempotency.get(idempotency_key) {
            let existing =
                self.entries
                    .get(existing_id)
                    .ok_or(LedgerStateError::InternalInvariant(
                        "idempotency index points to missing entry",
                    ))?;
            return if existing == &entry {
                Ok(CommitOutcome::Replayed)
            } else {
                Err(LedgerStateError::IdempotencyConflict(
                    idempotency_key.to_owned(),
                ))
            };
        }

        let business_key = entry.metadata().business_key().map(str::to_owned);
        if let Some(key) = business_key.as_deref() {
            if self.business_keys.contains_key(key) {
                return Err(LedgerStateError::BusinessKeyConflict(key.to_owned()));
            }
        }

        if self.entries.contains_key(entry.id()) {
            return Err(LedgerStateError::DuplicateEntryId(entry.id().clone()));
        }

        let mut deltas: BTreeMap<AccountId, AccountBalance> = BTreeMap::new();
        let mut entry_scope: Option<LedgerScopeId> = None;
        for posting in entry.postings() {
            let account = self
                .accounts
                .get(posting.account_id())
                .ok_or_else(|| LedgerStateError::UnknownAccount(posting.account_id().clone()))?;

            if let Some(expected_scope) = &entry_scope {
                if expected_scope != account.scope_id() {
                    return Err(LedgerStateError::CrossScopeEntry {
                        expected: expected_scope.clone(),
                        actual: account.scope_id().clone(),
                    });
                }
            } else {
                entry_scope = Some(account.scope_id().clone());
            }

            if account.currency() != posting.currency() {
                return Err(LedgerStateError::AccountCurrencyMismatch {
                    account_id: posting.account_id().clone(),
                    expected: account.currency(),
                    actual: posting.currency(),
                });
            }

            let delta = deltas.entry(posting.account_id().clone()).or_default();
            match posting.side() {
                Side::Debit => {
                    delta.debits = delta
                        .debits
                        .checked_add(posting.amount().value())
                        .ok_or_else(|| {
                            LedgerStateError::BalanceOverflow(posting.account_id().clone())
                        })?;
                }
                Side::Credit => {
                    delta.credits = delta
                        .credits
                        .checked_add(posting.amount().value())
                        .ok_or_else(|| {
                            LedgerStateError::BalanceOverflow(posting.account_id().clone())
                        })?;
                }
            }
        }

        let mut next_balances = Vec::with_capacity(deltas.len());
        for (account_id, delta) in deltas {
            let current = self.balances.get(&account_id).copied().ok_or(
                LedgerStateError::InternalInvariant("registered account is missing its balance"),
            )?;
            let debits = current
                .debits
                .checked_add(delta.debits)
                .ok_or_else(|| LedgerStateError::BalanceOverflow(account_id.clone()))?;
            let credits = current
                .credits
                .checked_add(delta.credits)
                .ok_or_else(|| LedgerStateError::BalanceOverflow(account_id.clone()))?;
            next_balances.push((account_id, AccountBalance { debits, credits }));
        }

        for (account_id, balance) in next_balances {
            self.balances.insert(account_id, balance);
        }

        let entry_id = entry.id().clone();
        self.idempotency
            .insert(idempotency_key.to_owned(), entry_id.clone());
        if let Some(key) = business_key {
            self.business_keys.insert(key, entry_id.clone());
        }
        self.entries.insert(entry_id, entry);
        Ok(CommitOutcome::Committed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerStateError {
    DuplicateAccount(AccountId),
    MissingIdempotencyKey,
    IdempotencyConflict(String),
    BusinessKeyConflict(String),
    DuplicateEntryId(JournalEntryId),
    UnknownAccount(AccountId),
    AccountCurrencyMismatch {
        account_id: AccountId,
        expected: Currency,
        actual: Currency,
    },
    CrossScopeEntry {
        expected: LedgerScopeId,
        actual: LedgerScopeId,
    },
    BalanceOverflow(AccountId),
    InternalInvariant(&'static str),
}

impl Display for LedgerStateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateAccount(id) => write!(f, "account already registered: {}", id.as_str()),
            Self::MissingIdempotencyKey => f.write_str("ledger commit requires an idempotency key"),
            Self::IdempotencyConflict(key) => {
                write!(
                    f,
                    "idempotency key was reused with different content: {key}"
                )
            }
            Self::BusinessKeyConflict(key) => {
                write!(
                    f,
                    "business key already has committed financial history: {key}"
                )
            }
            Self::DuplicateEntryId(id) => {
                write!(f, "journal entry ID already exists: {}", id.as_str())
            }
            Self::UnknownAccount(id) => write!(f, "unknown account: {}", id.as_str()),
            Self::AccountCurrencyMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "account {} requires currency {expected}, got {actual}",
                account_id.as_str()
            ),
            Self::CrossScopeEntry { expected, actual } => write!(
                f,
                "journal entry crosses ledger scopes: expected {}, got {}",
                expected.as_str(),
                actual.as_str()
            ),
            Self::BalanceOverflow(id) => {
                write!(f, "account balance overflowed: {}", id.as_str())
            }
            Self::InternalInvariant(message) => {
                write!(f, "ledger internal invariant violated: {message}")
            }
        }
    }
}

impl Error for LedgerStateError {}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{EntryMetadata, Posting};

    fn id(value: &str) -> AccountId {
        AccountId::new(value).unwrap()
    }

    fn scope(value: &str) -> LedgerScopeId {
        LedgerScopeId::new(value).unwrap()
    }

    fn usd() -> Currency {
        Currency::new("USD").unwrap()
    }

    fn eur() -> Currency {
        Currency::new("EUR").unwrap()
    }

    fn account(value: &str, kind: AccountKind, currency: Currency) -> Account {
        Account::new(id(value), scope("org-1"), kind, currency)
    }

    fn entry(
        entry_id: &str,
        idempotency_key: Option<&str>,
        debit_account: &str,
        credit_account: &str,
        currency: Currency,
        amount: i128,
    ) -> JournalEntry {
        JournalEntry::new(
            JournalEntryId::new(entry_id).unwrap(),
            vec![
                Posting::new(id(debit_account), currency, Side::Debit, amount).unwrap(),
                Posting::new(id(credit_account), currency, Side::Credit, amount).unwrap(),
            ],
            1_700_000_000_000,
            1_700_000_000_100,
            EntryMetadata::new(
                Some(format!("corr-{entry_id}")),
                idempotency_key.map(str::to_owned),
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn entry_with_business_key(
        entry_id: &str,
        idempotency_key: &str,
        business_key: &str,
        amount: i128,
    ) -> JournalEntry {
        let metadata = EntryMetadata::new(
            Some(format!("corr-{entry_id}")),
            Some(idempotency_key.to_owned()),
        )
        .unwrap()
        .with_business_key(Some(business_key.to_owned()))
        .unwrap();
        JournalEntry::new(
            JournalEntryId::new(entry_id).unwrap(),
            vec![
                Posting::new(id("cash"), usd(), Side::Debit, amount).unwrap(),
                Posting::new(id("revenue"), usd(), Side::Credit, amount).unwrap(),
            ],
            1_700_000_000_000,
            1_700_000_000_100,
            metadata,
        )
        .unwrap()
    }

    fn funded_ledger() -> Ledger {
        let mut ledger = Ledger::new();
        ledger
            .register_account(account("cash", AccountKind::Asset, usd()))
            .unwrap();
        ledger
            .register_account(account("revenue", AccountKind::Revenue, usd()))
            .unwrap();
        ledger
    }

    #[test]
    fn account_registration_and_normal_side_are_deterministic() {
        let mut ledger = Ledger::new();
        let asset = account("cash", AccountKind::Asset, usd());
        assert_eq!(asset.scope_id(), &scope("org-1"));
        assert_eq!(asset.kind().normal_side(), Side::Debit);
        assert_eq!(AccountKind::Revenue.normal_side(), Side::Credit);
        ledger.register_account(asset.clone()).unwrap();
        assert_eq!(ledger.account(asset.id()), Some(&asset));
        assert_eq!(ledger.balance(asset.id()), Some(AccountBalance::default()));
    }

    #[test]
    fn duplicate_account_is_rejected() {
        let mut ledger = Ledger::new();
        let cash = account("cash", AccountKind::Asset, usd());
        ledger.register_account(cash.clone()).unwrap();
        assert_eq!(
            ledger.register_account(cash),
            Err(LedgerStateError::DuplicateAccount(id("cash")))
        );
    }

    #[test]
    fn unknown_account_is_rejected_without_financial_effect() {
        let mut ledger = Ledger::new();
        ledger
            .register_account(account("cash", AccountKind::Asset, usd()))
            .unwrap();
        let candidate = entry("entry-1", Some("idem-1"), "cash", "revenue", usd(), 100);
        assert_eq!(
            ledger.commit(candidate),
            Err(LedgerStateError::UnknownAccount(id("revenue")))
        );
        assert_eq!(ledger.entry_count(), 0);
        assert_eq!(ledger.balance(&id("cash")), Some(AccountBalance::default()));
    }

    #[test]
    fn account_currency_mismatch_is_rejected() {
        let mut ledger = Ledger::new();
        ledger
            .register_account(account("cash", AccountKind::Asset, usd()))
            .unwrap();
        ledger
            .register_account(account("revenue", AccountKind::Revenue, eur()))
            .unwrap();
        let candidate = entry("entry-1", Some("idem-1"), "cash", "revenue", usd(), 100);
        assert_eq!(
            ledger.commit(candidate),
            Err(LedgerStateError::AccountCurrencyMismatch {
                account_id: id("revenue"),
                expected: eur(),
                actual: usd(),
            })
        );
        assert_eq!(ledger.entry_count(), 0);
    }

    #[test]
    fn commit_updates_balances_once() {
        let mut ledger = funded_ledger();
        let candidate = entry("entry-1", Some("idem-1"), "cash", "revenue", usd(), 125);
        assert_eq!(ledger.commit(candidate), Ok(CommitOutcome::Committed));
        assert_eq!(
            ledger.balance(&id("cash")),
            Some(AccountBalance {
                debits: 125,
                credits: 0
            })
        );
        assert_eq!(
            ledger.balance(&id("revenue")),
            Some(AccountBalance {
                debits: 0,
                credits: 125
            })
        );
    }

    #[test]
    fn identical_idempotent_replay_has_zero_duplicate_effect() {
        let mut ledger = funded_ledger();
        let candidate = entry("entry-1", Some("idem-1"), "cash", "revenue", usd(), 250);
        assert_eq!(
            ledger.commit(candidate.clone()),
            Ok(CommitOutcome::Committed)
        );
        let before_cash = ledger.balance(&id("cash"));
        let before_revenue = ledger.balance(&id("revenue"));
        assert_eq!(ledger.commit(candidate), Ok(CommitOutcome::Replayed));
        assert_eq!(ledger.entry_count(), 1);
        assert_eq!(ledger.balance(&id("cash")), before_cash);
        assert_eq!(ledger.balance(&id("revenue")), before_revenue);
    }

    #[test]
    fn conflicting_idempotency_reuse_is_rejected() {
        let mut ledger = funded_ledger();
        let first = entry("entry-1", Some("idem-1"), "cash", "revenue", usd(), 100);
        let second = entry("entry-2", Some("idem-1"), "cash", "revenue", usd(), 200);
        assert_eq!(ledger.commit(first), Ok(CommitOutcome::Committed));
        assert_eq!(
            ledger.commit(second),
            Err(LedgerStateError::IdempotencyConflict("idem-1".to_owned()))
        );
        assert_eq!(ledger.entry_count(), 1);
        assert_eq!(ledger.balance(&id("cash")).unwrap().debits(), 100);
    }

    #[test]
    fn business_key_cannot_create_second_financial_history() {
        let mut ledger = funded_ledger();
        let first = entry_with_business_key("entry-1", "idem-1", "invoice-1", 100);
        let second = entry_with_business_key("entry-2", "idem-2", "invoice-1", 100);
        assert_eq!(ledger.commit(first), Ok(CommitOutcome::Committed));
        let before_cash = ledger.balance(&id("cash"));
        let before_revenue = ledger.balance(&id("revenue"));
        assert_eq!(
            ledger.commit(second),
            Err(LedgerStateError::BusinessKeyConflict(
                "invoice-1".to_owned()
            ))
        );
        assert_eq!(ledger.entry_count(), 1);
        assert_eq!(ledger.balance(&id("cash")), before_cash);
        assert_eq!(ledger.balance(&id("revenue")), before_revenue);
    }

    #[test]
    fn exact_replay_with_business_key_remains_idempotent() {
        let mut ledger = funded_ledger();
        let candidate = entry_with_business_key("entry-1", "idem-1", "invoice-1", 100);
        assert_eq!(
            ledger.commit(candidate.clone()),
            Ok(CommitOutcome::Committed)
        );
        assert_eq!(ledger.commit(candidate), Ok(CommitOutcome::Replayed));
        assert_eq!(ledger.entry_count(), 1);
        assert_eq!(ledger.balance(&id("cash")).unwrap().debits(), 100);
    }

    #[test]
    fn duplicate_entry_id_with_new_key_is_rejected() {
        let mut ledger = funded_ledger();
        let first = entry("entry-1", Some("idem-1"), "cash", "revenue", usd(), 100);
        let duplicate = entry("entry-1", Some("idem-2"), "cash", "revenue", usd(), 100);
        ledger.commit(first).unwrap();
        assert_eq!(
            ledger.commit(duplicate),
            Err(LedgerStateError::DuplicateEntryId(
                JournalEntryId::new("entry-1").unwrap()
            ))
        );
    }

    #[test]
    fn missing_idempotency_key_is_rejected() {
        let mut ledger = funded_ledger();
        let candidate = entry("entry-1", None, "cash", "revenue", usd(), 100);
        assert_eq!(
            ledger.commit(candidate),
            Err(LedgerStateError::MissingIdempotencyKey)
        );
        assert_eq!(ledger.entry_count(), 0);
    }

    #[test]
    fn missing_registered_account_balance_fails_closed() {
        let mut ledger = funded_ledger();
        ledger.balances.remove(&id("cash"));
        let candidate = entry("entry-1", Some("idem-1"), "cash", "revenue", usd(), 100);
        assert_eq!(
            ledger.commit(candidate),
            Err(LedgerStateError::InternalInvariant(
                "registered account is missing its balance"
            ))
        );
        assert_eq!(ledger.entry_count(), 0);
        assert_eq!(
            ledger.balance(&id("revenue")),
            Some(AccountBalance::default())
        );
    }

    #[test]
    fn balance_accumulation_overflow_is_rejected_without_partial_effect() {
        let mut ledger = funded_ledger();
        let max = i128::MAX;
        ledger
            .commit(entry(
                "entry-1",
                Some("idem-1"),
                "cash",
                "revenue",
                usd(),
                max,
            ))
            .unwrap();
        ledger
            .commit(entry(
                "entry-2",
                Some("idem-2"),
                "cash",
                "revenue",
                usd(),
                max,
            ))
            .unwrap();
        let before_cash = ledger.balance(&id("cash"));
        let before_revenue = ledger.balance(&id("revenue"));
        let overflow = entry("entry-3", Some("idem-3"), "cash", "revenue", usd(), 2);
        assert_eq!(
            ledger.commit(overflow),
            Err(LedgerStateError::BalanceOverflow(id("cash")))
        );
        assert_eq!(ledger.entry_count(), 2);
        assert_eq!(ledger.balance(&id("cash")), before_cash);
        assert_eq!(ledger.balance(&id("revenue")), before_revenue);
    }
}
