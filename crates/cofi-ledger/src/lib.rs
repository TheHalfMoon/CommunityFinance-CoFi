use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

mod state;
pub use state::{Account, AccountBalance, AccountKind, CommitOutcome, Ledger, LedgerStateError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    Debit,
    Credit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Currency([u8; 3]);

impl Currency {
    pub fn new(code: &str) -> Result<Self, LedgerError> {
        let bytes = code.as_bytes();
        if bytes.len() != 3 || !bytes.iter().all(u8::is_ascii_uppercase) {
            return Err(LedgerError::InvalidCurrency(code.to_owned()));
        }

        Ok(Self([bytes[0], bytes[1], bytes[2]]))
    }

    #[must_use]
    pub fn code(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("???")
    }
}

impl Display for Currency {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MinorAmount(u128);

impl MinorAmount {
    pub fn new(value: i128) -> Result<Self, LedgerError> {
        if value <= 0 {
            return Err(LedgerError::InvalidPostingAmount(value));
        }

        Ok(Self(value as u128))
    }

    #[must_use]
    pub const fn value(self) -> u128 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LedgerScopeId(String);

impl LedgerScopeId {
    pub fn new(value: impl Into<String>) -> Result<Self, LedgerError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(LedgerError::EmptyIdentifier("ledger_scope_id"));
        }

        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AccountId(String);

impl AccountId {
    pub fn new(value: impl Into<String>) -> Result<Self, LedgerError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(LedgerError::EmptyIdentifier("account_id"));
        }

        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JournalEntryId(String);

impl JournalEntryId {
    pub fn new(value: impl Into<String>) -> Result<Self, LedgerError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(LedgerError::EmptyIdentifier("journal_entry_id"));
        }

        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posting {
    account_id: AccountId,
    currency: Currency,
    side: Side,
    amount: MinorAmount,
}

impl Posting {
    pub fn new(
        account_id: AccountId,
        currency: Currency,
        side: Side,
        amount_minor: i128,
    ) -> Result<Self, LedgerError> {
        Ok(Self {
            account_id,
            currency,
            side,
            amount: MinorAmount::new(amount_minor)?,
        })
    }

    #[must_use]
    pub fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    #[must_use]
    pub const fn side(&self) -> Side {
        self.side
    }

    #[must_use]
    pub const fn amount(&self) -> MinorAmount {
        self.amount
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EntryMetadata {
    correlation_id: Option<String>,
    idempotency_key: Option<String>,
    business_key: Option<String>,
}

impl EntryMetadata {
    pub fn new(
        correlation_id: Option<String>,
        idempotency_key: Option<String>,
    ) -> Result<Self, LedgerError> {
        validate_optional_identifier("correlation_id", correlation_id.as_deref())?;
        validate_optional_identifier("idempotency_key", idempotency_key.as_deref())?;

        Ok(Self {
            correlation_id,
            idempotency_key,
            business_key: None,
        })
    }

    #[must_use]
    pub fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    #[must_use]
    pub fn idempotency_key(&self) -> Option<&str> {
        self.idempotency_key.as_deref()
    }

    pub fn with_business_key(mut self, business_key: Option<String>) -> Result<Self, LedgerError> {
        validate_optional_identifier("business_key", business_key.as_deref())?;
        self.business_key = business_key;
        Ok(self)
    }

    #[must_use]
    pub fn business_key(&self) -> Option<&str> {
        self.business_key.as_deref()
    }
}

fn validate_optional_identifier(
    name: &'static str,
    value: Option<&str>,
) -> Result<(), LedgerError> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err(LedgerError::EmptyIdentifier(name));
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    id: JournalEntryId,
    postings: Box<[Posting]>,
    effective_at_unix_ms: i64,
    recorded_at_unix_ms: i64,
    metadata: EntryMetadata,
}

impl JournalEntry {
    pub fn new(
        id: JournalEntryId,
        postings: Vec<Posting>,
        effective_at_unix_ms: i64,
        recorded_at_unix_ms: i64,
        metadata: EntryMetadata,
    ) -> Result<Self, LedgerError> {
        validate_postings(&postings)?;

        Ok(Self {
            id,
            postings: postings.into_boxed_slice(),
            effective_at_unix_ms,
            recorded_at_unix_ms,
            metadata,
        })
    }

    #[must_use]
    pub fn id(&self) -> &JournalEntryId {
        &self.id
    }

    #[must_use]
    pub fn postings(&self) -> &[Posting] {
        &self.postings
    }

    #[must_use]
    pub const fn effective_at_unix_ms(&self) -> i64 {
        self.effective_at_unix_ms
    }

    #[must_use]
    pub const fn recorded_at_unix_ms(&self) -> i64 {
        self.recorded_at_unix_ms
    }

    #[must_use]
    pub const fn metadata(&self) -> &EntryMetadata {
        &self.metadata
    }
}

fn validate_postings(postings: &[Posting]) -> Result<(), LedgerError> {
    if postings.len() < 2 {
        return Err(LedgerError::InsufficientPostings);
    }

    let distinct_accounts = postings
        .iter()
        .map(|posting| &posting.account_id)
        .collect::<BTreeSet<_>>();

    if distinct_accounts.len() < 2 {
        return Err(LedgerError::InsufficientAccounts);
    }

    let mut totals: BTreeMap<Currency, (u128, u128)> = BTreeMap::new();

    for posting in postings {
        let totals_for_currency = totals.entry(posting.currency).or_insert((0, 0));
        let total = match posting.side {
            Side::Debit => &mut totals_for_currency.0,
            Side::Credit => &mut totals_for_currency.1,
        };

        *total = total
            .checked_add(posting.amount.value())
            .ok_or(LedgerError::AmountOverflow(posting.currency))?;
    }

    for (currency, (debits, credits)) in totals {
        if debits != credits {
            return Err(LedgerError::Unbalanced {
                currency,
                debits,
                credits,
            });
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerError {
    InvalidCurrency(String),
    InvalidPostingAmount(i128),
    EmptyIdentifier(&'static str),
    InsufficientPostings,
    InsufficientAccounts,
    AmountOverflow(Currency),
    Unbalanced {
        currency: Currency,
        debits: u128,
        credits: u128,
    },
}

impl Display for LedgerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCurrency(code) => {
                write!(
                    f,
                    "currency must be exactly three uppercase ASCII letters: {code}"
                )
            }
            Self::InvalidPostingAmount(amount) => {
                write!(
                    f,
                    "posting amount must be a positive integer in minor units: {amount}"
                )
            }
            Self::EmptyIdentifier(name) => write!(f, "{name} must not be empty"),
            Self::InsufficientPostings => {
                f.write_str("journal entry requires at least two postings")
            }
            Self::InsufficientAccounts => {
                f.write_str("journal entry requires at least two distinct accounts")
            }
            Self::AmountOverflow(currency) => {
                write!(f, "posting total overflowed for currency {currency}")
            }
            Self::Unbalanced {
                currency,
                debits,
                credits,
            } => write!(
                f,
                "journal entry is unbalanced for {currency}: debits={debits}, credits={credits}"
            ),
        }
    }
}

impl Error for LedgerError {}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn account(value: &str) -> AccountId {
        AccountId::new(value).unwrap()
    }

    fn usd() -> Currency {
        Currency::new("USD").unwrap()
    }

    fn eur() -> Currency {
        Currency::new("EUR").unwrap()
    }

    fn posting(account_id: &str, currency: Currency, side: Side, amount: i128) -> Posting {
        Posting::new(account(account_id), currency, side, amount).unwrap()
    }

    fn entry(postings: Vec<Posting>) -> Result<JournalEntry, LedgerError> {
        JournalEntry::new(
            JournalEntryId::new("entry-1").unwrap(),
            postings,
            1_700_000_000_000,
            1_700_000_000_100,
            EntryMetadata::new(Some("corr-1".to_owned()), Some("idem-1".to_owned())).unwrap(),
        )
    }

    #[test]
    fn balanced_entry_is_accepted() {
        let journal_entry = entry(vec![
            posting("accounts-receivable", usd(), Side::Debit, 10_000),
            posting("revenue", usd(), Side::Credit, 10_000),
        ])
        .unwrap();

        assert_eq!(journal_entry.postings().len(), 2);
        assert_eq!(journal_entry.id().as_str(), "entry-1");
        assert_eq!(journal_entry.metadata().correlation_id(), Some("corr-1"));
        assert_eq!(journal_entry.metadata().idempotency_key(), Some("idem-1"));
        assert_eq!(journal_entry.metadata().business_key(), None);
    }

    #[test]
    fn unbalanced_entry_is_rejected() {
        let result = entry(vec![
            posting("cash", usd(), Side::Debit, 10_000),
            posting("revenue", usd(), Side::Credit, 9_999),
        ]);

        assert_eq!(
            result,
            Err(LedgerError::Unbalanced {
                currency: usd(),
                debits: 10_000,
                credits: 9_999,
            })
        );
    }

    #[test]
    fn zero_and_negative_postings_are_rejected() {
        assert_eq!(
            Posting::new(account("cash"), usd(), Side::Debit, 0),
            Err(LedgerError::InvalidPostingAmount(0))
        );
        assert_eq!(
            Posting::new(account("cash"), usd(), Side::Debit, -1),
            Err(LedgerError::InvalidPostingAmount(-1))
        );
    }

    #[test]
    fn currencies_must_balance_independently() {
        let result = entry(vec![
            posting("cash-usd", usd(), Side::Debit, 10_000),
            posting("revenue-usd", usd(), Side::Credit, 10_000),
            posting("cash-eur", eur(), Side::Debit, 8_000),
            posting("revenue-eur", eur(), Side::Credit, 7_999),
        ]);

        assert_eq!(
            result,
            Err(LedgerError::Unbalanced {
                currency: eur(),
                debits: 8_000,
                credits: 7_999,
            })
        );
    }

    #[test]
    fn balanced_multi_currency_entry_is_accepted() {
        let result = entry(vec![
            posting("cash-usd", usd(), Side::Debit, 10_000),
            posting("revenue-usd", usd(), Side::Credit, 10_000),
            posting("cash-eur", eur(), Side::Debit, 8_000),
            posting("revenue-eur", eur(), Side::Credit, 8_000),
        ]);

        assert!(result.is_ok());
    }

    #[test]
    fn entry_requires_two_postings() {
        let result = entry(vec![posting("cash", usd(), Side::Debit, 10_000)]);
        assert_eq!(result, Err(LedgerError::InsufficientPostings));
    }

    #[test]
    fn entry_requires_two_distinct_accounts() {
        let result = entry(vec![
            posting("cash", usd(), Side::Debit, 10_000),
            posting("cash", usd(), Side::Credit, 10_000),
        ]);
        assert_eq!(result, Err(LedgerError::InsufficientAccounts));
    }

    #[test]
    fn invalid_currency_is_rejected() {
        assert_eq!(
            Currency::new("usd"),
            Err(LedgerError::InvalidCurrency("usd".to_owned()))
        );
        assert_eq!(
            Currency::new("USDT"),
            Err(LedgerError::InvalidCurrency("USDT".to_owned()))
        );
    }

    #[test]
    fn empty_ledger_scope_identifier_is_rejected() {
        assert_eq!(
            LedgerScopeId::new("   "),
            Err(LedgerError::EmptyIdentifier("ledger_scope_id"))
        );
    }

    #[test]
    fn empty_metadata_identifiers_are_rejected() {
        assert_eq!(
            EntryMetadata::new(Some("   ".to_owned()), None),
            Err(LedgerError::EmptyIdentifier("correlation_id"))
        );
        assert_eq!(
            EntryMetadata::new(None, Some(String::new())),
            Err(LedgerError::EmptyIdentifier("idempotency_key"))
        );
        assert_eq!(
            EntryMetadata::new(None, Some("idem-1".to_owned()))
                .unwrap()
                .with_business_key(Some("   ".to_owned())),
            Err(LedgerError::EmptyIdentifier("business_key"))
        );
    }
}
