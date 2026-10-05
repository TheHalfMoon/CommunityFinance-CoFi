use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_community::{CommunityId, CommunityRegistry, FundId, OrganizationId};
use cofi_governance::{GovernanceEngine, SpendingProposalId};
use cofi_ledger::{
    AccountId, AccountKind, CommitOutcome, Currency, EntryMetadata, JournalEntry, JournalEntryId,
    Ledger, LedgerError, LedgerStateError, Posting, Side,
};

macro_rules! spending_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, FundSpendError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(FundSpendError::EmptyIdentifier($label));
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

spending_id!(FundSpendEventId, "fund_spend_event_id");
spending_id!(FundSpendId, "fund_spend_id");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedFundSpendEvent {
    source_event_id: FundSpendEventId,
    spend_id: FundSpendId,
    proposal_id: SpendingProposalId,
    organization_id: OrganizationId,
    community_id: CommunityId,
    fund_id: FundId,
    currency: Currency,
    amount_minor: i128,
    purpose_reference: String,
    expense_account_id: AccountId,
    executed_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}
impl ApprovedFundSpendEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_event_id: FundSpendEventId,
        spend_id: FundSpendId,
        proposal_id: SpendingProposalId,
        organization_id: OrganizationId,
        community_id: CommunityId,
        fund_id: FundId,
        currency: Currency,
        amount_minor: i128,
        purpose_reference: impl Into<String>,
        expense_account_id: AccountId,
        executed_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    ) -> Result<Self, FundSpendError> {
        if amount_minor <= 0 {
            return Err(FundSpendError::InvalidAmount(amount_minor));
        }
        let purpose_reference = purpose_reference.into();
        if purpose_reference.trim().is_empty() {
            return Err(FundSpendError::EmptyPurposeReference);
        }
        Ok(Self {
            source_event_id,
            spend_id,
            proposal_id,
            organization_id,
            community_id,
            fund_id,
            currency,
            amount_minor,
            purpose_reference,
            expense_account_id,
            executed_at_unix_ms,
            observed_at_unix_ms,
        })
    }

    #[must_use]
    pub const fn spend_id(&self) -> &FundSpendId {
        &self.spend_id
    }
    #[must_use]
    pub const fn proposal_id(&self) -> &SpendingProposalId {
        &self.proposal_id
    }
    #[must_use]
    pub const fn amount_minor(&self) -> i128 {
        self.amount_minor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedFundSpend {
    spend_id: FundSpendId,
    proposal_id: SpendingProposalId,
    journal_entry_id: JournalEntryId,
    organization_id: OrganizationId,
    community_id: CommunityId,
    fund_id: FundId,
    currency: Currency,
    amount_minor: i128,
    purpose_reference: String,
    executed_at_unix_ms: i64,
}

impl VerifiedFundSpend {
    #[must_use]
    pub const fn spend_id(&self) -> &FundSpendId {
        &self.spend_id
    }
    #[must_use]
    pub const fn proposal_id(&self) -> &SpendingProposalId {
        &self.proposal_id
    }
    #[must_use]
    pub const fn journal_entry_id(&self) -> &JournalEntryId {
        &self.journal_entry_id
    }
    #[must_use]
    pub const fn organization_id(&self) -> &OrganizationId {
        &self.organization_id
    }
    #[must_use]
    pub const fn community_id(&self) -> &CommunityId {
        &self.community_id
    }
    #[must_use]
    pub const fn fund_id(&self) -> &FundId {
        &self.fund_id
    }
    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }
    #[must_use]
    pub const fn amount_minor(&self) -> i128 {
        self.amount_minor
    }
    #[must_use]
    pub fn purpose_reference(&self) -> &str {
        &self.purpose_reference
    }
    #[must_use]
    pub const fn executed_at_unix_ms(&self) -> i64 {
        self.executed_at_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FundSpendOutcome {
    Committed { journal_entry_id: JournalEntryId },
    Replayed { journal_entry_id: JournalEntryId },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FundSpendBridge;

impl FundSpendBridge {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn apply(
        &self,
        registry: &CommunityRegistry,
        governance: &GovernanceEngine,
        event: &ApprovedFundSpendEvent,
        ledger: &mut Ledger,
    ) -> Result<FundSpendOutcome, FundSpendError> {
        let PreparedFundSpend {
            entry,
            fund_account_id,
            receipt: _,
        } = prepare_spend(registry, governance, event, ledger)?;
        let entry_id = entry.id().clone();

        if let Some(existing) = ledger.entry(&entry_id) {
            return if existing == &entry {
                Ok(FundSpendOutcome::Replayed {
                    journal_entry_id: entry_id,
                })
            } else {
                Err(FundSpendError::SpendIdConflict(event.spend_id.clone()))
            };
        }

        let balance = ledger
            .balance(&fund_account_id)
            .ok_or_else(|| FundSpendError::UnknownLedgerAccount(fund_account_id.clone()))?;
        let available = balance.debits().checked_sub(balance.credits()).ok_or(
            FundSpendError::FundNegativeBalance {
                debits: balance.debits(),
                credits: balance.credits(),
            },
        )?;
        let requested = event.amount_minor as u128;
        if available < requested {
            return Err(FundSpendError::InsufficientFundBalance {
                available,
                requested,
            });
        }

        match ledger.commit(entry).map_err(FundSpendError::LedgerCommit)? {
            CommitOutcome::Committed => Ok(FundSpendOutcome::Committed {
                journal_entry_id: entry_id,
            }),
            CommitOutcome::Replayed => Ok(FundSpendOutcome::Replayed {
                journal_entry_id: entry_id,
            }),
        }
    }

    pub fn verify_committed(
        &self,
        registry: &CommunityRegistry,
        governance: &GovernanceEngine,
        event: &ApprovedFundSpendEvent,
        ledger: &Ledger,
    ) -> Result<VerifiedFundSpend, FundSpendError> {
        let prepared = prepare_spend(registry, governance, event, ledger)?;
        match ledger.entry(prepared.entry.id()) {
            Some(existing) if existing == &prepared.entry => Ok(prepared.receipt),
            Some(_) => Err(FundSpendError::SpendIdConflict(event.spend_id.clone())),
            None => Err(FundSpendError::UncommittedSpend(event.spend_id.clone())),
        }
    }
}

struct PreparedFundSpend {
    entry: JournalEntry,
    fund_account_id: AccountId,
    receipt: VerifiedFundSpend,
}

fn prepare_spend(
    registry: &CommunityRegistry,
    governance: &GovernanceEngine,
    event: &ApprovedFundSpendEvent,
    ledger: &Ledger,
) -> Result<PreparedFundSpend, FundSpendError> {
    let authorization = governance
        .authorization(event.proposal_id())
        .ok_or_else(|| FundSpendError::UnknownAuthorization(event.proposal_id.clone()))?;
    validate_authorization_snapshot(event, authorization)?;
    if event.executed_at_unix_ms < authorization.approved_at_unix_ms() {
        return Err(FundSpendError::ExecutionBeforeApproval {
            approved_at_unix_ms: authorization.approved_at_unix_ms(),
            executed_at_unix_ms: event.executed_at_unix_ms,
        });
    }

    let fund = registry
        .fund(authorization.fund_id())
        .ok_or_else(|| FundSpendError::UnknownFund(authorization.fund_id().clone()))?;
    let community = registry
        .community(authorization.community_id())
        .ok_or_else(|| FundSpendError::UnknownCommunity(authorization.community_id().clone()))?;
    if fund.community_id() != authorization.community_id() {
        return Err(FundSpendError::FundCommunityMismatch);
    }
    if community.organization_id() != authorization.organization_id() {
        return Err(FundSpendError::CommunityOrganizationMismatch);
    }
    if fund.currency() != authorization.currency() {
        return Err(FundSpendError::FundCurrencyMismatch {
            expected: authorization.currency(),
            actual: fund.currency(),
        });
    }
    if fund.ledger_account_id() == &event.expense_account_id {
        return Err(FundSpendError::SamePostingAccount(
            event.expense_account_id.clone(),
        ));
    }
    validate_account(
        ledger,
        fund.ledger_account_id(),
        authorization.organization_id(),
        authorization.currency(),
        AccountKind::Asset,
    )?;
    validate_account(
        ledger,
        &event.expense_account_id,
        authorization.organization_id(),
        authorization.currency(),
        AccountKind::Expense,
    )?;

    let entry_id = journal_entry_id_for_fund_spend(event.spend_id())?;
    let metadata = EntryMetadata::new(
        Some(spend_payload_correlation(event)),
        Some(event.source_event_id.as_str().to_owned()),
    )
    .and_then(|metadata| {
        metadata.with_business_key(Some(format!(
            "spending:authorization:{}",
            event.proposal_id.as_str()
        )))
    })
    .map_err(FundSpendError::LedgerBuild)?;
    let postings = vec![
        Posting::new(
            event.expense_account_id.clone(),
            authorization.currency(),
            Side::Debit,
            authorization.amount_minor(),
        )
        .map_err(FundSpendError::LedgerBuild)?,
        Posting::new(
            fund.ledger_account_id().clone(),
            authorization.currency(),
            Side::Credit,
            authorization.amount_minor(),
        )
        .map_err(FundSpendError::LedgerBuild)?,
    ];
    let entry = JournalEntry::new(
        entry_id.clone(),
        postings,
        event.executed_at_unix_ms,
        event.observed_at_unix_ms,
        metadata,
    )
    .map_err(FundSpendError::LedgerBuild)?;
    let receipt = VerifiedFundSpend {
        spend_id: event.spend_id.clone(),
        proposal_id: event.proposal_id.clone(),
        journal_entry_id: entry_id,
        organization_id: event.organization_id.clone(),
        community_id: event.community_id.clone(),
        fund_id: event.fund_id.clone(),
        currency: event.currency,
        amount_minor: event.amount_minor,
        purpose_reference: event.purpose_reference.clone(),
        executed_at_unix_ms: event.executed_at_unix_ms,
    };
    Ok(PreparedFundSpend {
        entry,
        fund_account_id: fund.ledger_account_id().clone(),
        receipt,
    })
}

pub fn journal_entry_id_for_fund_spend(
    spend_id: &FundSpendId,
) -> Result<JournalEntryId, FundSpendError> {
    JournalEntryId::new(format!("spending:fund-spend:{}", spend_id.as_str()))
        .map_err(FundSpendError::LedgerBuild)
}
fn validate_authorization_snapshot(
    event: &ApprovedFundSpendEvent,
    authorization: &cofi_governance::ApprovedSpendingAuthorization,
) -> Result<(), FundSpendError> {
    if &event.organization_id != authorization.organization_id() {
        return Err(FundSpendError::AuthorizationSnapshotMismatch(
            "organization",
        ));
    }
    if &event.community_id != authorization.community_id() {
        return Err(FundSpendError::AuthorizationSnapshotMismatch("community"));
    }
    if &event.fund_id != authorization.fund_id() {
        return Err(FundSpendError::AuthorizationSnapshotMismatch("fund"));
    }
    if event.currency != authorization.currency() {
        return Err(FundSpendError::AuthorizationSnapshotMismatch("currency"));
    }
    if event.amount_minor != authorization.amount_minor() {
        return Err(FundSpendError::AuthorizationSnapshotMismatch("amount"));
    }
    if event.purpose_reference != authorization.purpose_reference() {
        return Err(FundSpendError::AuthorizationSnapshotMismatch(
            "purpose_reference",
        ));
    }
    Ok(())
}

fn validate_account(
    ledger: &Ledger,
    account_id: &AccountId,
    organization_id: &OrganizationId,
    currency: Currency,
    expected_kind: AccountKind,
) -> Result<(), FundSpendError> {
    let account = ledger
        .account(account_id)
        .ok_or_else(|| FundSpendError::UnknownLedgerAccount(account_id.clone()))?;
    if account.kind() != expected_kind {
        return Err(FundSpendError::AccountKindMismatch {
            account_id: account_id.clone(),
            expected: expected_kind,
            actual: account.kind(),
        });
    }
    if account.scope_id().as_str() != organization_id.as_str() {
        return Err(FundSpendError::AccountScopeMismatch {
            account_id: account_id.clone(),
            expected_organization: organization_id.clone(),
            actual_scope: account.scope_id().as_str().to_owned(),
        });
    }
    if account.currency() != currency {
        return Err(FundSpendError::AccountCurrencyMismatch {
            account_id: account_id.clone(),
            expected: currency,
            actual: account.currency(),
        });
    }
    Ok(())
}

fn spend_payload_correlation(event: &ApprovedFundSpendEvent) -> String {
    format!(
        "fund-spend:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        event.proposal_id.as_str(),
        event.spend_id.as_str(),
        event.organization_id.as_str(),
        event.community_id.as_str(),
        event.fund_id.as_str(),
        event.currency.code(),
        event.amount_minor,
        event.purpose_reference,
        event.expense_account_id.as_str(),
        event.executed_at_unix_ms,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FundSpendError {
    EmptyIdentifier(&'static str),
    InvalidAmount(i128),
    EmptyPurposeReference,
    UnknownAuthorization(SpendingProposalId),
    AuthorizationSnapshotMismatch(&'static str),
    ExecutionBeforeApproval {
        approved_at_unix_ms: i64,
        executed_at_unix_ms: i64,
    },
    UnknownFund(FundId),
    UnknownCommunity(CommunityId),
    FundCommunityMismatch,
    CommunityOrganizationMismatch,
    FundCurrencyMismatch {
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
        expected_organization: OrganizationId,
        actual_scope: String,
    },
    AccountCurrencyMismatch {
        account_id: AccountId,
        expected: Currency,
        actual: Currency,
    },
    SpendIdConflict(FundSpendId),
    UncommittedSpend(FundSpendId),
    FundNegativeBalance {
        debits: u128,
        credits: u128,
    },
    InsufficientFundBalance {
        available: u128,
        requested: u128,
    },
    LedgerBuild(LedgerError),
    LedgerCommit(LedgerStateError),
}

impl Display for FundSpendError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(label) => write!(formatter, "{label} must not be empty"),
            Self::InvalidAmount(amount) => write!(formatter, "invalid spend amount {amount}"),
            Self::EmptyPurposeReference => write!(formatter, "purpose reference must not be empty"),
            Self::UnknownAuthorization(id) => {
                write!(formatter, "unknown approved authorization {}", id.as_str())
            }
            Self::AuthorizationSnapshotMismatch(field) => {
                write!(formatter, "authorization snapshot mismatch: {field}")
            }
            Self::ExecutionBeforeApproval {
                approved_at_unix_ms,
                executed_at_unix_ms,
            } => write!(
                formatter,
                "spend execution {executed_at_unix_ms} precedes approval {approved_at_unix_ms}"
            ),
            Self::UnknownFund(id) => write!(formatter, "unknown fund {}", id.as_str()),
            Self::UnknownCommunity(id) => write!(formatter, "unknown community {}", id.as_str()),
            Self::FundCommunityMismatch => write!(formatter, "fund/community boundary mismatch"),
            Self::CommunityOrganizationMismatch => {
                write!(formatter, "community/organization boundary mismatch")
            }
            Self::FundCurrencyMismatch { expected, actual } => write!(
                formatter,
                "fund currency mismatch: expected {}, got {}",
                expected.code(),
                actual.code()
            ),
            Self::SamePostingAccount(id) => {
                write!(
                    formatter,
                    "fund and expense posting account are both {}",
                    id.as_str()
                )
            }
            Self::UnknownLedgerAccount(id) => {
                write!(formatter, "unknown ledger account {}", id.as_str())
            }
            Self::AccountKindMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                formatter,
                "account {} kind mismatch: expected {expected:?}, got {actual:?}",
                account_id.as_str()
            ),
            Self::AccountScopeMismatch {
                account_id,
                expected_organization,
                actual_scope,
            } => write!(
                formatter,
                "account {} scope mismatch: expected {}, got {actual_scope}",
                account_id.as_str(),
                expected_organization.as_str()
            ),
            Self::AccountCurrencyMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                formatter,
                "account {} currency mismatch: expected {}, got {}",
                account_id.as_str(),
                expected.code(),
                actual.code()
            ),
            Self::SpendIdConflict(id) => write!(formatter, "spend id {} conflicts", id.as_str()),
            Self::UncommittedSpend(id) => {
                write!(formatter, "spend {} is not committed", id.as_str())
            }
            Self::FundNegativeBalance { debits, credits } => write!(
                formatter,
                "fund balance is negative: debits={debits}, credits={credits}"
            ),
            Self::InsufficientFundBalance {
                available,
                requested,
            } => write!(
                formatter,
                "insufficient fund balance: available={available}, requested={requested}"
            ),
            Self::LedgerBuild(error) => write!(formatter, "ledger build failed: {error}"),
            Self::LedgerCommit(error) => write!(formatter, "ledger commit failed: {error}"),
        }
    }
}

impl Error for FundSpendError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LedgerBuild(error) => Some(error),
            Self::LedgerCommit(error) => Some(error),
            _ => None,
        }
    }
}
