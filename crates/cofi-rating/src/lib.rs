use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_billing::BillingCustomerId;
use cofi_ledger::Currency;
use cofi_metering::{MeterId, SubjectId, UsageAggregate};

macro_rules! rating_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, RatingError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(RatingError::EmptyIdentifier($label));
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

rating_id!(RatingPlanId, "rating_plan_id");
rating_id!(RatingEventId, "rating_event_id");
rating_id!(RatedChargeId, "rated_charge_id");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatePlan {
    id: RatingPlanId,
    meter_id: MeterId,
    currency: Currency,
    unit_price_minor: i128,
    included_units: i128,
    effective_from_unix_ms: i64,
    effective_until_unix_ms: Option<i64>,
}

impl RatePlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: RatingPlanId,
        meter_id: MeterId,
        currency: Currency,
        unit_price_minor: i128,
        included_units: i128,
        effective_from_unix_ms: i64,
        effective_until_unix_ms: Option<i64>,
    ) -> Result<Self, RatingError> {
        if unit_price_minor <= 0 {
            return Err(RatingError::NonPositiveUnitPrice(unit_price_minor));
        }
        if included_units < 0 {
            return Err(RatingError::NegativeIncludedUnits(included_units));
        }
        if effective_from_unix_ms < 0 {
            return Err(RatingError::InvalidEffectiveTimestamp(
                "effective_from_unix_ms",
            ));
        }
        if let Some(until) = effective_until_unix_ms {
            if until < 0 {
                return Err(RatingError::InvalidEffectiveTimestamp(
                    "effective_until_unix_ms",
                ));
            }
            if until <= effective_from_unix_ms {
                return Err(RatingError::InvalidEffectiveRange {
                    effective_from_unix_ms,
                    effective_until_unix_ms: until,
                });
            }
        }
        Ok(Self {
            id,
            meter_id,
            currency,
            unit_price_minor,
            included_units,
            effective_from_unix_ms,
            effective_until_unix_ms,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &RatingPlanId {
        &self.id
    }

    #[must_use]
    pub const fn meter_id(&self) -> &MeterId {
        &self.meter_id
    }

    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    #[must_use]
    pub const fn unit_price_minor(&self) -> i128 {
        self.unit_price_minor
    }

    #[must_use]
    pub const fn included_units(&self) -> i128 {
        self.included_units
    }

    #[must_use]
    pub const fn effective_from_unix_ms(&self) -> i64 {
        self.effective_from_unix_ms
    }

    #[must_use]
    pub const fn effective_until_unix_ms(&self) -> Option<i64> {
        self.effective_until_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatingRequest {
    event_id: RatingEventId,
    aggregate: UsageAggregate,
    billing_customer_id: BillingCustomerId,
    plan: RatePlan,
    rated_at_unix_ms: i64,
}

impl RatingRequest {
    pub fn new(
        event_id: RatingEventId,
        aggregate: UsageAggregate,
        billing_customer_id: BillingCustomerId,
        plan: RatePlan,
        rated_at_unix_ms: i64,
    ) -> Result<Self, RatingError> {
        if rated_at_unix_ms < 0 {
            return Err(RatingError::InvalidRatedAt(rated_at_unix_ms));
        }
        if rated_at_unix_ms < aggregate.window_end_unix_ms() {
            return Err(RatingError::RatingBeforeWindowEnd {
                rated_at_unix_ms,
                window_end_unix_ms: aggregate.window_end_unix_ms(),
            });
        }
        Ok(Self {
            event_id,
            aggregate,
            billing_customer_id,
            plan,
            rated_at_unix_ms,
        })
    }

    #[must_use]
    pub const fn event_id(&self) -> &RatingEventId {
        &self.event_id
    }

    #[must_use]
    pub const fn aggregate(&self) -> &UsageAggregate {
        &self.aggregate
    }

    #[must_use]
    pub const fn billing_customer_id(&self) -> &BillingCustomerId {
        &self.billing_customer_id
    }

    #[must_use]
    pub const fn plan(&self) -> &RatePlan {
        &self.plan
    }

    #[must_use]
    pub const fn rated_at_unix_ms(&self) -> i64 {
        self.rated_at_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatedCharge {
    id: RatedChargeId,
    rating_event_id: RatingEventId,
    plan_id: RatingPlanId,
    meter_id: MeterId,
    subject_id: SubjectId,
    billing_customer_id: BillingCustomerId,
    window_start_unix_ms: i64,
    window_end_unix_ms: i64,
    source_event_count: u64,
    aggregate_units: i128,
    billable_units: i128,
    currency: Currency,
    unit_price_minor: i128,
    included_units: i128,
    amount_minor: i128,
    rated_at_unix_ms: i64,
}

impl RatedCharge {
    #[must_use]
    pub const fn id(&self) -> &RatedChargeId {
        &self.id
    }

    #[must_use]
    pub const fn rating_event_id(&self) -> &RatingEventId {
        &self.rating_event_id
    }

    #[must_use]
    pub const fn plan_id(&self) -> &RatingPlanId {
        &self.plan_id
    }

    #[must_use]
    pub const fn meter_id(&self) -> &MeterId {
        &self.meter_id
    }

    #[must_use]
    pub const fn subject_id(&self) -> &SubjectId {
        &self.subject_id
    }

    #[must_use]
    pub const fn billing_customer_id(&self) -> &BillingCustomerId {
        &self.billing_customer_id
    }

    #[must_use]
    pub const fn window_start_unix_ms(&self) -> i64 {
        self.window_start_unix_ms
    }

    #[must_use]
    pub const fn window_end_unix_ms(&self) -> i64 {
        self.window_end_unix_ms
    }

    #[must_use]
    pub const fn source_event_count(&self) -> u64 {
        self.source_event_count
    }

    #[must_use]
    pub const fn aggregate_units(&self) -> i128 {
        self.aggregate_units
    }

    #[must_use]
    pub const fn billable_units(&self) -> i128 {
        self.billable_units
    }

    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    #[must_use]
    pub const fn unit_price_minor(&self) -> i128 {
        self.unit_price_minor
    }

    #[must_use]
    pub const fn included_units(&self) -> i128 {
        self.included_units
    }

    #[must_use]
    pub const fn amount_minor(&self) -> i128 {
        self.amount_minor
    }

    #[must_use]
    pub const fn rated_at_unix_ms(&self) -> i64 {
        self.rated_at_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RatingOutcome {
    Rated { charge: RatedCharge },
    Replayed { charge: RatedCharge },
}

impl RatingOutcome {
    #[must_use]
    pub const fn charge(&self) -> &RatedCharge {
        match self {
            Self::Rated { charge } | Self::Replayed { charge } => charge,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RatingKey {
    meter_id: MeterId,
    subject_id: SubjectId,
    window_start_unix_ms: i64,
    window_end_unix_ms: i64,
    plan_id: RatingPlanId,
}

impl RatingKey {
    fn from_request(request: &RatingRequest) -> Self {
        Self {
            meter_id: request.aggregate.meter_id().clone(),
            subject_id: request.aggregate.subject_id().clone(),
            window_start_unix_ms: request.aggregate.window_start_unix_ms(),
            window_end_unix_ms: request.aggregate.window_end_unix_ms(),
            plan_id: request.plan.id().clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredRating {
    request: RatingRequest,
    charge: RatedCharge,
}

#[derive(Debug, Clone, Default)]
pub struct RatingRegistry {
    events: BTreeMap<RatingEventId, StoredRating>,
    rating_keys: BTreeMap<RatingKey, RatingEventId>,
}

impl RatingRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn rating_key_count(&self) -> usize {
        self.rating_keys.len()
    }

    #[must_use]
    pub fn charge_for_event(&self, event_id: &RatingEventId) -> Option<&RatedCharge> {
        self.events.get(event_id).map(|stored| &stored.charge)
    }

    pub fn rate(&mut self, request: RatingRequest) -> Result<RatingOutcome, RatingError> {
        if let Some(existing) = self.events.get(request.event_id()) {
            return if existing.request == request {
                Ok(RatingOutcome::Replayed {
                    charge: existing.charge.clone(),
                })
            } else {
                Err(RatingError::RatingEventConflict(request.event_id().clone()))
            };
        }

        validate_request(&request)?;
        let key = RatingKey::from_request(&request);
        if let Some(existing_event_id) = self.rating_keys.get(&key) {
            return Err(RatingError::DuplicateRatingKey {
                existing_event_id: existing_event_id.clone(),
            });
        }

        let aggregate_units = request.aggregate.aggregate_value();
        let billable_units = if aggregate_units > request.plan.included_units() {
            aggregate_units
                .checked_sub(request.plan.included_units())
                .ok_or(RatingError::ArithmeticOverflow)?
        } else {
            0
        };
        let amount_minor = billable_units
            .checked_mul(request.plan.unit_price_minor())
            .ok_or(RatingError::ArithmeticOverflow)?;

        let charge = RatedCharge {
            id: RatedChargeId::new(format!("rating:{}:charge", request.event_id.as_str()))?,
            rating_event_id: request.event_id.clone(),
            plan_id: request.plan.id.clone(),
            meter_id: request.aggregate.meter_id().clone(),
            subject_id: request.aggregate.subject_id().clone(),
            billing_customer_id: request.billing_customer_id.clone(),
            window_start_unix_ms: request.aggregate.window_start_unix_ms(),
            window_end_unix_ms: request.aggregate.window_end_unix_ms(),
            source_event_count: request.aggregate.event_count(),
            aggregate_units,
            billable_units,
            currency: request.plan.currency(),
            unit_price_minor: request.plan.unit_price_minor(),
            included_units: request.plan.included_units(),
            amount_minor,
            rated_at_unix_ms: request.rated_at_unix_ms,
        };

        self.rating_keys.insert(key, request.event_id().clone());
        self.events.insert(
            request.event_id().clone(),
            StoredRating {
                request,
                charge: charge.clone(),
            },
        );
        Ok(RatingOutcome::Rated { charge })
    }
}

fn validate_request(request: &RatingRequest) -> Result<(), RatingError> {
    if request.plan.meter_id() != request.aggregate.meter_id() {
        return Err(RatingError::MeterMismatch {
            plan_meter_id: request.plan.meter_id().clone(),
            aggregate_meter_id: request.aggregate.meter_id().clone(),
        });
    }

    let window_start = request.aggregate.window_start_unix_ms();
    let window_end = request.aggregate.window_end_unix_ms();
    let outside_start = window_start < request.plan.effective_from_unix_ms();
    let outside_end = request
        .plan
        .effective_until_unix_ms()
        .is_some_and(|until| window_end > until);
    if outside_start || outside_end {
        return Err(RatingError::WindowOutsidePlan {
            window_start_unix_ms: window_start,
            window_end_unix_ms: window_end,
            effective_from_unix_ms: request.plan.effective_from_unix_ms(),
            effective_until_unix_ms: request.plan.effective_until_unix_ms(),
        });
    }

    if request.aggregate.aggregate_value() < 0 {
        return Err(RatingError::NegativeAggregateValue(
            request.aggregate.aggregate_value(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RatingError {
    EmptyIdentifier(&'static str),
    NonPositiveUnitPrice(i128),
    NegativeIncludedUnits(i128),
    InvalidEffectiveTimestamp(&'static str),
    InvalidEffectiveRange {
        effective_from_unix_ms: i64,
        effective_until_unix_ms: i64,
    },
    InvalidRatedAt(i64),
    RatingBeforeWindowEnd {
        rated_at_unix_ms: i64,
        window_end_unix_ms: i64,
    },
    MeterMismatch {
        plan_meter_id: MeterId,
        aggregate_meter_id: MeterId,
    },
    WindowOutsidePlan {
        window_start_unix_ms: i64,
        window_end_unix_ms: i64,
        effective_from_unix_ms: i64,
        effective_until_unix_ms: Option<i64>,
    },
    NegativeAggregateValue(i128),
    ArithmeticOverflow,
    RatingEventConflict(RatingEventId),
    DuplicateRatingKey {
        existing_event_id: RatingEventId,
    },
}

impl Display for RatingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(label) => write!(formatter, "{label} must not be empty"),
            Self::NonPositiveUnitPrice(value) => {
                write!(formatter, "unit price must be positive: {value}")
            }
            Self::NegativeIncludedUnits(value) => {
                write!(formatter, "included units must not be negative: {value}")
            }
            Self::InvalidEffectiveTimestamp(label) => {
                write!(formatter, "{label} must not be negative")
            }
            Self::InvalidEffectiveRange {
                effective_from_unix_ms,
                effective_until_unix_ms,
            } => write!(
                formatter,
                "rate plan effective-until {effective_until_unix_ms} must be after effective-from {effective_from_unix_ms}"
            ),
            Self::InvalidRatedAt(value) => {
                write!(formatter, "rated_at_unix_ms must not be negative: {value}")
            }
            Self::RatingBeforeWindowEnd {
                rated_at_unix_ms,
                window_end_unix_ms,
            } => write!(
                formatter,
                "rating timestamp {rated_at_unix_ms} precedes usage-window end {window_end_unix_ms}"
            ),
            Self::MeterMismatch {
                plan_meter_id,
                aggregate_meter_id,
            } => write!(
                formatter,
                "rate plan meter {} does not match aggregate meter {}",
                plan_meter_id.as_str(),
                aggregate_meter_id.as_str()
            ),
            Self::WindowOutsidePlan {
                window_start_unix_ms,
                window_end_unix_ms,
                effective_from_unix_ms,
                effective_until_unix_ms,
            } => write!(
                formatter,
                "usage window [{window_start_unix_ms}, {window_end_unix_ms}) is outside rate-plan interval [{effective_from_unix_ms}, {effective_until_unix_ms:?})"
            ),
            Self::NegativeAggregateValue(value) => {
                write!(formatter, "aggregate usage must not be negative: {value}")
            }
            Self::ArithmeticOverflow => write!(formatter, "rating arithmetic overflow"),
            Self::RatingEventConflict(id) => {
                write!(formatter, "rating event id conflict: {}", id.as_str())
            }
            Self::DuplicateRatingKey { existing_event_id } => write!(
                formatter,
                "usage window is already rated by event {}",
                existing_event_id.as_str()
            ),
        }
    }
}

impl Error for RatingError {}
