use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_billing::BillingCustomerId;
use cofi_ledger::LedgerScopeId;
use cofi_metering::{MeterId, SubjectId, UsageAggregate};
use cofi_rating::{
    RatedCharge, RatingError, RatingEventId, RatingOutcome, RatingRegistry, RatingRequest,
};
use cofi_subscriptions::{Subscription, SubscriptionError, SubscriptionRegistry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedRatingRequest {
    event_id: RatingEventId,
    organization_scope: LedgerScopeId,
    billing_customer_id: BillingCustomerId,
    aggregate: UsageAggregate,
    rated_at_unix_ms: i64,
}

impl AuthorizedRatingRequest {
    #[must_use]
    pub fn new(
        event_id: RatingEventId,
        organization_scope: LedgerScopeId,
        billing_customer_id: BillingCustomerId,
        aggregate: UsageAggregate,
        rated_at_unix_ms: i64,
    ) -> Self {
        Self {
            event_id,
            organization_scope,
            billing_customer_id,
            aggregate,
            rated_at_unix_ms,
        }
    }

    #[must_use]
    pub const fn event_id(&self) -> &RatingEventId {
        &self.event_id
    }

    #[must_use]
    pub const fn organization_scope(&self) -> &LedgerScopeId {
        &self.organization_scope
    }

    #[must_use]
    pub const fn billing_customer_id(&self) -> &BillingCustomerId {
        &self.billing_customer_id
    }

    #[must_use]
    pub const fn aggregate(&self) -> &UsageAggregate {
        &self.aggregate
    }

    #[must_use]
    pub const fn rated_at_unix_ms(&self) -> i64 {
        self.rated_at_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedRating {
    subscription: Subscription,
    charge: RatedCharge,
}

impl AuthorizedRating {
    #[must_use]
    pub const fn subscription(&self) -> &Subscription {
        &self.subscription
    }

    #[must_use]
    pub const fn charge(&self) -> &RatedCharge {
        &self.charge
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizedRatingOutcome {
    Rated { authorization: AuthorizedRating },
    Replayed { authorization: AuthorizedRating },
}

impl AuthorizedRatingOutcome {
    #[must_use]
    pub const fn authorization(&self) -> &AuthorizedRating {
        match self {
            Self::Rated { authorization } | Self::Replayed { authorization } => authorization,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredAuthorization {
    request: AuthorizedRatingRequest,
    authorization: AuthorizedRating,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorizedRatingRegistry {
    events: BTreeMap<RatingEventId, StoredAuthorization>,
    ratings: RatingRegistry,
}

impl AuthorizedRatingRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn authorization_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn rating_event_count(&self) -> usize {
        self.ratings.event_count()
    }

    #[must_use]
    pub fn authorization_for_event(&self, event_id: &RatingEventId) -> Option<&AuthorizedRating> {
        self.events
            .get(event_id)
            .map(|stored| &stored.authorization)
    }

    #[must_use]
    pub fn charge_for_event(&self, event_id: &RatingEventId) -> Option<&RatedCharge> {
        self.ratings.charge_for_event(event_id)
    }

    pub fn rate(
        &mut self,
        request: AuthorizedRatingRequest,
        subscriptions: &SubscriptionRegistry,
    ) -> Result<AuthorizedRatingOutcome, AuthorizedRatingError> {
        if let Some(existing) = self.events.get(request.event_id()) {
            return if existing.request == request {
                Ok(AuthorizedRatingOutcome::Replayed {
                    authorization: existing.authorization.clone(),
                })
            } else {
                Err(AuthorizedRatingError::RatingEventConflict(
                    request.event_id().clone(),
                ))
            };
        }

        let aggregate = request.aggregate();
        let subscription = subscriptions
            .resolve_for_window(
                request.organization_scope(),
                request.billing_customer_id(),
                aggregate.subject_id(),
                aggregate.meter_id(),
                aggregate.window_start_unix_ms(),
                aggregate.window_end_unix_ms(),
            )
            .map_err(AuthorizedRatingError::Subscription)?
            .ok_or_else(|| AuthorizedRatingError::NoCanonicalSubscription {
                organization_scope: request.organization_scope().clone(),
                billing_customer_id: request.billing_customer_id().clone(),
                subject_id: aggregate.subject_id().clone(),
                meter_id: aggregate.meter_id().clone(),
                window_start_unix_ms: aggregate.window_start_unix_ms(),
                window_end_unix_ms: aggregate.window_end_unix_ms(),
            })?
            .clone();

        let canonical_request = RatingRequest::new(
            request.event_id().clone(),
            aggregate.clone(),
            request.billing_customer_id().clone(),
            subscription.plan().clone(),
            request.rated_at_unix_ms(),
        )
        .map_err(AuthorizedRatingError::Rating)?;

        let charge = match self
            .ratings
            .rate(canonical_request)
            .map_err(AuthorizedRatingError::Rating)?
        {
            RatingOutcome::Rated { charge } => charge,
            RatingOutcome::Replayed { .. } => {
                return Err(AuthorizedRatingError::UnexpectedInternalReplay(
                    request.event_id().clone(),
                ));
            }
        };

        let authorization = AuthorizedRating {
            subscription,
            charge,
        };
        self.events.insert(
            request.event_id().clone(),
            StoredAuthorization {
                request,
                authorization: authorization.clone(),
            },
        );
        Ok(AuthorizedRatingOutcome::Rated { authorization })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizedRatingError {
    NoCanonicalSubscription {
        organization_scope: LedgerScopeId,
        billing_customer_id: BillingCustomerId,
        subject_id: SubjectId,
        meter_id: MeterId,
        window_start_unix_ms: i64,
        window_end_unix_ms: i64,
    },
    RatingEventConflict(RatingEventId),
    Subscription(SubscriptionError),
    Rating(RatingError),
    UnexpectedInternalReplay(RatingEventId),
}

impl Display for AuthorizedRatingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCanonicalSubscription {
                organization_scope,
                billing_customer_id,
                subject_id,
                meter_id,
                window_start_unix_ms,
                window_end_unix_ms,
            } => write!(
                formatter,
                "no canonical subscription covers scope {}, customer {}, subject {}, meter {}, window [{window_start_unix_ms}, {window_end_unix_ms})",
                organization_scope.as_str(),
                billing_customer_id.as_str(),
                subject_id.as_str(),
                meter_id.as_str(),
            ),
            Self::RatingEventConflict(event_id) => write!(
                formatter,
                "authorized rating event conflict: {}",
                event_id.as_str()
            ),
            Self::Subscription(error) => {
                write!(formatter, "subscription resolution failed: {error}")
            }
            Self::Rating(error) => write!(formatter, "canonical rating failed: {error}"),
            Self::UnexpectedInternalReplay(event_id) => write!(
                formatter,
                "internal rating history replayed without matching authorization history for event {}",
                event_id.as_str()
            ),
        }
    }
}

impl Error for AuthorizedRatingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Subscription(error) => Some(error),
            Self::Rating(error) => Some(error),
            Self::NoCanonicalSubscription { .. }
            | Self::RatingEventConflict(_)
            | Self::UnexpectedInternalReplay(_) => None,
        }
    }
}
