use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_ledger::{
    AccountId, AccountKind, CommitOutcome, Currency, EntryMetadata, JournalEntry, JournalEntryId,
    Ledger, LedgerError, LedgerScopeId, LedgerStateError, Posting, Side,
};

use crate::{CommunityId, CommunityRegistry, FundId, OrganizationId};

macro_rules! allocation_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, FundAllocationError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(FundAllocationError::EmptyIdentifier($label));
                }
                Ok(Self(value))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

allocation_id!(FundAllocationEventId, "fund_allocation_event_id");
allocation_id!(FundAllocationId, "fund_allocation_id");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundAllocationEvent {
    source_event_id: FundAllocationEventId,
    organization_scope: LedgerScopeId,
    fund_id: FundId,
    allocation_id: FundAllocationId,
    currency: Currency,
    amount_minor: i128,
    effective_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

impl FundAllocationEvent {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        source_event_id: FundAllocationEventId,
        organization_scope: LedgerScopeId,
        fund_id: FundId,
        allocation_id: FundAllocationId,
        currency: Currency,
        amount_minor: i128,
        effective_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            organization_scope,
            fund_id,
            allocation_id,
            currency,
            amount_minor,
            effective_at_unix_ms,
            observed_at_unix_ms,
        }
    }

    /// Original accepted source identity.
    #[must_use]
    pub fn source_event_id(&self) -> &FundAllocationEventId {
        &self.source_event_id
    }

    /// Original effective time.
    #[must_use]
    pub const fn effective_at_unix_ms(&self) -> i64 {
        self.effective_at_unix_ms
    }

    /// Original observed time.
    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at_unix_ms
    }

    #[must_use]
    pub fn organization_scope(&self) -> &LedgerScopeId {
        &self.organization_scope
    }

    #[must_use]
    pub fn fund_id(&self) -> &FundId {
        &self.fund_id
    }

    #[must_use]
    pub fn allocation_id(&self) -> &FundAllocationId {
        &self.allocation_id
    }

    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    #[must_use]
    pub const fn amount_minor(&self) -> i128 {
        self.amount_minor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundAllocationAccounts {
    source_cash: AccountId,
}

impl FundAllocationAccounts {
    #[must_use]
    pub const fn new(source_cash: AccountId) -> Self {
        Self { source_cash }
    }

    #[must_use]
    pub fn source_cash(&self) -> &AccountId {
        &self.source_cash
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FundAllocationOutcome {
    Committed { journal_entry_id: JournalEntryId },
    Replayed { journal_entry_id: JournalEntryId },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FundAllocationBridge;

impl FundAllocationBridge {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn apply(
        &self,
        registry: &CommunityRegistry,
        event: &FundAllocationEvent,
        accounts: &FundAllocationAccounts,
        ledger: &mut Ledger,
    ) -> Result<FundAllocationOutcome, FundAllocationError> {
        if event.amount_minor <= 0 {
            return Err(FundAllocationError::InvalidAmount(event.amount_minor));
        }

        let fund = registry
            .fund(event.fund_id())
            .ok_or_else(|| FundAllocationError::UnknownFund(event.fund_id().clone()))?;
        let community = registry.community(fund.community_id()).ok_or_else(|| {
            FundAllocationError::MissingFundCommunity(fund.community_id().clone())
        })?;
        if community.organization_id().as_str() != event.organization_scope().as_str() {
            return Err(FundAllocationError::OrganizationScopeMismatch {
                fund_organization_id: community.organization_id().clone(),
                event_scope: event.organization_scope().clone(),
            });
        }
        if fund.currency() != event.currency() {
            return Err(FundAllocationError::FundCurrencyMismatch {
                fund_id: fund.id().clone(),
                expected: fund.currency(),
                actual: event.currency(),
            });
        }

        if accounts.source_cash() == fund.ledger_account_id() {
            return Err(FundAllocationError::SamePostingAccount(
                accounts.source_cash().clone(),
            ));
        }

        validate_asset_account(
            ledger,
            accounts.source_cash(),
            event.organization_scope(),
            event.currency(),
        )?;
        validate_asset_account(
            ledger,
            fund.ledger_account_id(),
            event.organization_scope(),
            event.currency(),
        )?;

        let entry_id = journal_entry_id_for_fund_allocation(event.allocation_id())?;
        let metadata = EntryMetadata::new(
            Some(allocation_payload_correlation(
                event,
                accounts.source_cash(),
            )),
            Some(event.source_event_id.as_str().to_owned()),
        )
        .and_then(|metadata| {
            metadata.with_business_key(Some(format!(
                "community:fund-allocation:{}",
                event.allocation_id.as_str()
            )))
        })
        .map_err(FundAllocationError::LedgerBuild)?;
        let postings = vec![
            Posting::new(
                fund.ledger_account_id().clone(),
                event.currency(),
                Side::Debit,
                event.amount_minor,
            )
            .map_err(FundAllocationError::LedgerBuild)?,
            Posting::new(
                accounts.source_cash().clone(),
                event.currency(),
                Side::Credit,
                event.amount_minor,
            )
            .map_err(FundAllocationError::LedgerBuild)?,
        ];
        let entry = JournalEntry::new(
            entry_id.clone(),
            postings,
            event.effective_at_unix_ms,
            event.observed_at_unix_ms,
            metadata,
        )
        .map_err(FundAllocationError::LedgerBuild)?;

        if ledger.entry(&entry_id) == Some(&entry) {
            return Ok(FundAllocationOutcome::Replayed {
                journal_entry_id: entry_id,
            });
        }

        let balance = ledger.balance(accounts.source_cash()).ok_or_else(|| {
            FundAllocationError::UnknownLedgerAccount(accounts.source_cash().clone())
        })?;
        let available = balance.debits().checked_sub(balance.credits()).ok_or(
            FundAllocationError::SourceCashNegativeBalance {
                debits: balance.debits(),
                credits: balance.credits(),
            },
        )?;
        let requested = event.amount_minor as u128;
        if available < requested {
            return Err(FundAllocationError::InsufficientSourceCash {
                available,
                requested,
            });
        }

        match ledger
            .commit(entry)
            .map_err(FundAllocationError::LedgerCommit)?
        {
            CommitOutcome::Committed => Ok(FundAllocationOutcome::Committed {
                journal_entry_id: entry_id,
            }),
            CommitOutcome::Replayed => Ok(FundAllocationOutcome::Replayed {
                journal_entry_id: entry_id,
            }),
        }
    }
}

pub fn journal_entry_id_for_fund_allocation(
    allocation_id: &FundAllocationId,
) -> Result<JournalEntryId, FundAllocationError> {
    JournalEntryId::new(format!(
        "community:fund-allocation:{}",
        allocation_id.as_str()
    ))
    .map_err(FundAllocationError::LedgerBuild)
}
fn allocation_payload_correlation(event: &FundAllocationEvent, source_cash: &AccountId) -> String {
    format!(
        "fund-allocation:{}:{}:{}:{}:{}:{}:{}",
        event.fund_id.as_str(),
        event.allocation_id.as_str(),
        source_cash.as_str(),
        event.organization_scope.as_str(),
        event.currency.code(),
        event.amount_minor,
        event.effective_at_unix_ms,
    )
}

fn validate_asset_account(
    ledger: &Ledger,
    account_id: &AccountId,
    expected_scope: &LedgerScopeId,
    expected_currency: Currency,
) -> Result<(), FundAllocationError> {
    let account = ledger
        .account(account_id)
        .ok_or_else(|| FundAllocationError::UnknownLedgerAccount(account_id.clone()))?;
    if account.kind() != AccountKind::Asset {
        return Err(FundAllocationError::AccountKindMismatch {
            account_id: account_id.clone(),
            expected: AccountKind::Asset,
            actual: account.kind(),
        });
    }
    if account.scope_id() != expected_scope {
        return Err(FundAllocationError::AccountScopeMismatch {
            account_id: account_id.clone(),
            expected: expected_scope.clone(),
            actual: account.scope_id().clone(),
        });
    }
    if account.currency() != expected_currency {
        return Err(FundAllocationError::AccountCurrencyMismatch {
            account_id: account_id.clone(),
            expected: expected_currency,
            actual: account.currency(),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FundAllocationError {
    EmptyIdentifier(&'static str),
    InvalidAmount(i128),
    UnknownFund(FundId),
    MissingFundCommunity(CommunityId),
    OrganizationScopeMismatch {
        fund_organization_id: OrganizationId,
        event_scope: LedgerScopeId,
    },
    FundCurrencyMismatch {
        fund_id: FundId,
        expected: Currency,
        actual: Currency,
    },
    SamePostingAccount(AccountId),
    UnknownLedgerAccount(AccountId),
    AccountKindMismatch {
        account_id: AccountId,
        expected: AccountKind,
        actual: AccountKind,
    },
    AccountScopeMismatch {
        account_id: AccountId,
        expected: LedgerScopeId,
        actual: LedgerScopeId,
    },
    AccountCurrencyMismatch {
        account_id: AccountId,
        expected: Currency,
        actual: Currency,
    },
    SourceCashNegativeBalance {
        debits: u128,
        credits: u128,
    },
    InsufficientSourceCash {
        available: u128,
        requested: u128,
    },
    LedgerBuild(LedgerError),
    LedgerCommit(LedgerStateError),
}

impl Display for FundAllocationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(name) => write!(f, "{name} must not be empty"),
            Self::InvalidAmount(value) => {
                write!(f, "fund allocation amount must be positive: {value}")
            }
            Self::UnknownFund(id) => write!(f, "unknown fund: {}", id.as_str()),
            Self::MissingFundCommunity(id) => {
                write!(f, "fund references a missing community: {}", id.as_str())
            }
            Self::OrganizationScopeMismatch {
                fund_organization_id,
                event_scope,
            } => write!(
                f,
                "fund organization {} does not match allocation scope {}",
                fund_organization_id.as_str(),
                event_scope.as_str()
            ),
            Self::FundCurrencyMismatch {
                fund_id,
                expected,
                actual,
            } => write!(
                f,
                "fund {} uses currency {expected}; allocation uses {actual}",
                fund_id.as_str()
            ),
            Self::SamePostingAccount(id) => write!(
                f,
                "source cash and target fund account must be distinct: {}",
                id.as_str()
            ),
            Self::UnknownLedgerAccount(id) => {
                write!(f, "unknown allocation ledger account: {}", id.as_str())
            }
            Self::AccountKindMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "allocation ledger account {} has kind {actual:?}; expected {expected:?}",
                account_id.as_str()
            ),
            Self::AccountScopeMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "allocation ledger account {} has scope {}; expected {}",
                account_id.as_str(),
                actual.as_str(),
                expected.as_str()
            ),
            Self::AccountCurrencyMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "allocation ledger account {} has currency {actual}; expected {expected}",
                account_id.as_str()
            ),
            Self::SourceCashNegativeBalance { debits, credits } => write!(
                f,
                "source cash has negative debit-normal balance: debits={debits}, credits={credits}"
            ),
            Self::InsufficientSourceCash {
                available,
                requested,
            } => write!(
                f,
                "insufficient source cash: available={available}, requested={requested}"
            ),
            Self::LedgerBuild(error) => {
                write!(f, "failed to build fund allocation journal entry: {error}")
            }
            Self::LedgerCommit(error) => {
                write!(f, "failed to commit fund allocation journal entry: {error}")
            }
        }
    }
}

impl Error for FundAllocationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LedgerBuild(error) => Some(error),
            Self::LedgerCommit(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{Community, CommunityError, Fund, Organization};
    use cofi_ledger::{Account, AccountKind};

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

    fn fund_id(value: &str) -> FundId {
        FundId::new(value).unwrap()
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

    fn seed_cash(ledger: &mut Ledger, amount: i128) {
        let metadata = EntryMetadata::new(
            Some("seed-cash-correlation".to_owned()),
            Some("seed-cash-idempotency".to_owned()),
        )
        .unwrap();
        let entry = JournalEntry::new(
            JournalEntryId::new("seed-cash").unwrap(),
            vec![
                Posting::new(account_id("bank-cash"), usd(), Side::Debit, amount).unwrap(),
                Posting::new(account_id("opening-equity"), usd(), Side::Credit, amount).unwrap(),
            ],
            1_700_000_000_000,
            1_700_000_000_100,
            metadata,
        )
        .unwrap();
        assert_eq!(ledger.commit(entry).unwrap(), CommitOutcome::Committed);
    }

    fn registry_and_ledger_with_cash(amount: i128) -> (CommunityRegistry, Ledger) {
        let mut ledger = Ledger::new();
        register_account(&mut ledger, "bank-cash", "org-1", AccountKind::Asset, usd());
        register_account(
            &mut ledger,
            "fund-asset",
            "org-1",
            AccountKind::Asset,
            usd(),
        );
        register_account(
            &mut ledger,
            "opening-equity",
            "org-1",
            AccountKind::Equity,
            usd(),
        );
        seed_cash(&mut ledger, amount);

        let mut registry = CommunityRegistry::new();
        registry
            .register_organization(Organization::new(OrganizationId::new("org-1").unwrap()))
            .unwrap();
        registry
            .register_community(Community::new(
                CommunityId::new("community-1").unwrap(),
                OrganizationId::new("org-1").unwrap(),
            ))
            .unwrap();
        registry
            .register_fund(
                Fund::new(
                    fund_id("fund-1"),
                    CommunityId::new("community-1").unwrap(),
                    account_id("fund-asset"),
                    usd(),
                ),
                &ledger,
            )
            .unwrap();
        (registry, ledger)
    }

    fn event(source: &str, allocation: &str, amount: i128) -> FundAllocationEvent {
        FundAllocationEvent::new(
            FundAllocationEventId::new(source).unwrap(),
            scope("org-1"),
            fund_id("fund-1"),
            FundAllocationId::new(allocation).unwrap(),
            usd(),
            amount,
            1_700_000_000_200,
            1_700_000_000_300,
        )
    }

    fn accounts() -> FundAllocationAccounts {
        FundAllocationAccounts::new(account_id("bank-cash"))
    }

    #[test]
    fn fund_registration_requires_asset_account() {
        let mut ledger = Ledger::new();
        register_account(
            &mut ledger,
            "bad-fund",
            "org-1",
            AccountKind::Revenue,
            usd(),
        );
        let mut registry = CommunityRegistry::new();
        registry
            .register_organization(Organization::new(OrganizationId::new("org-1").unwrap()))
            .unwrap();
        registry
            .register_community(Community::new(
                CommunityId::new("community-1").unwrap(),
                OrganizationId::new("org-1").unwrap(),
            ))
            .unwrap();
        let fund = Fund::new(
            fund_id("fund-bad"),
            CommunityId::new("community-1").unwrap(),
            account_id("bad-fund"),
            usd(),
        );
        assert_eq!(
            registry.register_fund(fund.clone(), &ledger),
            Err(CommunityError::FundLedgerAccountKindMismatch {
                fund_id: fund_id("fund-bad"),
                actual: AccountKind::Revenue,
            })
        );
        assert!(registry.fund(fund.id()).is_none());
    }

    #[test]
    fn allocates_settled_cash_into_registered_fund() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        let allocation = event("allocation-event-1", "allocation-1", 400);
        let outcome = FundAllocationBridge::new()
            .apply(&registry, &allocation, &accounts(), &mut ledger)
            .unwrap();
        assert!(matches!(outcome, FundAllocationOutcome::Committed { .. }));
        let bank = ledger.balance(&account_id("bank-cash")).unwrap();
        let fund = ledger.balance(&account_id("fund-asset")).unwrap();
        assert_eq!((bank.debits(), bank.credits()), (1_000, 400));
        assert_eq!((fund.debits(), fund.credits()), (400, 0));
    }

    #[test]
    fn rejects_zero_and_negative_allocation_amounts() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        for amount in [0, -1] {
            let allocation = event("allocation-event-invalid", "allocation-invalid", amount);
            assert_eq!(
                FundAllocationBridge::new().apply(&registry, &allocation, &accounts(), &mut ledger),
                Err(FundAllocationError::InvalidAmount(amount))
            );
        }
    }

    #[test]
    fn rejects_unknown_fund_without_mutation() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        let mut allocation = event("allocation-event-2", "allocation-2", 100);
        allocation.fund_id = fund_id("missing-fund");
        let before = ledger.entry_count();
        assert_eq!(
            FundAllocationBridge::new().apply(&registry, &allocation, &accounts(), &mut ledger),
            Err(FundAllocationError::UnknownFund(fund_id("missing-fund")))
        );
        assert_eq!(ledger.entry_count(), before);
    }

    #[test]
    fn source_cash_and_fund_account_must_be_distinct() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        let allocation = event("allocation-event-3", "allocation-3", 100);
        let same = FundAllocationAccounts::new(account_id("fund-asset"));
        assert!(matches!(
            FundAllocationBridge::new().apply(&registry, &allocation, &same, &mut ledger),
            Err(FundAllocationError::SamePostingAccount(_))
        ));
    }

    #[test]
    fn source_cash_kind_scope_and_currency_are_validated() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        register_account(
            &mut ledger,
            "revenue-source",
            "org-1",
            AccountKind::Revenue,
            usd(),
        );
        register_account(
            &mut ledger,
            "other-scope",
            "org-2",
            AccountKind::Asset,
            usd(),
        );
        register_account(
            &mut ledger,
            "eur-source",
            "org-1",
            AccountKind::Asset,
            eur(),
        );
        let allocation = event("allocation-event-4", "allocation-4", 100);

        for (account, expected) in [
            ("revenue-source", "kind"),
            ("other-scope", "scope"),
            ("eur-source", "currency"),
        ] {
            let result = FundAllocationBridge::new().apply(
                &registry,
                &allocation,
                &FundAllocationAccounts::new(account_id(account)),
                &mut ledger,
            );
            match expected {
                "kind" => assert!(matches!(
                    result,
                    Err(FundAllocationError::AccountKindMismatch { .. })
                )),
                "scope" => assert!(matches!(
                    result,
                    Err(FundAllocationError::AccountScopeMismatch { .. })
                )),
                _ => assert!(matches!(
                    result,
                    Err(FundAllocationError::AccountCurrencyMismatch { .. })
                )),
            }
        }
    }

    #[test]
    fn allocation_cannot_exceed_available_source_cash() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        let allocation = event("allocation-event-5", "allocation-5", 1_001);
        let before = ledger.entry_count();
        assert_eq!(
            FundAllocationBridge::new().apply(&registry, &allocation, &accounts(), &mut ledger),
            Err(FundAllocationError::InsufficientSourceCash {
                available: 1_000,
                requested: 1_001,
            })
        );
        assert_eq!(ledger.entry_count(), before);
    }

    #[test]
    fn exact_replay_has_zero_duplicate_effect() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        let allocation = event("allocation-event-6", "allocation-6", 100);
        let bridge = FundAllocationBridge::new();
        assert!(matches!(
            bridge
                .apply(&registry, &allocation, &accounts(), &mut ledger)
                .unwrap(),
            FundAllocationOutcome::Committed { .. }
        ));
        let count = ledger.entry_count();
        assert!(matches!(
            bridge
                .apply(&registry, &allocation, &accounts(), &mut ledger)
                .unwrap(),
            FundAllocationOutcome::Replayed { .. }
        ));
        assert_eq!(ledger.entry_count(), count);
    }

    #[test]
    fn exact_replay_after_full_cash_allocation_still_replays() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        let allocation = event("allocation-event-full", "allocation-full", 1_000);
        let bridge = FundAllocationBridge::new();
        assert!(matches!(
            bridge
                .apply(&registry, &allocation, &accounts(), &mut ledger)
                .unwrap(),
            FundAllocationOutcome::Committed { .. }
        ));
        let count = ledger.entry_count();
        let bank = ledger.balance(&account_id("bank-cash")).unwrap();
        assert_eq!(bank.debits(), bank.credits());
        assert!(matches!(
            bridge
                .apply(&registry, &allocation, &accounts(), &mut ledger)
                .unwrap(),
            FundAllocationOutcome::Replayed { .. }
        ));
        assert_eq!(ledger.entry_count(), count);
    }

    #[test]
    fn conflicting_source_event_reuse_fails_closed() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        let first = event("shared-allocation-source", "allocation-7a", 100);
        FundAllocationBridge::new()
            .apply(&registry, &first, &accounts(), &mut ledger)
            .unwrap();
        let count = ledger.entry_count();
        let second = event("shared-allocation-source", "allocation-7b", 100);
        assert!(
            FundAllocationBridge::new()
                .apply(&registry, &second, &accounts(), &mut ledger)
                .is_err()
        );
        assert_eq!(ledger.entry_count(), count);
    }

    #[test]
    fn same_allocation_id_cannot_create_second_history() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        let first = event("allocation-source-8a", "allocation-8", 100);
        FundAllocationBridge::new()
            .apply(&registry, &first, &accounts(), &mut ledger)
            .unwrap();
        let count = ledger.entry_count();
        let second = event("allocation-source-8b", "allocation-8", 100);
        assert!(
            FundAllocationBridge::new()
                .apply(&registry, &second, &accounts(), &mut ledger)
                .is_err()
        );
        assert_eq!(ledger.entry_count(), count);
    }

    #[test]
    fn rejected_allocation_does_not_reserve_identity() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        let invalid = event("allocation-source-9a", "allocation-9", 1_001);
        assert!(
            FundAllocationBridge::new()
                .apply(&registry, &invalid, &accounts(), &mut ledger)
                .is_err()
        );
        let valid = event("allocation-source-9b", "allocation-9", 100);
        assert!(matches!(
            FundAllocationBridge::new()
                .apply(&registry, &valid, &accounts(), &mut ledger)
                .unwrap(),
            FundAllocationOutcome::Committed { .. }
        ));
    }

    #[test]
    fn fund_scope_and_currency_must_match_event() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        let mut wrong_scope = event("allocation-source-10a", "allocation-10a", 100);
        wrong_scope.organization_scope = scope("org-2");
        assert!(matches!(
            FundAllocationBridge::new().apply(&registry, &wrong_scope, &accounts(), &mut ledger),
            Err(FundAllocationError::OrganizationScopeMismatch { .. })
        ));
        let mut wrong_currency = event("allocation-source-10b", "allocation-10b", 100);
        wrong_currency.currency = eur();
        assert!(matches!(
            FundAllocationBridge::new().apply(&registry, &wrong_currency, &accounts(), &mut ledger),
            Err(FundAllocationError::FundCurrencyMismatch { .. })
        ));
    }

    #[test]
    fn unknown_source_cash_is_rejected() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(1_000);
        let allocation = event("allocation-source-11", "allocation-11", 100);
        let unknown = FundAllocationAccounts::new(account_id("missing-cash"));
        assert!(matches!(
            FundAllocationBridge::new().apply(&registry, &allocation, &unknown, &mut ledger),
            Err(FundAllocationError::UnknownLedgerAccount(_))
        ));
    }

    #[test]
    fn negative_debit_normal_source_balance_fails_closed() {
        let (registry, mut ledger) = registry_and_ledger_with_cash(100);
        register_account(
            &mut ledger,
            "loss-expense",
            "org-1",
            AccountKind::Expense,
            usd(),
        );
        let metadata = EntryMetadata::new(
            Some("drain-bank-correlation".to_owned()),
            Some("drain-bank-idempotency".to_owned()),
        )
        .unwrap();
        let drain = JournalEntry::new(
            JournalEntryId::new("drain-bank").unwrap(),
            vec![
                Posting::new(account_id("loss-expense"), usd(), Side::Debit, 200).unwrap(),
                Posting::new(account_id("bank-cash"), usd(), Side::Credit, 200).unwrap(),
            ],
            1_700_000_000_150,
            1_700_000_000_160,
            metadata,
        )
        .unwrap();
        ledger.commit(drain).unwrap();

        let allocation = event("allocation-source-12", "allocation-12", 10);
        assert_eq!(
            FundAllocationBridge::new().apply(&registry, &allocation, &accounts(), &mut ledger),
            Err(FundAllocationError::SourceCashNegativeBalance {
                debits: 100,
                credits: 200,
            })
        );
    }
}
