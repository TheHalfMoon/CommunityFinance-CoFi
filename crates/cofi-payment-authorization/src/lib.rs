use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_finalization_authorization::AuthorizedFinalization;
use cofi_ledger::{JournalEntryId, Ledger};
use cofi_payments::{
    ConnectorTransactionId, PaymentApplyOutcome, PaymentError, PaymentEvent, PaymentEventId,
    PaymentId, PaymentLedgerAccounts, PaymentLedgerBridge, PaymentStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedCaptureRequest {
    source_event_id: PaymentEventId,
    payment_id: PaymentId,
    connector_transaction_id: ConnectorTransactionId,
    authorized_finalization: AuthorizedFinalization,
    captured_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

impl AuthorizedCaptureRequest {
    #[must_use]
    pub const fn new(
        source_event_id: PaymentEventId,
        payment_id: PaymentId,
        connector_transaction_id: ConnectorTransactionId,
        authorized_finalization: AuthorizedFinalization,
        captured_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            payment_id,
            connector_transaction_id,
            authorized_finalization,
            captured_at_unix_ms,
            observed_at_unix_ms,
        }
    }

    #[must_use]
    pub const fn source_event_id(&self) -> &PaymentEventId {
        &self.source_event_id
    }

    #[must_use]
    pub const fn payment_id(&self) -> &PaymentId {
        &self.payment_id
    }

    #[must_use]
    pub const fn connector_transaction_id(&self) -> &ConnectorTransactionId {
        &self.connector_transaction_id
    }

    #[must_use]
    pub const fn authorized_finalization(&self) -> &AuthorizedFinalization {
        &self.authorized_finalization
    }

    #[must_use]
    pub const fn captured_at_unix_ms(&self) -> i64 {
        self.captured_at_unix_ms
    }

    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedCapture {
    authorized_finalization: AuthorizedFinalization,
    payment_event: PaymentEvent,
    journal_entry_id: JournalEntryId,
}

impl AuthorizedCapture {
    #[must_use]
    pub const fn authorized_finalization(&self) -> &AuthorizedFinalization {
        &self.authorized_finalization
    }

    #[must_use]
    pub const fn payment_event(&self) -> &PaymentEvent {
        &self.payment_event
    }

    #[must_use]
    pub const fn journal_entry_id(&self) -> &JournalEntryId {
        &self.journal_entry_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizedCaptureOutcome {
    Created { authorization: AuthorizedCapture },
    Replayed { authorization: AuthorizedCapture },
}

impl AuthorizedCaptureOutcome {
    #[must_use]
    pub const fn authorization(&self) -> &AuthorizedCapture {
        match self {
            Self::Created { authorization } | Self::Replayed { authorization } => authorization,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredAuthorizedCapture {
    request: AuthorizedCaptureRequest,
    authorization: AuthorizedCapture,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorizedCaptureRegistry {
    events: BTreeMap<PaymentEventId, StoredAuthorizedCapture>,
}

impl AuthorizedCaptureRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn authorization_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn authorization_for_event(&self, event_id: &PaymentEventId) -> Option<&AuthorizedCapture> {
        self.events
            .get(event_id)
            .map(|stored| &stored.authorization)
    }

    pub fn capture(
        &mut self,
        request: AuthorizedCaptureRequest,
        accounts: &PaymentLedgerAccounts,
        ledger: &mut Ledger,
    ) -> Result<AuthorizedCaptureOutcome, AuthorizedCaptureError> {
        if let Some(existing) = self.events.get(request.source_event_id()) {
            return if existing.request == request {
                Ok(AuthorizedCaptureOutcome::Replayed {
                    authorization: existing.authorization.clone(),
                })
            } else {
                Err(AuthorizedCaptureError::SourceEventConflict(
                    request.source_event_id().clone(),
                ))
            };
        }

        let draft = request.authorized_finalization.finalized().draft();
        let payment_event = PaymentEvent::new(
            request.source_event_id.clone(),
            draft.organization_scope().clone(),
            draft.invoice_id().clone(),
            request.payment_id.clone(),
            request.connector_transaction_id.clone(),
            PaymentStatus::Charged,
            draft.currency(),
            draft.total_minor(),
            Some(request.captured_at_unix_ms),
            request.observed_at_unix_ms,
        );

        let journal_entry_id = match PaymentLedgerBridge::new()
            .apply(&payment_event, accounts, ledger)
            .map_err(AuthorizedCaptureError::Payment)?
        {
            PaymentApplyOutcome::Committed { journal_entry_id } => journal_entry_id,
            PaymentApplyOutcome::Replayed { journal_entry_id } => {
                return Err(AuthorizedCaptureError::UnexpectedInternalReplay {
                    source_event_id: request.source_event_id.clone(),
                    journal_entry_id,
                });
            }
            PaymentApplyOutcome::IgnoredNoCapture { status } => {
                return Err(AuthorizedCaptureError::UnexpectedNoCapture(status));
            }
        };

        let authorization = AuthorizedCapture {
            authorized_finalization: request.authorized_finalization.clone(),
            payment_event,
            journal_entry_id,
        };
        self.events.insert(
            request.source_event_id.clone(),
            StoredAuthorizedCapture {
                request,
                authorization: authorization.clone(),
            },
        );

        Ok(AuthorizedCaptureOutcome::Created { authorization })
    }
}

#[derive(Debug)]
pub enum AuthorizedCaptureError {
    SourceEventConflict(PaymentEventId),
    Payment(PaymentError),
    UnexpectedInternalReplay {
        source_event_id: PaymentEventId,
        journal_entry_id: JournalEntryId,
    },
    UnexpectedNoCapture(PaymentStatus),
}

impl Display for AuthorizedCaptureError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceEventConflict(event_id) => write!(
                formatter,
                "authorized capture source event conflict: {}",
                event_id.as_str()
            ),
            Self::Payment(error) => {
                write!(formatter, "canonical payment application failed: {error}")
            }
            Self::UnexpectedInternalReplay {
                source_event_id,
                journal_entry_id,
            } => write!(
                formatter,
                "canonical payment history replayed without matching P24 authorization history for event {} and journal entry {}",
                source_event_id.as_str(),
                journal_entry_id.as_str()
            ),
            Self::UnexpectedNoCapture(status) => write!(
                formatter,
                "P24 constructed a charged payment but canonical P05 returned no-capture status {status:?}"
            ),
        }
    }
}

impl Error for AuthorizedCaptureError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Payment(error) => Some(error),
            Self::SourceEventConflict(_)
            | Self::UnexpectedInternalReplay { .. }
            | Self::UnexpectedNoCapture(_) => None,
        }
    }
}
