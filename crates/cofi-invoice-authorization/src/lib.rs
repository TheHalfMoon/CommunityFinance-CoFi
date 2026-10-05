use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_billing::{BillingEventId, BillingInvoiceId};
use cofi_invoicing::{
    DraftAssemblyOutcome, DraftInvoice, DraftInvoiceRegistry, DraftInvoiceRequest, InvoicingError,
};
use cofi_rating_authorization::AuthorizedRating;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedDraftRequest {
    event_id: BillingEventId,
    invoice_id: BillingInvoiceId,
    period_start_unix_ms: i64,
    period_end_unix_ms: i64,
    observed_at_unix_ms: i64,
    authorizations: Vec<AuthorizedRating>,
}

impl AuthorizedDraftRequest {
    pub fn new(
        event_id: BillingEventId,
        invoice_id: BillingInvoiceId,
        period_start_unix_ms: i64,
        period_end_unix_ms: i64,
        observed_at_unix_ms: i64,
        mut authorizations: Vec<AuthorizedRating>,
    ) -> Result<Self, AuthorizedDraftError> {
        if authorizations.is_empty() {
            return Err(AuthorizedDraftError::EmptyAuthorizationSet);
        }

        authorizations.sort_by(|left, right| left.charge().id().cmp(right.charge().id()));

        let first = &authorizations[0];
        let expected_scope = first.subscription().organization_scope();
        let expected_customer = first.subscription().billing_customer_id();
        for authorization in authorizations.iter().skip(1) {
            if authorization.subscription().organization_scope() != expected_scope {
                return Err(AuthorizedDraftError::MixedOrganizationScope);
            }
            if authorization.subscription().billing_customer_id() != expected_customer {
                return Err(AuthorizedDraftError::MixedBillingCustomer);
            }
        }

        Ok(Self {
            event_id,
            invoice_id,
            period_start_unix_ms,
            period_end_unix_ms,
            observed_at_unix_ms,
            authorizations,
        })
    }

    #[must_use]
    pub const fn event_id(&self) -> &BillingEventId {
        &self.event_id
    }

    #[must_use]
    pub const fn invoice_id(&self) -> &BillingInvoiceId {
        &self.invoice_id
    }

    #[must_use]
    pub const fn period_start_unix_ms(&self) -> i64 {
        self.period_start_unix_ms
    }

    #[must_use]
    pub const fn period_end_unix_ms(&self) -> i64 {
        self.period_end_unix_ms
    }

    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at_unix_ms
    }

    #[must_use]
    pub fn authorizations(&self) -> &[AuthorizedRating] {
        &self.authorizations
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedDraft {
    authorizations: Vec<AuthorizedRating>,
    draft: DraftInvoice,
}

impl AuthorizedDraft {
    #[must_use]
    pub fn authorizations(&self) -> &[AuthorizedRating] {
        &self.authorizations
    }

    #[must_use]
    pub const fn draft(&self) -> &DraftInvoice {
        &self.draft
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizedDraftOutcome {
    Created { authorization: AuthorizedDraft },
    Replayed { authorization: AuthorizedDraft },
}

impl AuthorizedDraftOutcome {
    #[must_use]
    pub const fn authorization(&self) -> &AuthorizedDraft {
        match self {
            Self::Created { authorization } | Self::Replayed { authorization } => authorization,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredAuthorizedDraft {
    request: AuthorizedDraftRequest,
    authorization: AuthorizedDraft,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorizedDraftRegistry {
    events: BTreeMap<BillingEventId, StoredAuthorizedDraft>,
    drafts: DraftInvoiceRegistry,
}

impl AuthorizedDraftRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn authorization_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn draft_event_count(&self) -> usize {
        self.drafts.event_count()
    }

    #[must_use]
    pub fn authorization_for_event(&self, event_id: &BillingEventId) -> Option<&AuthorizedDraft> {
        self.events
            .get(event_id)
            .map(|stored| &stored.authorization)
    }

    pub fn assemble(
        &mut self,
        request: AuthorizedDraftRequest,
    ) -> Result<AuthorizedDraftOutcome, AuthorizedDraftError> {
        if let Some(existing) = self.events.get(request.event_id()) {
            return if existing.request == request {
                Ok(AuthorizedDraftOutcome::Replayed {
                    authorization: existing.authorization.clone(),
                })
            } else {
                Err(AuthorizedDraftError::SourceEventConflict(
                    request.event_id().clone(),
                ))
            };
        }

        let first = request
            .authorizations
            .first()
            .ok_or(AuthorizedDraftError::EmptyAuthorizationSet)?;
        let organization_scope = first.subscription().organization_scope().clone();
        let billing_customer_id = first.subscription().billing_customer_id().clone();
        let charges = request
            .authorizations
            .iter()
            .map(|authorization| authorization.charge().clone())
            .collect();

        let canonical_request = DraftInvoiceRequest::new(
            request.event_id.clone(),
            request.invoice_id.clone(),
            billing_customer_id,
            organization_scope,
            request.period_start_unix_ms,
            request.period_end_unix_ms,
            request.observed_at_unix_ms,
            charges,
        )
        .map_err(AuthorizedDraftError::Invoicing)?;

        let draft = match self
            .drafts
            .assemble(canonical_request)
            .map_err(AuthorizedDraftError::Invoicing)?
        {
            DraftAssemblyOutcome::Created { draft } => draft,
            DraftAssemblyOutcome::Replayed { .. } => {
                return Err(AuthorizedDraftError::UnexpectedInternalReplay(
                    request.event_id.clone(),
                ));
            }
        };

        let authorization = AuthorizedDraft {
            authorizations: request.authorizations.clone(),
            draft,
        };
        self.events.insert(
            request.event_id.clone(),
            StoredAuthorizedDraft {
                request,
                authorization: authorization.clone(),
            },
        );

        Ok(AuthorizedDraftOutcome::Created { authorization })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizedDraftError {
    EmptyAuthorizationSet,
    MixedOrganizationScope,
    MixedBillingCustomer,
    SourceEventConflict(BillingEventId),
    Invoicing(InvoicingError),
    UnexpectedInternalReplay(BillingEventId),
}

impl Display for AuthorizedDraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyAuthorizationSet => {
                write!(
                    formatter,
                    "authorized draft requires at least one P21 authorization"
                )
            }
            Self::MixedOrganizationScope => write!(
                formatter,
                "authorized draft cannot mix P21 authorizations from different organization scopes"
            ),
            Self::MixedBillingCustomer => write!(
                formatter,
                "authorized draft cannot mix P21 authorizations from different billing customers"
            ),
            Self::SourceEventConflict(event_id) => write!(
                formatter,
                "authorized draft source event conflict: {}",
                event_id.as_str()
            ),
            Self::Invoicing(error) => write!(formatter, "canonical invoicing failed: {error}"),
            Self::UnexpectedInternalReplay(event_id) => write!(
                formatter,
                "internal draft history replayed without matching P22 authorization history for event {}",
                event_id.as_str()
            ),
        }
    }
}

impl Error for AuthorizedDraftError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Invoicing(error) => Some(error),
            Self::EmptyAuthorizationSet
            | Self::MixedOrganizationScope
            | Self::MixedBillingCustomer
            | Self::SourceEventConflict(_)
            | Self::UnexpectedInternalReplay(_) => None,
        }
    }
}
