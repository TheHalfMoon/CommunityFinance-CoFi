use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_billing::BillingCustomerId;
use cofi_ledger::LedgerScopeId;
use cofi_metering::{MeterId, SubjectId};
use cofi_rating::RatePlan;

macro_rules! subscription_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, SubscriptionError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(SubscriptionError::EmptyIdentifier($label));
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

subscription_id!(SubscriptionId, "subscription_id");
subscription_id!(SubscriptionEventId, "subscription_event_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionStatus {
    Scheduled,
    Active,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ScheduleKey {
    scope: String,
    customer: String,
    subject: String,
    meter: String,
}

impl ScheduleKey {
    fn new(
        scope: &LedgerScopeId,
        customer: &BillingCustomerId,
        subject: &SubjectId,
        meter: &MeterId,
    ) -> Self {
        Self {
            scope: scope.as_str().to_owned(),
            customer: customer.as_str().to_owned(),
            subject: subject.as_str().to_owned(),
            meter: meter.as_str().to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionRequest {
    event_id: SubscriptionEventId,
    subscription_id: SubscriptionId,
    organization_scope: LedgerScopeId,
    billing_customer_id: BillingCustomerId,
    subject_id: SubjectId,
    plan: RatePlan,
    active_from_unix_ms: i64,
    active_until_unix_ms: Option<i64>,
}

impl SubscriptionRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_id: SubscriptionEventId,
        subscription_id: SubscriptionId,
        organization_scope: LedgerScopeId,
        billing_customer_id: BillingCustomerId,
        subject_id: SubjectId,
        plan: RatePlan,
        active_from_unix_ms: i64,
        active_until_unix_ms: Option<i64>,
    ) -> Result<Self, SubscriptionError> {
        if active_from_unix_ms < 0 {
            return Err(SubscriptionError::InvalidActiveFrom(active_from_unix_ms));
        }
        if let Some(until) = active_until_unix_ms {
            if until < 0 {
                return Err(SubscriptionError::InvalidActiveUntil(until));
            }
            if until <= active_from_unix_ms {
                return Err(SubscriptionError::InvalidActiveRange {
                    active_from_unix_ms,
                    active_until_unix_ms: until,
                });
            }
        }
        if active_from_unix_ms < plan.effective_from_unix_ms() {
            return Err(SubscriptionError::SubscriptionBeforePlan {
                subscription_id,
                active_from_unix_ms,
                plan_effective_from_unix_ms: plan.effective_from_unix_ms(),
            });
        }
        match (active_until_unix_ms, plan.effective_until_unix_ms()) {
            (None, Some(plan_until)) => {
                return Err(SubscriptionError::OpenEndedSubscriptionOnFinitePlan {
                    plan_id: plan.id().as_str().to_owned(),
                    plan_effective_until_unix_ms: plan_until,
                });
            }
            (Some(subscription_until), Some(plan_until)) if subscription_until > plan_until => {
                return Err(SubscriptionError::SubscriptionBeyondPlan {
                    subscription_id,
                    active_until_unix_ms: subscription_until,
                    plan_effective_until_unix_ms: plan_until,
                });
            }
            _ => {}
        }

        Ok(Self {
            event_id,
            subscription_id,
            organization_scope,
            billing_customer_id,
            subject_id,
            plan,
            active_from_unix_ms,
            active_until_unix_ms,
        })
    }

    #[must_use]
    pub const fn event_id(&self) -> &SubscriptionEventId {
        &self.event_id
    }

    #[must_use]
    pub const fn subscription_id(&self) -> &SubscriptionId {
        &self.subscription_id
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
    pub const fn subject_id(&self) -> &SubjectId {
        &self.subject_id
    }

    #[must_use]
    pub const fn plan(&self) -> &RatePlan {
        &self.plan
    }

    #[must_use]
    pub const fn active_from_unix_ms(&self) -> i64 {
        self.active_from_unix_ms
    }

    #[must_use]
    pub const fn active_until_unix_ms(&self) -> Option<i64> {
        self.active_until_unix_ms
    }

    fn schedule_key(&self) -> ScheduleKey {
        ScheduleKey::new(
            &self.organization_scope,
            &self.billing_customer_id,
            &self.subject_id,
            self.plan.meter_id(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    source_event_id: SubscriptionEventId,
    id: SubscriptionId,
    organization_scope: LedgerScopeId,
    billing_customer_id: BillingCustomerId,
    subject_id: SubjectId,
    plan: RatePlan,
    active_from_unix_ms: i64,
    active_until_unix_ms: Option<i64>,
}

impl Subscription {
    fn from_request(request: &SubscriptionRequest) -> Self {
        Self {
            source_event_id: request.event_id.clone(),
            id: request.subscription_id.clone(),
            organization_scope: request.organization_scope.clone(),
            billing_customer_id: request.billing_customer_id.clone(),
            subject_id: request.subject_id.clone(),
            plan: request.plan.clone(),
            active_from_unix_ms: request.active_from_unix_ms,
            active_until_unix_ms: request.active_until_unix_ms,
        }
    }

    #[must_use]
    pub const fn source_event_id(&self) -> &SubscriptionEventId {
        &self.source_event_id
    }

    #[must_use]
    pub const fn id(&self) -> &SubscriptionId {
        &self.id
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
    pub const fn subject_id(&self) -> &SubjectId {
        &self.subject_id
    }

    #[must_use]
    pub const fn plan(&self) -> &RatePlan {
        &self.plan
    }

    #[must_use]
    pub const fn active_from_unix_ms(&self) -> i64 {
        self.active_from_unix_ms
    }

    #[must_use]
    pub const fn active_until_unix_ms(&self) -> Option<i64> {
        self.active_until_unix_ms
    }

    pub fn status_at(
        &self,
        timestamp_unix_ms: i64,
    ) -> Result<SubscriptionStatus, SubscriptionError> {
        if timestamp_unix_ms < 0 {
            return Err(SubscriptionError::InvalidStatusTimestamp(timestamp_unix_ms));
        }
        if timestamp_unix_ms < self.active_from_unix_ms {
            return Ok(SubscriptionStatus::Scheduled);
        }
        if self
            .active_until_unix_ms
            .is_some_and(|until| timestamp_unix_ms >= until)
        {
            return Ok(SubscriptionStatus::Ended);
        }
        Ok(SubscriptionStatus::Active)
    }

    fn covers_window_unchecked(&self, window_start_unix_ms: i64, window_end_unix_ms: i64) -> bool {
        window_start_unix_ms >= self.active_from_unix_ms
            && self
                .active_until_unix_ms
                .is_none_or(|until| window_end_unix_ms <= until)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionCreateOutcome {
    Created { subscription: Subscription },
    Replayed { subscription: Subscription },
}

impl SubscriptionCreateOutcome {
    #[must_use]
    pub const fn subscription(&self) -> &Subscription {
        match self {
            Self::Created { subscription } | Self::Replayed { subscription } => subscription,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredSubscription {
    request: SubscriptionRequest,
    subscription: Subscription,
}

#[derive(Debug, Clone, Default)]
pub struct SubscriptionRegistry {
    events: BTreeMap<SubscriptionEventId, StoredSubscription>,
    subscriptions: BTreeMap<SubscriptionId, SubscriptionEventId>,
    schedules: BTreeMap<ScheduleKey, Vec<SubscriptionId>>,
}

impl SubscriptionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    #[must_use]
    pub fn subscription_for_id(&self, id: &SubscriptionId) -> Option<&Subscription> {
        self.subscriptions
            .get(id)
            .and_then(|event_id| self.events.get(event_id))
            .map(|stored| &stored.subscription)
    }

    pub fn create(
        &mut self,
        request: SubscriptionRequest,
    ) -> Result<SubscriptionCreateOutcome, SubscriptionError> {
        if let Some(existing) = self.events.get(request.event_id()) {
            return if existing.request == request {
                Ok(SubscriptionCreateOutcome::Replayed {
                    subscription: existing.subscription.clone(),
                })
            } else {
                Err(SubscriptionError::SourceEventConflict(
                    request.event_id().clone(),
                ))
            };
        }

        if let Some(existing_event_id) = self.subscriptions.get(request.subscription_id()) {
            return Err(SubscriptionError::SubscriptionIdConflict {
                subscription_id: request.subscription_id().clone(),
                existing_event_id: existing_event_id.clone(),
            });
        }

        let schedule_key = request.schedule_key();
        if let Some(existing_ids) = self.schedules.get(&schedule_key) {
            for existing_id in existing_ids {
                if let Some(existing) = self.subscription_for_id(existing_id) {
                    if intervals_overlap(
                        request.active_from_unix_ms(),
                        request.active_until_unix_ms(),
                        existing.active_from_unix_ms(),
                        existing.active_until_unix_ms(),
                    ) {
                        return Err(SubscriptionError::OverlappingSchedule {
                            subscription_id: request.subscription_id().clone(),
                            existing_subscription_id: existing.id().clone(),
                        });
                    }
                }
            }
        }

        let subscription = Subscription::from_request(&request);
        let event_id = request.event_id().clone();
        let subscription_id = request.subscription_id().clone();

        self.events.insert(
            event_id.clone(),
            StoredSubscription {
                request,
                subscription: subscription.clone(),
            },
        );
        self.subscriptions.insert(subscription_id.clone(), event_id);
        self.schedules
            .entry(schedule_key)
            .or_default()
            .push(subscription_id);

        Ok(SubscriptionCreateOutcome::Created { subscription })
    }

    pub fn resolve_for_window(
        &self,
        organization_scope: &LedgerScopeId,
        billing_customer_id: &BillingCustomerId,
        subject_id: &SubjectId,
        meter_id: &MeterId,
        window_start_unix_ms: i64,
        window_end_unix_ms: i64,
    ) -> Result<Option<&Subscription>, SubscriptionError> {
        validate_window(window_start_unix_ms, window_end_unix_ms)?;
        let key = ScheduleKey::new(
            organization_scope,
            billing_customer_id,
            subject_id,
            meter_id,
        );
        let Some(subscription_ids) = self.schedules.get(&key) else {
            return Ok(None);
        };
        for subscription_id in subscription_ids {
            if let Some(subscription) = self.subscription_for_id(subscription_id) {
                if subscription.covers_window_unchecked(window_start_unix_ms, window_end_unix_ms) {
                    return Ok(Some(subscription));
                }
            }
        }
        Ok(None)
    }
}

fn validate_window(
    window_start_unix_ms: i64,
    window_end_unix_ms: i64,
) -> Result<(), SubscriptionError> {
    if window_start_unix_ms < 0 || window_end_unix_ms <= window_start_unix_ms {
        return Err(SubscriptionError::InvalidWindow {
            window_start_unix_ms,
            window_end_unix_ms,
        });
    }
    Ok(())
}

fn intervals_overlap(
    left_start: i64,
    left_end: Option<i64>,
    right_start: i64,
    right_end: Option<i64>,
) -> bool {
    let left_starts_before_right_ends = right_end.is_none_or(|end| left_start < end);
    let right_starts_before_left_ends = left_end.is_none_or(|end| right_start < end);
    left_starts_before_right_ends && right_starts_before_left_ends
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionError {
    EmptyIdentifier(&'static str),
    InvalidActiveFrom(i64),
    InvalidActiveUntil(i64),
    InvalidActiveRange {
        active_from_unix_ms: i64,
        active_until_unix_ms: i64,
    },
    SubscriptionBeforePlan {
        subscription_id: SubscriptionId,
        active_from_unix_ms: i64,
        plan_effective_from_unix_ms: i64,
    },
    OpenEndedSubscriptionOnFinitePlan {
        plan_id: String,
        plan_effective_until_unix_ms: i64,
    },
    SubscriptionBeyondPlan {
        subscription_id: SubscriptionId,
        active_until_unix_ms: i64,
        plan_effective_until_unix_ms: i64,
    },
    InvalidStatusTimestamp(i64),
    InvalidWindow {
        window_start_unix_ms: i64,
        window_end_unix_ms: i64,
    },
    SourceEventConflict(SubscriptionEventId),
    SubscriptionIdConflict {
        subscription_id: SubscriptionId,
        existing_event_id: SubscriptionEventId,
    },
    OverlappingSchedule {
        subscription_id: SubscriptionId,
        existing_subscription_id: SubscriptionId,
    },
}

impl Display for SubscriptionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(label) => write!(formatter, "{label} must not be empty"),
            Self::InvalidActiveFrom(value) => {
                write!(
                    formatter,
                    "active_from_unix_ms must not be negative: {value}"
                )
            }
            Self::InvalidActiveUntil(value) => {
                write!(
                    formatter,
                    "active_until_unix_ms must not be negative: {value}"
                )
            }
            Self::InvalidActiveRange {
                active_from_unix_ms,
                active_until_unix_ms,
            } => write!(
                formatter,
                "subscription interval must be increasing: {active_from_unix_ms}..{active_until_unix_ms}"
            ),
            Self::SubscriptionBeforePlan {
                subscription_id,
                active_from_unix_ms,
                plan_effective_from_unix_ms,
            } => write!(
                formatter,
                "subscription {} starts at {} before plan effectiveness {}",
                subscription_id.as_str(),
                active_from_unix_ms,
                plan_effective_from_unix_ms
            ),
            Self::OpenEndedSubscriptionOnFinitePlan {
                plan_id,
                plan_effective_until_unix_ms,
            } => write!(
                formatter,
                "open-ended subscription cannot outlive finite plan {plan_id} ending at {plan_effective_until_unix_ms}"
            ),
            Self::SubscriptionBeyondPlan {
                subscription_id,
                active_until_unix_ms,
                plan_effective_until_unix_ms,
            } => write!(
                formatter,
                "subscription {} ends at {} after plan effectiveness {}",
                subscription_id.as_str(),
                active_until_unix_ms,
                plan_effective_until_unix_ms
            ),
            Self::InvalidStatusTimestamp(value) => {
                write!(formatter, "status timestamp must not be negative: {value}")
            }
            Self::InvalidWindow {
                window_start_unix_ms,
                window_end_unix_ms,
            } => write!(
                formatter,
                "usage window must be non-negative and increasing: {window_start_unix_ms}..{window_end_unix_ms}"
            ),
            Self::SourceEventConflict(event_id) => write!(
                formatter,
                "subscription source event conflict: {}",
                event_id.as_str()
            ),
            Self::SubscriptionIdConflict {
                subscription_id,
                existing_event_id,
            } => write!(
                formatter,
                "subscription {} already has history under event {}",
                subscription_id.as_str(),
                existing_event_id.as_str()
            ),
            Self::OverlappingSchedule {
                subscription_id,
                existing_subscription_id,
            } => write!(
                formatter,
                "subscription {} overlaps schedule {} for the same commercial key",
                subscription_id.as_str(),
                existing_subscription_id.as_str()
            ),
        }
    }
}

impl Error for SubscriptionError {}
