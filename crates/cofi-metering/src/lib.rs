use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

macro_rules! metering_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, MeteringError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(MeteringError::EmptyIdentifier($label));
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

metering_id!(MeterId, "meter_id");
metering_id!(UsageEventId, "usage_event_id");
metering_id!(SubjectId, "subject_id");
metering_id!(EventType, "event_type");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregation {
    Count,
    Sum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSize {
    Minute,
    Hour,
    Day,
}

impl WindowSize {
    #[must_use]
    pub const fn duration_ms(self) -> i64 {
        match self {
            Self::Minute => 60_000,
            Self::Hour => 3_600_000,
            Self::Day => 86_400_000,
        }
    }

    #[must_use]
    pub const fn is_aligned(self, timestamp_unix_ms: i64) -> bool {
        timestamp_unix_ms >= 0 && timestamp_unix_ms % self.duration_ms() == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeterDefinition {
    id: MeterId,
    event_type: EventType,
    aggregation: Aggregation,
    window_size: WindowSize,
    active_from_unix_ms: Option<i64>,
}

impl MeterDefinition {
    pub fn new(
        id: MeterId,
        event_type: EventType,
        aggregation: Aggregation,
        window_size: WindowSize,
        active_from_unix_ms: Option<i64>,
    ) -> Result<Self, MeteringError> {
        if active_from_unix_ms.is_some_and(|timestamp| timestamp < 0) {
            return Err(MeteringError::InvalidTimestamp("active_from_unix_ms"));
        }
        Ok(Self {
            id,
            event_type,
            aggregation,
            window_size,
            active_from_unix_ms,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &MeterId {
        &self.id
    }

    #[must_use]
    pub const fn event_type(&self) -> &EventType {
        &self.event_type
    }

    #[must_use]
    pub const fn aggregation(&self) -> Aggregation {
        self.aggregation
    }

    #[must_use]
    pub const fn window_size(&self) -> WindowSize {
        self.window_size
    }

    #[must_use]
    pub const fn active_from_unix_ms(&self) -> Option<i64> {
        self.active_from_unix_ms
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageValue {
    Count,
    Sum(i128),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageEvent {
    id: UsageEventId,
    meter_id: MeterId,
    event_type: EventType,
    subject_id: SubjectId,
    occurred_at_unix_ms: i64,
    observed_at_unix_ms: i64,
    value: UsageValue,
}

impl UsageEvent {
    pub fn new(
        id: UsageEventId,
        meter_id: MeterId,
        event_type: EventType,
        subject_id: SubjectId,
        occurred_at_unix_ms: i64,
        observed_at_unix_ms: i64,
        value: UsageValue,
    ) -> Result<Self, MeteringError> {
        if occurred_at_unix_ms < 0 {
            return Err(MeteringError::InvalidTimestamp("occurred_at_unix_ms"));
        }
        if observed_at_unix_ms < 0 {
            return Err(MeteringError::InvalidTimestamp("observed_at_unix_ms"));
        }
        if occurred_at_unix_ms > observed_at_unix_ms {
            return Err(MeteringError::ObservedBeforeOccurrence {
                occurred_at_unix_ms,
                observed_at_unix_ms,
            });
        }
        if matches!(value, UsageValue::Sum(amount) if amount <= 0) {
            return Err(MeteringError::NonPositiveUsageValue);
        }
        Ok(Self {
            id,
            meter_id,
            event_type,
            subject_id,
            occurred_at_unix_ms,
            observed_at_unix_ms,
            value,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &UsageEventId {
        &self.id
    }

    #[must_use]
    pub const fn meter_id(&self) -> &MeterId {
        &self.meter_id
    }

    #[must_use]
    pub const fn event_type(&self) -> &EventType {
        &self.event_type
    }

    #[must_use]
    pub const fn subject_id(&self) -> &SubjectId {
        &self.subject_id
    }

    #[must_use]
    pub const fn occurred_at_unix_ms(&self) -> i64 {
        self.occurred_at_unix_ms
    }

    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at_unix_ms
    }

    #[must_use]
    pub const fn value(&self) -> UsageValue {
        self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationOutcome {
    Registered,
    Replayed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestionOutcome {
    Accepted,
    Replayed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageAggregate {
    meter_id: MeterId,
    subject_id: SubjectId,
    window_start_unix_ms: i64,
    window_end_unix_ms: i64,
    event_count: u64,
    aggregate_value: i128,
}

impl UsageAggregate {
    #[must_use]
    pub const fn meter_id(&self) -> &MeterId {
        &self.meter_id
    }
    #[must_use]
    pub const fn subject_id(&self) -> &SubjectId {
        &self.subject_id
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
    pub const fn event_count(&self) -> u64 {
        self.event_count
    }
    #[must_use]
    pub const fn aggregate_value(&self) -> i128 {
        self.aggregate_value
    }
}

#[derive(Debug, Clone, Default)]
pub struct MeteringEngine {
    meters: BTreeMap<MeterId, MeterDefinition>,
    events: BTreeMap<UsageEventId, UsageEvent>,
}

impl MeteringEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn meter(&self, id: &MeterId) -> Option<&MeterDefinition> {
        self.meters.get(id)
    }

    #[must_use]
    pub fn event(&self, id: &UsageEventId) -> Option<&UsageEvent> {
        self.events.get(id)
    }

    #[must_use]
    pub fn meter_count(&self) -> usize {
        self.meters.len()
    }

    #[must_use]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn register_meter(
        &mut self,
        definition: MeterDefinition,
    ) -> Result<RegistrationOutcome, MeteringError> {
        if let Some(existing) = self.meters.get(definition.id()) {
            return if existing == &definition {
                Ok(RegistrationOutcome::Replayed)
            } else {
                Err(MeteringError::MeterIdConflict(definition.id.clone()))
            };
        }
        self.meters.insert(definition.id.clone(), definition);
        Ok(RegistrationOutcome::Registered)
    }

    pub fn ingest(&mut self, event: UsageEvent) -> Result<IngestionOutcome, MeteringError> {
        if let Some(existing) = self.events.get(event.id()) {
            return if existing == &event {
                Ok(IngestionOutcome::Replayed)
            } else {
                Err(MeteringError::UsageEventIdConflict(event.id.clone()))
            };
        }

        let meter = self
            .meters
            .get(event.meter_id())
            .ok_or_else(|| MeteringError::UnknownMeter(event.meter_id.clone()))?;
        if event.event_type() != meter.event_type() {
            return Err(MeteringError::EventTypeMismatch);
        }
        if meter
            .active_from_unix_ms()
            .is_some_and(|active_from| event.occurred_at_unix_ms() < active_from)
        {
            return Err(MeteringError::EventBeforeActivation);
        }
        validate_usage_shape(meter.aggregation(), event.value())?;
        self.events.insert(event.id.clone(), event);
        Ok(IngestionOutcome::Accepted)
    }

    pub fn aggregate(
        &self,
        meter_id: &MeterId,
        subject_id: &SubjectId,
        window_start_unix_ms: i64,
        window_end_unix_ms: i64,
    ) -> Result<UsageAggregate, MeteringError> {
        let meter = self
            .meters
            .get(meter_id)
            .ok_or_else(|| MeteringError::UnknownMeter(meter_id.clone()))?;
        validate_query_window(
            meter.window_size(),
            window_start_unix_ms,
            window_end_unix_ms,
        )?;

        let mut event_count = 0_u64;
        let mut aggregate_value = 0_i128;
        for event in self.events.values() {
            if event.meter_id() != meter_id || event.subject_id() != subject_id {
                continue;
            }
            if event.occurred_at_unix_ms() < window_start_unix_ms
                || event.occurred_at_unix_ms() >= window_end_unix_ms
            {
                continue;
            }
            event_count = event_count
                .checked_add(1)
                .ok_or(MeteringError::AggregationOverflow)?;
            match (meter.aggregation(), event.value()) {
                (Aggregation::Count, UsageValue::Count) => {
                    aggregate_value = aggregate_value
                        .checked_add(1)
                        .ok_or(MeteringError::AggregationOverflow)?;
                }
                (Aggregation::Sum, UsageValue::Sum(value)) => {
                    aggregate_value = aggregate_value
                        .checked_add(value)
                        .ok_or(MeteringError::AggregationOverflow)?;
                }
                _ => return Err(MeteringError::UsageShapeMismatch),
            }
        }

        Ok(UsageAggregate {
            meter_id: meter_id.clone(),
            subject_id: subject_id.clone(),
            window_start_unix_ms,
            window_end_unix_ms,
            event_count,
            aggregate_value,
        })
    }
}
fn validate_usage_shape(aggregation: Aggregation, value: UsageValue) -> Result<(), MeteringError> {
    match (aggregation, value) {
        (Aggregation::Count, UsageValue::Count) | (Aggregation::Sum, UsageValue::Sum(_)) => Ok(()),
        _ => Err(MeteringError::UsageShapeMismatch),
    }
}

fn validate_query_window(
    window_size: WindowSize,
    start_unix_ms: i64,
    end_unix_ms: i64,
) -> Result<(), MeteringError> {
    if start_unix_ms < 0 || end_unix_ms < 0 || start_unix_ms >= end_unix_ms {
        return Err(MeteringError::InvalidWindowRange {
            start_unix_ms,
            end_unix_ms,
        });
    }
    if !window_size.is_aligned(start_unix_ms) || !window_size.is_aligned(end_unix_ms) {
        return Err(MeteringError::UnalignedWindow);
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeteringError {
    EmptyIdentifier(&'static str),
    InvalidTimestamp(&'static str),
    ObservedBeforeOccurrence {
        occurred_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    },
    NonPositiveUsageValue,
    MeterIdConflict(MeterId),
    UsageEventIdConflict(UsageEventId),
    UnknownMeter(MeterId),
    EventTypeMismatch,
    EventBeforeActivation,
    UsageShapeMismatch,
    InvalidWindowRange {
        start_unix_ms: i64,
        end_unix_ms: i64,
    },
    UnalignedWindow,
    AggregationOverflow,
}

impl Display for MeteringError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(label) => write!(formatter, "{label} must not be empty"),
            Self::InvalidTimestamp(label) => write!(formatter, "{label} must not be negative"),
            Self::ObservedBeforeOccurrence {
                occurred_at_unix_ms,
                observed_at_unix_ms,
            } => write!(
                formatter,
                "observed timestamp {observed_at_unix_ms} precedes occurrence {occurred_at_unix_ms}"
            ),
            Self::NonPositiveUsageValue => write!(formatter, "sum usage value must be positive"),
            Self::MeterIdConflict(id) => write!(formatter, "meter id conflict: {}", id.as_str()),
            Self::UsageEventIdConflict(id) => {
                write!(formatter, "usage event id conflict: {}", id.as_str())
            }
            Self::UnknownMeter(id) => write!(formatter, "unknown meter: {}", id.as_str()),
            Self::EventTypeMismatch => write!(formatter, "usage event type does not match meter"),
            Self::EventBeforeActivation => {
                write!(formatter, "usage event precedes meter activation")
            }
            Self::UsageShapeMismatch => {
                write!(formatter, "usage value shape does not match aggregation")
            }
            Self::InvalidWindowRange {
                start_unix_ms,
                end_unix_ms,
            } => write!(
                formatter,
                "invalid query window [{start_unix_ms}, {end_unix_ms})"
            ),
            Self::UnalignedWindow => write!(
                formatter,
                "query window is not aligned to meter window size"
            ),
            Self::AggregationOverflow => write!(formatter, "usage aggregation overflow"),
        }
    }
}

impl Error for MeteringError {}
