use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_community::{
    CommunityRegistry, FundAllocationError, FundId, FundTransferBridge, FundTransferError,
    FundTransferEvent, FundTransferEventId, FundTransferId, FundTransferOutcome,
    journal_entry_id_for_fund_allocation,
};
use cofi_fund_allocation_authorization::AuthorizedFundAllocation;
use cofi_ledger::{AccountId, AccountKind, Currency, JournalEntryId, Ledger, LedgerScopeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedFundTransferRequest {
    source_event_id: FundTransferEventId,
    transfer_id: FundTransferId,
    destination_fund_id: FundId,
    authorized_allocation: AuthorizedFundAllocation,
    effective_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

impl AuthorizedFundTransferRequest {
    #[must_use]
    pub const fn new(
        source_event_id: FundTransferEventId,
        transfer_id: FundTransferId,
        destination_fund_id: FundId,
        authorized_allocation: AuthorizedFundAllocation,
        effective_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            transfer_id,
            destination_fund_id,
            authorized_allocation,
            effective_at_unix_ms,
            observed_at_unix_ms,
        }
    }

    #[must_use]
    pub const fn source_event_id(&self) -> &FundTransferEventId {
        &self.source_event_id
    }

    #[must_use]
    pub const fn transfer_id(&self) -> &FundTransferId {
        &self.transfer_id
    }

    #[must_use]
    pub const fn destination_fund_id(&self) -> &FundId {
        &self.destination_fund_id
    }

    #[must_use]
    pub const fn authorized_allocation(&self) -> &AuthorizedFundAllocation {
        &self.authorized_allocation
    }

    #[must_use]
    pub const fn effective_at_unix_ms(&self) -> i64 {
        self.effective_at_unix_ms
    }

    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedFundTransfer {
    authorized_allocation: AuthorizedFundAllocation,
    transfer_event: FundTransferEvent,
    journal_entry_id: JournalEntryId,
}

impl AuthorizedFundTransfer {
    #[must_use]
    pub const fn authorized_allocation(&self) -> &AuthorizedFundAllocation {
        &self.authorized_allocation
    }

    #[must_use]
    pub const fn transfer_event(&self) -> &FundTransferEvent {
        &self.transfer_event
    }

    #[must_use]
    pub const fn journal_entry_id(&self) -> &JournalEntryId {
        &self.journal_entry_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizedFundTransferOutcome {
    Created {
        authorization: AuthorizedFundTransfer,
    },
    Replayed {
        authorization: AuthorizedFundTransfer,
    },
}

impl AuthorizedFundTransferOutcome {
    #[must_use]
    pub const fn authorization(&self) -> &AuthorizedFundTransfer {
        match self {
            Self::Created { authorization } | Self::Replayed { authorization } => authorization,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredAuthorizedFundTransfer {
    request: AuthorizedFundTransferRequest,
    authorization: AuthorizedFundTransfer,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorizedFundTransferRegistry {
    events: BTreeMap<FundTransferEventId, StoredAuthorizedFundTransfer>,
    allocation_transfers: BTreeMap<JournalEntryId, FundTransferEventId>,
}

impl AuthorizedFundTransferRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn authorization_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn allocation_transfer_count(&self) -> usize {
        self.allocation_transfers.len()
    }

    #[must_use]
    pub fn authorization_for_event(
        &self,
        event_id: &FundTransferEventId,
    ) -> Option<&AuthorizedFundTransfer> {
        self.events
            .get(event_id)
            .map(|stored| &stored.authorization)
    }

    pub fn apply(
        &mut self,
        community_registry: &CommunityRegistry,
        request: AuthorizedFundTransferRequest,
        ledger: &mut Ledger,
    ) -> Result<AuthorizedFundTransferOutcome, AuthorizedFundTransferError> {
        if let Some(existing) = self.events.get(request.source_event_id()) {
            return if existing.request == request {
                Ok(AuthorizedFundTransferOutcome::Replayed {
                    authorization: existing.authorization.clone(),
                })
            } else {
                Err(AuthorizedFundTransferError::SourceEventConflict(
                    request.source_event_id().clone(),
                ))
            };
        }

        let allocation_journal_entry_id = request.authorized_allocation.journal_entry_id().clone();
        if let Some(existing_event_id) = self.allocation_transfers.get(&allocation_journal_entry_id)
        {
            return Err(AuthorizedFundTransferError::AllocationAlreadyTransferred {
                allocation_journal_entry_id,
                existing_event_id: existing_event_id.clone(),
            });
        }

        let allocation_event = request.authorized_allocation.allocation_event();
        let source_fund_id = allocation_event.fund_id().clone();
        let organization_scope = allocation_event.organization_scope().clone();
        let currency = allocation_event.currency();
        let amount_minor = allocation_event.amount_minor();

        let allocation_effective_at_unix_ms = derive_canonical_allocation_effective_at(
            community_registry,
            ledger,
            &request.authorized_allocation,
        )?;

        if request.effective_at_unix_ms < allocation_effective_at_unix_ms {
            return Err(AuthorizedFundTransferError::TransferPrecedesAllocation {
                transfer_effective_at: request.effective_at_unix_ms,
                allocation_effective_at: allocation_effective_at_unix_ms,
            });
        }
        if request.observed_at_unix_ms < request.effective_at_unix_ms {
            return Err(AuthorizedFundTransferError::ObservationPrecedesTransfer {
                observed_at: request.observed_at_unix_ms,
                effective_at: request.effective_at_unix_ms,
            });
        }

        let transfer_event = FundTransferEvent::new(
            request.source_event_id.clone(),
            organization_scope,
            source_fund_id,
            request.destination_fund_id.clone(),
            request.transfer_id.clone(),
            currency,
            amount_minor,
            request.effective_at_unix_ms,
            request.observed_at_unix_ms,
        );

        let journal_entry_id = match FundTransferBridge::new()
            .apply(community_registry, &transfer_event, ledger)
            .map_err(AuthorizedFundTransferError::Transfer)?
        {
            FundTransferOutcome::Committed { journal_entry_id } => journal_entry_id,
            FundTransferOutcome::Replayed { journal_entry_id } => {
                return Err(AuthorizedFundTransferError::UnexpectedInternalReplay {
                    source_event_id: request.source_event_id.clone(),
                    journal_entry_id,
                });
            }
        };

        let authorization = AuthorizedFundTransfer {
            authorized_allocation: request.authorized_allocation.clone(),
            transfer_event,
            journal_entry_id,
        };
        self.allocation_transfers
            .insert(allocation_journal_entry_id, request.source_event_id.clone());
        self.events.insert(
            request.source_event_id.clone(),
            StoredAuthorizedFundTransfer {
                request,
                authorization: authorization.clone(),
            },
        );

        Ok(AuthorizedFundTransferOutcome::Created { authorization })
    }
}

fn derive_canonical_allocation_effective_at(
    community_registry: &CommunityRegistry,
    ledger: &Ledger,
    authorization: &AuthorizedFundAllocation,
) -> Result<i64, AuthorizedFundTransferError> {
    let allocation_event = authorization.allocation_event();
    let expected_journal_entry_id =
        journal_entry_id_for_fund_allocation(allocation_event.allocation_id())
            .map_err(AuthorizedFundTransferError::AllocationLineage)?;
    if &expected_journal_entry_id != authorization.journal_entry_id() {
        return Err(
            AuthorizedFundTransferError::CanonicalAllocationJournalIdentityMismatch {
                expected: expected_journal_entry_id,
                actual: authorization.journal_entry_id().clone(),
            },
        );
    }

    let allocation_entry = ledger
        .entry(authorization.journal_entry_id())
        .ok_or_else(|| {
            AuthorizedFundTransferError::MissingCanonicalAllocationJournal(
                authorization.journal_entry_id().clone(),
            )
        })?;

    let source_fund = community_registry
        .fund(allocation_event.fund_id())
        .ok_or_else(|| {
            AuthorizedFundTransferError::MissingCanonicalSourceFund(
                allocation_event.fund_id().clone(),
            )
        })?;

    if source_fund.currency() != allocation_event.currency() {
        return Err(
            AuthorizedFundTransferError::CanonicalSourceFundCurrencyMismatch {
                fund_id: source_fund.id().clone(),
                expected: allocation_event.currency(),
                actual: source_fund.currency(),
            },
        );
    }

    let source_account = ledger
        .account(source_fund.ledger_account_id())
        .ok_or_else(|| {
            AuthorizedFundTransferError::CanonicalSourceFundUnknownAccount(
                source_fund.ledger_account_id().clone(),
            )
        })?;
    if source_account.kind() != AccountKind::Asset {
        return Err(
            AuthorizedFundTransferError::CanonicalSourceFundAccountKindMismatch {
                account_id: source_account.id().clone(),
                actual: source_account.kind(),
            },
        );
    }
    if source_account.scope_id() != allocation_event.organization_scope() {
        return Err(
            AuthorizedFundTransferError::CanonicalSourceFundAccountScopeMismatch {
                account_id: source_account.id().clone(),
                expected: allocation_event.organization_scope().clone(),
                actual: source_account.scope_id().clone(),
            },
        );
    }
    if source_account.currency() != allocation_event.currency() {
        return Err(
            AuthorizedFundTransferError::CanonicalSourceFundAccountCurrencyMismatch {
                account_id: source_account.id().clone(),
                expected: allocation_event.currency(),
                actual: source_account.currency(),
            },
        );
    }

    let expected_amount = u128::try_from(allocation_event.amount_minor()).map_err(|_| {
        AuthorizedFundTransferError::InvalidCanonicalAllocationAmount(
            allocation_event.amount_minor(),
        )
    })?;
    if expected_amount == 0 {
        return Err(
            AuthorizedFundTransferError::InvalidCanonicalAllocationAmount(
                allocation_event.amount_minor(),
            ),
        );
    }

    let mut source_postings = allocation_entry
        .postings()
        .iter()
        .filter(|posting| posting.account_id() == source_fund.ledger_account_id());
    let source_posting = source_postings.next().ok_or_else(|| {
        AuthorizedFundTransferError::MissingCanonicalSourceFundDebit {
            journal_entry_id: authorization.journal_entry_id().clone(),
            account_id: source_fund.ledger_account_id().clone(),
        }
    })?;
    if source_postings.next().is_some() {
        return Err(
            AuthorizedFundTransferError::AmbiguousCanonicalSourceFundDebit {
                journal_entry_id: authorization.journal_entry_id().clone(),
                account_id: source_fund.ledger_account_id().clone(),
            },
        );
    }
    if source_posting.currency() != allocation_event.currency()
        || source_posting.side() != source_account.kind().normal_side()
        || source_posting.amount().value() != expected_amount
    {
        return Err(
            AuthorizedFundTransferError::CanonicalSourceFundDebitMismatch {
                journal_entry_id: authorization.journal_entry_id().clone(),
                account_id: source_fund.ledger_account_id().clone(),
            },
        );
    }

    Ok(allocation_entry.effective_at_unix_ms())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizedFundTransferError {
    SourceEventConflict(FundTransferEventId),
    AllocationAlreadyTransferred {
        allocation_journal_entry_id: JournalEntryId,
        existing_event_id: FundTransferEventId,
    },
    CanonicalAllocationJournalIdentityMismatch {
        expected: JournalEntryId,
        actual: JournalEntryId,
    },
    MissingCanonicalAllocationJournal(JournalEntryId),
    MissingCanonicalSourceFund(FundId),
    CanonicalSourceFundCurrencyMismatch {
        fund_id: FundId,
        expected: Currency,
        actual: Currency,
    },
    CanonicalSourceFundUnknownAccount(AccountId),
    CanonicalSourceFundAccountKindMismatch {
        account_id: AccountId,
        actual: AccountKind,
    },
    CanonicalSourceFundAccountScopeMismatch {
        account_id: AccountId,
        expected: LedgerScopeId,
        actual: LedgerScopeId,
    },
    CanonicalSourceFundAccountCurrencyMismatch {
        account_id: AccountId,
        expected: Currency,
        actual: Currency,
    },
    InvalidCanonicalAllocationAmount(i128),
    MissingCanonicalSourceFundDebit {
        journal_entry_id: JournalEntryId,
        account_id: AccountId,
    },
    AmbiguousCanonicalSourceFundDebit {
        journal_entry_id: JournalEntryId,
        account_id: AccountId,
    },
    CanonicalSourceFundDebitMismatch {
        journal_entry_id: JournalEntryId,
        account_id: AccountId,
    },
    TransferPrecedesAllocation {
        transfer_effective_at: i64,
        allocation_effective_at: i64,
    },
    ObservationPrecedesTransfer {
        observed_at: i64,
        effective_at: i64,
    },
    AllocationLineage(FundAllocationError),
    Transfer(FundTransferError),
    UnexpectedInternalReplay {
        source_event_id: FundTransferEventId,
        journal_entry_id: JournalEntryId,
    },
}

impl Display for AuthorizedFundTransferError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceEventConflict(event_id) => write!(
                formatter,
                "authorized fund-transfer source event conflict: {}",
                event_id.as_str()
            ),
            Self::AllocationAlreadyTransferred {
                allocation_journal_entry_id,
                existing_event_id,
            } => write!(
                formatter,
                "canonical P26 allocation journal {} already has P27 transfer event {}",
                allocation_journal_entry_id.as_str(),
                existing_event_id.as_str()
            ),
            Self::CanonicalAllocationJournalIdentityMismatch { expected, actual } => write!(
                formatter,
                "canonical P26 allocation journal identity mismatch: expected {}, got {}",
                expected.as_str(),
                actual.as_str()
            ),
            Self::MissingCanonicalAllocationJournal(entry_id) => write!(
                formatter,
                "missing canonical P26 allocation journal entry: {}",
                entry_id.as_str()
            ),
            Self::MissingCanonicalSourceFund(fund_id) => write!(
                formatter,
                "canonical P26 allocation references missing source fund: {}",
                fund_id.as_str()
            ),
            Self::CanonicalSourceFundCurrencyMismatch {
                fund_id,
                expected,
                actual,
            } => write!(
                formatter,
                "canonical source fund {} uses currency {actual}; expected {expected}",
                fund_id.as_str()
            ),
            Self::CanonicalSourceFundUnknownAccount(account_id) => write!(
                formatter,
                "canonical source fund references unknown ledger account: {}",
                account_id.as_str()
            ),
            Self::CanonicalSourceFundAccountKindMismatch { account_id, actual } => write!(
                formatter,
                "canonical source fund account {} has kind {actual:?}; expected Asset",
                account_id.as_str()
            ),
            Self::CanonicalSourceFundAccountScopeMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                formatter,
                "canonical source fund account {} has scope {}; expected {}",
                account_id.as_str(),
                actual.as_str(),
                expected.as_str()
            ),
            Self::CanonicalSourceFundAccountCurrencyMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                formatter,
                "canonical source fund account {} uses currency {actual}; expected {expected}",
                account_id.as_str()
            ),
            Self::InvalidCanonicalAllocationAmount(amount) => write!(
                formatter,
                "canonical P26 allocation amount must be positive: {amount}"
            ),
            Self::MissingCanonicalSourceFundDebit {
                journal_entry_id,
                account_id,
            } => write!(
                formatter,
                "canonical P26 allocation journal {} has no posting for source fund account {}",
                journal_entry_id.as_str(),
                account_id.as_str()
            ),
            Self::AmbiguousCanonicalSourceFundDebit {
                journal_entry_id,
                account_id,
            } => write!(
                formatter,
                "canonical P26 allocation journal {} has multiple postings for source fund account {}",
                journal_entry_id.as_str(),
                account_id.as_str()
            ),
            Self::CanonicalSourceFundDebitMismatch {
                journal_entry_id,
                account_id,
            } => write!(
                formatter,
                "canonical P26 allocation journal {} does not contain the exact derived debit for source fund account {}",
                journal_entry_id.as_str(),
                account_id.as_str()
            ),
            Self::TransferPrecedesAllocation {
                transfer_effective_at,
                allocation_effective_at,
            } => write!(
                formatter,
                "fund transfer effective time {transfer_effective_at} precedes canonical allocation time {allocation_effective_at}"
            ),
            Self::ObservationPrecedesTransfer {
                observed_at,
                effective_at,
            } => write!(
                formatter,
                "fund transfer observation time {observed_at} precedes effective time {effective_at}"
            ),
            Self::AllocationLineage(error) => write!(
                formatter,
                "canonical P26 allocation lineage identity failed: {error}"
            ),
            Self::Transfer(error) => {
                write!(formatter, "canonical P08 fund transfer failed: {error}")
            }
            Self::UnexpectedInternalReplay {
                source_event_id,
                journal_entry_id,
            } => write!(
                formatter,
                "canonical P08 replay for P27 event {} had no matching P27 history; journal {}",
                source_event_id.as_str(),
                journal_entry_id.as_str()
            ),
        }
    }
}

impl Error for AuthorizedFundTransferError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::AllocationLineage(error) => Some(error),
            Self::Transfer(error) => Some(error),
            _ => None,
        }
    }
}
