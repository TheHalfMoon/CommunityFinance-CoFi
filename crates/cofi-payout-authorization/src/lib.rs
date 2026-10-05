use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_ledger::{JournalEntryId, Ledger};
use cofi_payment_authorization::AuthorizedCapture;
use cofi_payments::{
    BankTransactionReference, PayoutApplyOutcome, PayoutError, PayoutLedgerAccounts,
    PayoutLedgerBridge, ProcessorPayoutEvent, ProcessorPayoutEventId, ProcessorPayoutId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedPayoutRequest {
    source_event_id: ProcessorPayoutEventId,
    payout_id: ProcessorPayoutId,
    bank_transaction_reference: BankTransactionReference,
    authorized_capture: AuthorizedCapture,
    processor_fee_minor: i128,
    net_amount_minor: i128,
    paid_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

impl AuthorizedPayoutRequest {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        source_event_id: ProcessorPayoutEventId,
        payout_id: ProcessorPayoutId,
        bank_transaction_reference: BankTransactionReference,
        authorized_capture: AuthorizedCapture,
        processor_fee_minor: i128,
        net_amount_minor: i128,
        paid_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            payout_id,
            bank_transaction_reference,
            authorized_capture,
            processor_fee_minor,
            net_amount_minor,
            paid_at_unix_ms,
            observed_at_unix_ms,
        }
    }

    #[must_use]
    pub const fn source_event_id(&self) -> &ProcessorPayoutEventId {
        &self.source_event_id
    }

    #[must_use]
    pub const fn payout_id(&self) -> &ProcessorPayoutId {
        &self.payout_id
    }

    #[must_use]
    pub const fn bank_transaction_reference(&self) -> &BankTransactionReference {
        &self.bank_transaction_reference
    }

    #[must_use]
    pub const fn authorized_capture(&self) -> &AuthorizedCapture {
        &self.authorized_capture
    }

    #[must_use]
    pub const fn processor_fee_minor(&self) -> i128 {
        self.processor_fee_minor
    }

    #[must_use]
    pub const fn net_amount_minor(&self) -> i128 {
        self.net_amount_minor
    }

    #[must_use]
    pub const fn paid_at_unix_ms(&self) -> i64 {
        self.paid_at_unix_ms
    }

    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedPayout {
    authorized_capture: AuthorizedCapture,
    payout_event: ProcessorPayoutEvent,
    journal_entry_id: JournalEntryId,
}

impl AuthorizedPayout {
    #[must_use]
    pub const fn authorized_capture(&self) -> &AuthorizedCapture {
        &self.authorized_capture
    }

    #[must_use]
    pub const fn payout_event(&self) -> &ProcessorPayoutEvent {
        &self.payout_event
    }

    #[must_use]
    pub const fn journal_entry_id(&self) -> &JournalEntryId {
        &self.journal_entry_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizedPayoutOutcome {
    Created { authorization: AuthorizedPayout },
    Replayed { authorization: AuthorizedPayout },
}

impl AuthorizedPayoutOutcome {
    #[must_use]
    pub const fn authorization(&self) -> &AuthorizedPayout {
        match self {
            Self::Created { authorization } | Self::Replayed { authorization } => authorization,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredAuthorizedPayout {
    request: AuthorizedPayoutRequest,
    authorization: AuthorizedPayout,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorizedPayoutRegistry {
    events: BTreeMap<ProcessorPayoutEventId, StoredAuthorizedPayout>,
}

impl AuthorizedPayoutRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn authorization_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn authorization_for_event(
        &self,
        event_id: &ProcessorPayoutEventId,
    ) -> Option<&AuthorizedPayout> {
        self.events
            .get(event_id)
            .map(|stored| &stored.authorization)
    }

    pub fn apply(
        &mut self,
        request: AuthorizedPayoutRequest,
        accounts: &PayoutLedgerAccounts,
        ledger: &mut Ledger,
    ) -> Result<AuthorizedPayoutOutcome, AuthorizedPayoutError> {
        if let Some(existing) = self.events.get(request.source_event_id()) {
            return if existing.request == request {
                Ok(AuthorizedPayoutOutcome::Replayed {
                    authorization: existing.authorization.clone(),
                })
            } else {
                Err(AuthorizedPayoutError::SourceEventConflict(
                    request.source_event_id().clone(),
                ))
            };
        }

        let payment_event = request.authorized_capture.payment_event();
        let payout_event = ProcessorPayoutEvent::new(
            request.source_event_id.clone(),
            payment_event.organization_scope().clone(),
            payment_event.payment_id().clone(),
            request.payout_id.clone(),
            request.bank_transaction_reference.clone(),
            payment_event.currency(),
            payment_event.amount_captured_minor(),
            request.processor_fee_minor,
            request.net_amount_minor,
            request.paid_at_unix_ms,
            request.observed_at_unix_ms,
        );

        let journal_entry_id = match PayoutLedgerBridge::new()
            .apply(&payout_event, accounts, ledger)
            .map_err(AuthorizedPayoutError::Payout)?
        {
            PayoutApplyOutcome::Committed { journal_entry_id } => journal_entry_id,
            PayoutApplyOutcome::Replayed { journal_entry_id } => {
                return Err(AuthorizedPayoutError::UnexpectedInternalReplay {
                    source_event_id: request.source_event_id.clone(),
                    journal_entry_id,
                });
            }
        };

        let authorization = AuthorizedPayout {
            authorized_capture: request.authorized_capture.clone(),
            payout_event,
            journal_entry_id,
        };
        self.events.insert(
            request.source_event_id.clone(),
            StoredAuthorizedPayout {
                request,
                authorization: authorization.clone(),
            },
        );

        Ok(AuthorizedPayoutOutcome::Created { authorization })
    }
}

#[derive(Debug)]
pub enum AuthorizedPayoutError {
    SourceEventConflict(ProcessorPayoutEventId),
    Payout(PayoutError),
    UnexpectedInternalReplay {
        source_event_id: ProcessorPayoutEventId,
        journal_entry_id: JournalEntryId,
    },
}

impl Display for AuthorizedPayoutError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceEventConflict(event_id) => write!(
                formatter,
                "authorized payout source event conflict: {}",
                event_id.as_str()
            ),
            Self::Payout(error) => {
                write!(formatter, "canonical payout application failed: {error}")
            }
            Self::UnexpectedInternalReplay {
                source_event_id,
                journal_entry_id,
            } => write!(
                formatter,
                "canonical payout history replayed without matching P25 authorization history for event {} and journal entry {}",
                source_event_id.as_str(),
                journal_entry_id.as_str()
            ),
        }
    }
}

impl Error for AuthorizedPayoutError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Payout(error) => Some(error),
            Self::SourceEventConflict(_) | Self::UnexpectedInternalReplay { .. } => None,
        }
    }
}
