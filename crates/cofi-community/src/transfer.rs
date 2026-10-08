use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_ledger::{
    AccountId, AccountKind, CommitOutcome, Currency, EntryMetadata, JournalEntry, JournalEntryId,
    Ledger, LedgerError, LedgerScopeId, LedgerStateError, Posting, Side,
};

use crate::{CommunityId, CommunityRegistry, FundId, OrganizationId};

macro_rules! transfer_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, FundTransferError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(FundTransferError::EmptyIdentifier($label));
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

transfer_id!(FundTransferEventId, "fund_transfer_event_id");
transfer_id!(FundTransferId, "fund_transfer_id");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundTransferEvent {
    source_event_id: FundTransferEventId,
    organization_scope: LedgerScopeId,
    source_fund_id: FundId,
    destination_fund_id: FundId,
    transfer_id: FundTransferId,
    currency: Currency,
    amount_minor: i128,
    effective_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

impl FundTransferEvent {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        source_event_id: FundTransferEventId,
        organization_scope: LedgerScopeId,
        source_fund_id: FundId,
        destination_fund_id: FundId,
        transfer_id: FundTransferId,
        currency: Currency,
        amount_minor: i128,
        effective_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            organization_scope,
            source_fund_id,
            destination_fund_id,
            transfer_id,
            currency,
            amount_minor,
            effective_at_unix_ms,
            observed_at_unix_ms,
        }
    }

    /// Original accepted source identity.
    #[must_use]
    pub fn source_event_id(&self) -> &FundTransferEventId {
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
    pub fn source_fund_id(&self) -> &FundId {
        &self.source_fund_id
    }

    #[must_use]
    pub fn destination_fund_id(&self) -> &FundId {
        &self.destination_fund_id
    }

    #[must_use]
    pub fn transfer_id(&self) -> &FundTransferId {
        &self.transfer_id
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
pub enum FundTransferOutcome {
    Committed { journal_entry_id: JournalEntryId },
    Replayed { journal_entry_id: JournalEntryId },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FundTransferBridge;

impl FundTransferBridge {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn apply(
        &self,
        registry: &CommunityRegistry,
        event: &FundTransferEvent,
        ledger: &mut Ledger,
    ) -> Result<FundTransferOutcome, FundTransferError> {
        if event.amount_minor <= 0 {
            return Err(FundTransferError::InvalidAmount(event.amount_minor));
        }
        if event.source_fund_id() == event.destination_fund_id() {
            return Err(FundTransferError::SameFund(event.source_fund_id().clone()));
        }

        let source_fund = registry
            .fund(event.source_fund_id())
            .ok_or_else(|| FundTransferError::UnknownFund(event.source_fund_id().clone()))?;
        let destination_fund = registry
            .fund(event.destination_fund_id())
            .ok_or_else(|| FundTransferError::UnknownFund(event.destination_fund_id().clone()))?;

        let source_community = registry
            .community(source_fund.community_id())
            .ok_or_else(|| {
                FundTransferError::MissingFundCommunity(source_fund.community_id().clone())
            })?;
        let destination_community = registry
            .community(destination_fund.community_id())
            .ok_or_else(|| {
                FundTransferError::MissingFundCommunity(destination_fund.community_id().clone())
            })?;

        validate_fund_scope(
            source_community.organization_id(),
            event.organization_scope(),
            event.source_fund_id(),
        )?;
        validate_fund_scope(
            destination_community.organization_id(),
            event.organization_scope(),
            event.destination_fund_id(),
        )?;

        validate_fund_currency(source_fund.id(), source_fund.currency(), event.currency())?;
        validate_fund_currency(
            destination_fund.id(),
            destination_fund.currency(),
            event.currency(),
        )?;

        if source_fund.ledger_account_id() == destination_fund.ledger_account_id() {
            return Err(FundTransferError::SamePostingAccount(
                source_fund.ledger_account_id().clone(),
            ));
        }

        validate_asset_account(
            ledger,
            source_fund.ledger_account_id(),
            event.organization_scope(),
            event.currency(),
        )?;
        validate_asset_account(
            ledger,
            destination_fund.ledger_account_id(),
            event.organization_scope(),
            event.currency(),
        )?;

        let entry_id = journal_entry_id_for_fund_transfer(event.transfer_id())?;
        let metadata = EntryMetadata::new(
            Some(transfer_payload_correlation(event)),
            Some(event.source_event_id.as_str().to_owned()),
        )
        .and_then(|metadata| {
            metadata.with_business_key(Some(format!(
                "community:fund-transfer:{}",
                event.transfer_id.as_str()
            )))
        })
        .map_err(FundTransferError::LedgerBuild)?;

        let postings = vec![
            Posting::new(
                destination_fund.ledger_account_id().clone(),
                event.currency(),
                Side::Debit,
                event.amount_minor,
            )
            .map_err(FundTransferError::LedgerBuild)?,
            Posting::new(
                source_fund.ledger_account_id().clone(),
                event.currency(),
                Side::Credit,
                event.amount_minor,
            )
            .map_err(FundTransferError::LedgerBuild)?,
        ];
        let entry = JournalEntry::new(
            entry_id.clone(),
            postings,
            event.effective_at_unix_ms,
            event.observed_at_unix_ms,
            metadata,
        )
        .map_err(FundTransferError::LedgerBuild)?;

        if ledger.entry(&entry_id) == Some(&entry) {
            return Ok(FundTransferOutcome::Replayed {
                journal_entry_id: entry_id,
            });
        }

        let balance = ledger
            .balance(source_fund.ledger_account_id())
            .ok_or_else(|| {
                FundTransferError::UnknownLedgerAccount(source_fund.ledger_account_id().clone())
            })?;
        let available = balance.debits().checked_sub(balance.credits()).ok_or(
            FundTransferError::SourceFundNegativeBalance {
                debits: balance.debits(),
                credits: balance.credits(),
            },
        )?;
        let requested = event.amount_minor as u128;
        if available < requested {
            return Err(FundTransferError::InsufficientSourceFund {
                available,
                requested,
            });
        }

        match ledger
            .commit(entry)
            .map_err(FundTransferError::LedgerCommit)?
        {
            CommitOutcome::Committed => Ok(FundTransferOutcome::Committed {
                journal_entry_id: entry_id,
            }),
            CommitOutcome::Replayed => Ok(FundTransferOutcome::Replayed {
                journal_entry_id: entry_id,
            }),
        }
    }
}

pub fn journal_entry_id_for_fund_transfer(
    transfer_id: &FundTransferId,
) -> Result<JournalEntryId, FundTransferError> {
    JournalEntryId::new(format!("community:fund-transfer:{}", transfer_id.as_str()))
        .map_err(FundTransferError::LedgerBuild)
}

fn transfer_payload_correlation(event: &FundTransferEvent) -> String {
    format!(
        "fund-transfer:{}:{}:{}:{}:{}:{}:{}",
        event.source_fund_id.as_str(),
        event.destination_fund_id.as_str(),
        event.transfer_id.as_str(),
        event.organization_scope.as_str(),
        event.currency.code(),
        event.amount_minor,
        event.effective_at_unix_ms,
    )
}

fn validate_fund_scope(
    organization_id: &OrganizationId,
    expected_scope: &LedgerScopeId,
    fund_id: &FundId,
) -> Result<(), FundTransferError> {
    if organization_id.as_str() != expected_scope.as_str() {
        return Err(FundTransferError::OrganizationScopeMismatch {
            fund_id: fund_id.clone(),
            fund_organization_id: organization_id.clone(),
            event_scope: expected_scope.clone(),
        });
    }
    Ok(())
}

fn validate_fund_currency(
    fund_id: &FundId,
    fund_currency: Currency,
    expected_currency: Currency,
) -> Result<(), FundTransferError> {
    if fund_currency != expected_currency {
        return Err(FundTransferError::FundCurrencyMismatch {
            fund_id: fund_id.clone(),
            expected: fund_currency,
            actual: expected_currency,
        });
    }
    Ok(())
}

fn validate_asset_account(
    ledger: &Ledger,
    account_id: &AccountId,
    expected_scope: &LedgerScopeId,
    expected_currency: Currency,
) -> Result<(), FundTransferError> {
    let account = ledger
        .account(account_id)
        .ok_or_else(|| FundTransferError::UnknownLedgerAccount(account_id.clone()))?;
    if account.kind() != AccountKind::Asset {
        return Err(FundTransferError::AccountKindMismatch {
            account_id: account_id.clone(),
            expected: AccountKind::Asset,
            actual: account.kind(),
        });
    }
    if account.scope_id() != expected_scope {
        return Err(FundTransferError::AccountScopeMismatch {
            account_id: account_id.clone(),
            expected: expected_scope.clone(),
            actual: account.scope_id().clone(),
        });
    }
    if account.currency() != expected_currency {
        return Err(FundTransferError::AccountCurrencyMismatch {
            account_id: account_id.clone(),
            expected: expected_currency,
            actual: account.currency(),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FundTransferError {
    EmptyIdentifier(&'static str),
    InvalidAmount(i128),
    SameFund(FundId),
    UnknownFund(FundId),
    MissingFundCommunity(CommunityId),
    OrganizationScopeMismatch {
        fund_id: FundId,
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
    SourceFundNegativeBalance {
        debits: u128,
        credits: u128,
    },
    InsufficientSourceFund {
        available: u128,
        requested: u128,
    },
    LedgerBuild(LedgerError),
    LedgerCommit(LedgerStateError),
}

impl Display for FundTransferError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(name) => write!(f, "{name} must not be empty"),
            Self::InvalidAmount(value) => {
                write!(f, "fund transfer amount must be positive: {value}")
            }
            Self::SameFund(id) => write!(
                f,
                "source and destination fund must be distinct: {}",
                id.as_str()
            ),
            Self::UnknownFund(id) => write!(f, "unknown fund: {}", id.as_str()),
            Self::MissingFundCommunity(id) => {
                write!(f, "fund references a missing community: {}", id.as_str())
            }
            Self::OrganizationScopeMismatch {
                fund_id,
                fund_organization_id,
                event_scope,
            } => write!(
                f,
                "fund {} belongs to organization {}; transfer scope is {}",
                fund_id.as_str(),
                fund_organization_id.as_str(),
                event_scope.as_str()
            ),
            Self::FundCurrencyMismatch {
                fund_id,
                expected,
                actual,
            } => write!(
                f,
                "fund {} uses currency {expected}; transfer uses {actual}",
                fund_id.as_str()
            ),
            Self::SamePostingAccount(id) => write!(
                f,
                "source and destination fund accounts must be distinct: {}",
                id.as_str()
            ),
            Self::UnknownLedgerAccount(id) => {
                write!(f, "unknown transfer ledger account: {}", id.as_str())
            }
            Self::AccountKindMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "transfer ledger account {} has kind {actual:?}; expected {expected:?}",
                account_id.as_str()
            ),
            Self::AccountScopeMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "transfer ledger account {} has scope {}; expected {}",
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
                "transfer ledger account {} has currency {actual}; expected {expected}",
                account_id.as_str()
            ),
            Self::SourceFundNegativeBalance { debits, credits } => write!(
                f,
                "source fund has negative debit-normal balance: debits={debits}, credits={credits}"
            ),
            Self::InsufficientSourceFund {
                available,
                requested,
            } => write!(
                f,
                "source fund balance is insufficient: available={available}, requested={requested}"
            ),
            Self::LedgerBuild(error) => {
                write!(f, "failed to build fund transfer entry: {error}")
            }
            Self::LedgerCommit(error) => {
                write!(f, "failed to commit fund transfer entry: {error}")
            }
        }
    }
}

impl Error for FundTransferError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LedgerBuild(error) => Some(error),
            Self::LedgerCommit(error) => Some(error),
            _ => None,
        }
    }
}
