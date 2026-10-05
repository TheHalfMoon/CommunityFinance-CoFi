use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_billing::BillingEventId;
use cofi_invoice_authorization::AuthorizedDraft;
use cofi_invoicing::{
    FinalizationError, FinalizationOutcome, FinalizationRegistry, FinalizationRequest,
    FinalizedInvoice,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedFinalizationRequest {
    source_event_id: BillingEventId,
    authorized_draft: AuthorizedDraft,
    finalized_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

impl AuthorizedFinalizationRequest {
    #[must_use]
    pub const fn new(
        source_event_id: BillingEventId,
        authorized_draft: AuthorizedDraft,
        finalized_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            authorized_draft,
            finalized_at_unix_ms,
            observed_at_unix_ms,
        }
    }

    #[must_use]
    pub const fn source_event_id(&self) -> &BillingEventId {
        &self.source_event_id
    }

    #[must_use]
    pub const fn authorized_draft(&self) -> &AuthorizedDraft {
        &self.authorized_draft
    }

    #[must_use]
    pub const fn finalized_at_unix_ms(&self) -> i64 {
        self.finalized_at_unix_ms
    }

    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedFinalization {
    authorized_draft: AuthorizedDraft,
    finalized: FinalizedInvoice,
}

impl AuthorizedFinalization {
    #[must_use]
    pub const fn authorized_draft(&self) -> &AuthorizedDraft {
        &self.authorized_draft
    }

    #[must_use]
    pub const fn finalized(&self) -> &FinalizedInvoice {
        &self.finalized
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizedFinalizationOutcome {
    Created {
        authorization: AuthorizedFinalization,
    },
    Replayed {
        authorization: AuthorizedFinalization,
    },
}

impl AuthorizedFinalizationOutcome {
    #[must_use]
    pub const fn authorization(&self) -> &AuthorizedFinalization {
        match self {
            Self::Created { authorization } | Self::Replayed { authorization } => authorization,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredAuthorizedFinalization {
    request: AuthorizedFinalizationRequest,
    authorization: AuthorizedFinalization,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorizedFinalizationRegistry {
    events: BTreeMap<BillingEventId, StoredAuthorizedFinalization>,
    finalizations: FinalizationRegistry,
}

impl AuthorizedFinalizationRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn authorization_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn finalization_event_count(&self) -> usize {
        self.finalizations.event_count()
    }

    #[must_use]
    pub fn authorization_for_event(
        &self,
        event_id: &BillingEventId,
    ) -> Option<&AuthorizedFinalization> {
        self.events
            .get(event_id)
            .map(|stored| &stored.authorization)
    }

    pub fn finalize(
        &mut self,
        request: AuthorizedFinalizationRequest,
    ) -> Result<AuthorizedFinalizationOutcome, AuthorizedFinalizationError> {
        if let Some(existing) = self.events.get(request.source_event_id()) {
            return if existing.request == request {
                Ok(AuthorizedFinalizationOutcome::Replayed {
                    authorization: existing.authorization.clone(),
                })
            } else {
                Err(AuthorizedFinalizationError::SourceEventConflict(
                    request.source_event_id().clone(),
                ))
            };
        }

        let canonical_request = FinalizationRequest::new(
            request.source_event_id.clone(),
            request.authorized_draft.draft().clone(),
            request.finalized_at_unix_ms,
            request.observed_at_unix_ms,
        )
        .map_err(AuthorizedFinalizationError::Finalization)?;

        let finalized = match self
            .finalizations
            .finalize(canonical_request)
            .map_err(AuthorizedFinalizationError::Finalization)?
        {
            FinalizationOutcome::Created { finalized } => finalized,
            FinalizationOutcome::Replayed { .. } => {
                return Err(AuthorizedFinalizationError::UnexpectedInternalReplay(
                    request.source_event_id.clone(),
                ));
            }
        };

        let authorization = AuthorizedFinalization {
            authorized_draft: request.authorized_draft.clone(),
            finalized,
        };
        self.events.insert(
            request.source_event_id.clone(),
            StoredAuthorizedFinalization {
                request,
                authorization: authorization.clone(),
            },
        );

        Ok(AuthorizedFinalizationOutcome::Created { authorization })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizedFinalizationError {
    SourceEventConflict(BillingEventId),
    Finalization(FinalizationError),
    UnexpectedInternalReplay(BillingEventId),
}

impl Display for AuthorizedFinalizationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceEventConflict(event_id) => write!(
                formatter,
                "authorized finalization source event conflict: {}",
                event_id.as_str()
            ),
            Self::Finalization(error) => {
                write!(formatter, "canonical finalization failed: {error}")
            }
            Self::UnexpectedInternalReplay(event_id) => write!(
                formatter,
                "internal finalization history replayed without matching P23 authorization history for event {}",
                event_id.as_str()
            ),
        }
    }
}

impl Error for AuthorizedFinalizationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Finalization(error) => Some(error),
            Self::SourceEventConflict(_) | Self::UnexpectedInternalReplay(_) => None,
        }
    }
}
