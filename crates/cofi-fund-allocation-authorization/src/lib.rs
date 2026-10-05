use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_community::{
    CommunityRegistry, FundAllocationAccounts, FundAllocationBridge, FundAllocationError,
    FundAllocationEvent, FundAllocationEventId, FundAllocationId, FundAllocationOutcome, FundId,
};
use cofi_ledger::{AccountId, AccountKind, Currency, JournalEntryId, Ledger, LedgerScopeId};
use cofi_payout_authorization::AuthorizedPayout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedFundAllocationRequest {
    source_event_id: FundAllocationEventId,
    allocation_id: FundAllocationId,
    fund_id: FundId,
    authorized_payout: AuthorizedPayout,
    effective_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

impl AuthorizedFundAllocationRequest {
    #[must_use]
    pub const fn new(
        source_event_id: FundAllocationEventId,
        allocation_id: FundAllocationId,
        fund_id: FundId,
        authorized_payout: AuthorizedPayout,
        effective_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            allocation_id,
            fund_id,
            authorized_payout,
            effective_at_unix_ms,
            observed_at_unix_ms,
        }
    }

    #[must_use]
    pub const fn source_event_id(&self) -> &FundAllocationEventId {
        &self.source_event_id
    }

    #[must_use]
    pub const fn allocation_id(&self) -> &FundAllocationId {
        &self.allocation_id
    }

    #[must_use]
    pub const fn fund_id(&self) -> &FundId {
        &self.fund_id
    }

    #[must_use]
    pub const fn authorized_payout(&self) -> &AuthorizedPayout {
        &self.authorized_payout
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
pub struct AuthorizedFundAllocation {
    authorized_payout: AuthorizedPayout,
    allocation_event: FundAllocationEvent,
    source_cash_account_id: AccountId,
    journal_entry_id: JournalEntryId,
}

impl AuthorizedFundAllocation {
    #[must_use]
    pub const fn authorized_payout(&self) -> &AuthorizedPayout {
        &self.authorized_payout
    }

    #[must_use]
    pub const fn allocation_event(&self) -> &FundAllocationEvent {
        &self.allocation_event
    }

    #[must_use]
    pub const fn source_cash_account_id(&self) -> &AccountId {
        &self.source_cash_account_id
    }

    #[must_use]
    pub const fn journal_entry_id(&self) -> &JournalEntryId {
        &self.journal_entry_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizedFundAllocationOutcome {
    Created {
        authorization: AuthorizedFundAllocation,
    },
    Replayed {
        authorization: AuthorizedFundAllocation,
    },
}

impl AuthorizedFundAllocationOutcome {
    #[must_use]
    pub const fn authorization(&self) -> &AuthorizedFundAllocation {
        match self {
            Self::Created { authorization } | Self::Replayed { authorization } => authorization,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredAuthorizedFundAllocation {
    request: AuthorizedFundAllocationRequest,
    authorization: AuthorizedFundAllocation,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorizedFundAllocationRegistry {
    events: BTreeMap<FundAllocationEventId, StoredAuthorizedFundAllocation>,
    payout_allocations: BTreeMap<JournalEntryId, FundAllocationEventId>,
}

impl AuthorizedFundAllocationRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn authorization_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn payout_allocation_count(&self) -> usize {
        self.payout_allocations.len()
    }

    #[must_use]
    pub fn authorization_for_event(
        &self,
        event_id: &FundAllocationEventId,
    ) -> Option<&AuthorizedFundAllocation> {
        self.events
            .get(event_id)
            .map(|stored| &stored.authorization)
    }

    pub fn apply(
        &mut self,
        community_registry: &CommunityRegistry,
        request: AuthorizedFundAllocationRequest,
        ledger: &mut Ledger,
    ) -> Result<AuthorizedFundAllocationOutcome, AuthorizedFundAllocationError> {
        if let Some(existing) = self.events.get(request.source_event_id()) {
            return if existing.request == request {
                Ok(AuthorizedFundAllocationOutcome::Replayed {
                    authorization: existing.authorization.clone(),
                })
            } else {
                Err(AuthorizedFundAllocationError::SourceEventConflict(
                    request.source_event_id().clone(),
                ))
            };
        }

        let payout_journal_entry_id = request.authorized_payout.journal_entry_id().clone();
        if let Some(existing_event_id) = self.payout_allocations.get(&payout_journal_entry_id) {
            return Err(AuthorizedFundAllocationError::PayoutAlreadyAllocated {
                payout_journal_entry_id,
                existing_event_id: existing_event_id.clone(),
            });
        }

        let payout_event = request.authorized_payout.payout_event();
        let payout_scope = payout_event.organization_scope().clone();
        let payout_currency = payout_event.currency();
        let (source_cash_account_id, amount_minor, payout_effective_at_unix_ms) =
            derive_canonical_source_cash(
                ledger,
                &payout_journal_entry_id,
                &payout_scope,
                payout_currency,
            )?;

        if request.effective_at_unix_ms < payout_effective_at_unix_ms {
            return Err(AuthorizedFundAllocationError::AllocationPrecedesPayout {
                allocation_effective_at: request.effective_at_unix_ms,
                payout_effective_at: payout_effective_at_unix_ms,
            });
        }
        if request.observed_at_unix_ms < request.effective_at_unix_ms {
            return Err(
                AuthorizedFundAllocationError::ObservationPrecedesAllocation {
                    observed_at: request.observed_at_unix_ms,
                    effective_at: request.effective_at_unix_ms,
                },
            );
        }

        let allocation_event = FundAllocationEvent::new(
            request.source_event_id.clone(),
            payout_scope,
            request.fund_id.clone(),
            request.allocation_id.clone(),
            payout_currency,
            amount_minor,
            request.effective_at_unix_ms,
            request.observed_at_unix_ms,
        );
        let allocation_accounts = FundAllocationAccounts::new(source_cash_account_id.clone());

        let journal_entry_id = match FundAllocationBridge::new()
            .apply(
                community_registry,
                &allocation_event,
                &allocation_accounts,
                ledger,
            )
            .map_err(AuthorizedFundAllocationError::Allocation)?
        {
            FundAllocationOutcome::Committed { journal_entry_id } => journal_entry_id,
            FundAllocationOutcome::Replayed { journal_entry_id } => {
                return Err(AuthorizedFundAllocationError::UnexpectedInternalReplay {
                    source_event_id: request.source_event_id.clone(),
                    journal_entry_id,
                });
            }
        };

        let authorization = AuthorizedFundAllocation {
            authorized_payout: request.authorized_payout.clone(),
            allocation_event,
            source_cash_account_id,
            journal_entry_id,
        };
        self.payout_allocations
            .insert(payout_journal_entry_id, request.source_event_id.clone());
        self.events.insert(
            request.source_event_id.clone(),
            StoredAuthorizedFundAllocation {
                request,
                authorization: authorization.clone(),
            },
        );

        Ok(AuthorizedFundAllocationOutcome::Created { authorization })
    }
}

fn derive_canonical_source_cash(
    ledger: &Ledger,
    payout_journal_entry_id: &JournalEntryId,
    expected_scope: &LedgerScopeId,
    expected_currency: Currency,
) -> Result<(AccountId, i128, i64), AuthorizedFundAllocationError> {
    let payout_entry = ledger.entry(payout_journal_entry_id).ok_or_else(|| {
        AuthorizedFundAllocationError::MissingCanonicalPayoutJournal(
            payout_journal_entry_id.clone(),
        )
    })?;

    let mut source_cash: Option<(AccountId, u128)> = None;
    for posting in payout_entry.postings() {
        if posting.currency() != expected_currency {
            return Err(
                AuthorizedFundAllocationError::CanonicalPayoutPostingCurrencyMismatch {
                    account_id: posting.account_id().clone(),
                    expected: expected_currency,
                    actual: posting.currency(),
                },
            );
        }

        let account = ledger.account(posting.account_id()).ok_or_else(|| {
            AuthorizedFundAllocationError::CanonicalPayoutUnknownAccount {
                account_id: posting.account_id().clone(),
            }
        })?;
        if account.scope_id() != expected_scope {
            return Err(
                AuthorizedFundAllocationError::CanonicalPayoutAccountScopeMismatch {
                    account_id: account.id().clone(),
                    expected: expected_scope.clone(),
                    actual: account.scope_id().clone(),
                },
            );
        }
        if account.currency() != expected_currency {
            return Err(
                AuthorizedFundAllocationError::CanonicalPayoutAccountCurrencyMismatch {
                    account_id: account.id().clone(),
                    expected: expected_currency,
                    actual: account.currency(),
                },
            );
        }

        if account.kind() == AccountKind::Asset && posting.side() == account.kind().normal_side() {
            if source_cash.is_some() {
                return Err(
                    AuthorizedFundAllocationError::AmbiguousCanonicalSourceCashDebit(
                        payout_journal_entry_id.clone(),
                    ),
                );
            }
            source_cash = Some((account.id().clone(), posting.amount().value()));
        }
    }

    let (source_cash_account_id, amount) = source_cash.ok_or_else(|| {
        AuthorizedFundAllocationError::MissingCanonicalSourceCashDebit(
            payout_journal_entry_id.clone(),
        )
    })?;
    let amount_minor = i128::try_from(amount).map_err(|_| {
        AuthorizedFundAllocationError::CanonicalSourceCashAmountOutOfRange {
            payout_journal_entry_id: payout_journal_entry_id.clone(),
            amount,
        }
    })?;

    Ok((
        source_cash_account_id,
        amount_minor,
        payout_entry.effective_at_unix_ms(),
    ))
}

#[derive(Debug)]
pub enum AuthorizedFundAllocationError {
    SourceEventConflict(FundAllocationEventId),
    PayoutAlreadyAllocated {
        payout_journal_entry_id: JournalEntryId,
        existing_event_id: FundAllocationEventId,
    },
    MissingCanonicalPayoutJournal(JournalEntryId),
    CanonicalPayoutUnknownAccount {
        account_id: AccountId,
    },
    CanonicalPayoutPostingCurrencyMismatch {
        account_id: AccountId,
        expected: Currency,
        actual: Currency,
    },
    CanonicalPayoutAccountScopeMismatch {
        account_id: AccountId,
        expected: LedgerScopeId,
        actual: LedgerScopeId,
    },
    CanonicalPayoutAccountCurrencyMismatch {
        account_id: AccountId,
        expected: Currency,
        actual: Currency,
    },
    MissingCanonicalSourceCashDebit(JournalEntryId),
    AmbiguousCanonicalSourceCashDebit(JournalEntryId),
    CanonicalSourceCashAmountOutOfRange {
        payout_journal_entry_id: JournalEntryId,
        amount: u128,
    },
    AllocationPrecedesPayout {
        allocation_effective_at: i64,
        payout_effective_at: i64,
    },
    ObservationPrecedesAllocation {
        observed_at: i64,
        effective_at: i64,
    },
    Allocation(FundAllocationError),
    UnexpectedInternalReplay {
        source_event_id: FundAllocationEventId,
        journal_entry_id: JournalEntryId,
    },
}

impl Display for AuthorizedFundAllocationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceEventConflict(event_id) => write!(
                formatter,
                "authorized fund-allocation source event conflict: {}",
                event_id.as_str()
            ),
            Self::PayoutAlreadyAllocated {
                payout_journal_entry_id,
                existing_event_id,
            } => write!(
                formatter,
                "canonical payout journal {} already has P26 allocation event {}",
                payout_journal_entry_id.as_str(),
                existing_event_id.as_str()
            ),
            Self::MissingCanonicalPayoutJournal(entry_id) => write!(
                formatter,
                "missing canonical P25 payout journal entry: {}",
                entry_id.as_str()
            ),
            Self::CanonicalPayoutUnknownAccount { account_id } => write!(
                formatter,
                "canonical P25 payout journal references unknown account: {}",
                account_id.as_str()
            ),
            Self::CanonicalPayoutPostingCurrencyMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                formatter,
                "canonical P25 payout posting for account {} uses currency {actual}; expected {expected}",
                account_id.as_str()
            ),
            Self::CanonicalPayoutAccountScopeMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                formatter,
                "canonical P25 payout account {} has scope {}; expected {}",
                account_id.as_str(),
                actual.as_str(),
                expected.as_str()
            ),
            Self::CanonicalPayoutAccountCurrencyMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                formatter,
                "canonical P25 payout account {} has currency {actual}; expected {expected}",
                account_id.as_str()
            ),
            Self::MissingCanonicalSourceCashDebit(entry_id) => write!(
                formatter,
                "canonical P25 payout journal {} has no unique debit-normal Asset source cash posting",
                entry_id.as_str()
            ),
            Self::AmbiguousCanonicalSourceCashDebit(entry_id) => write!(
                formatter,
                "canonical P25 payout journal {} has multiple debit-normal Asset source cash postings",
                entry_id.as_str()
            ),
            Self::CanonicalSourceCashAmountOutOfRange {
                payout_journal_entry_id,
                amount,
            } => write!(
                formatter,
                "canonical P25 payout journal {} source cash amount {amount} exceeds P07 input range",
                payout_journal_entry_id.as_str()
            ),
            Self::AllocationPrecedesPayout {
                allocation_effective_at,
                payout_effective_at,
            } => write!(
                formatter,
                "fund allocation effective time {allocation_effective_at} precedes canonical payout time {payout_effective_at}"
            ),
            Self::ObservationPrecedesAllocation {
                observed_at,
                effective_at,
            } => write!(
                formatter,
                "fund allocation observation time {observed_at} precedes effective time {effective_at}"
            ),
            Self::Allocation(error) => {
                write!(formatter, "canonical P07 fund allocation failed: {error}")
            }
            Self::UnexpectedInternalReplay {
                source_event_id,
                journal_entry_id,
            } => write!(
                formatter,
                "canonical P07 allocation history replayed without matching P26 authorization history for event {} and journal entry {}",
                source_event_id.as_str(),
                journal_entry_id.as_str()
            ),
        }
    }
}

impl Error for AuthorizedFundAllocationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}
