//! Versioned, checked persistence records for metering facts.
//!
//! This module reconstructs accepted meter definitions and usage events using
//! the original domain constructors and registry methods. It does not persist
//! derived aggregates or enable a durable production acceptance boundary.

use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageEvent,
    UsageEventId, UsageValue, WindowSize,
};
use serde::{Deserialize, Serialize};

use crate::{CodecError, parse_i64_exact, parse_i128_exact};

const VERSION: u64 = 1;
const DEFINITION_KIND: &str = "meter.definition";
const USAGE_KIND: &str = "meter.event";
const MAX_METER_FACT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredAggregation {
    Count,
    Sum,
}

impl From<Aggregation> for StoredAggregation {
    fn from(value: Aggregation) -> Self {
        match value {
            Aggregation::Count => Self::Count,
            Aggregation::Sum => Self::Sum,
        }
    }
}

impl From<StoredAggregation> for Aggregation {
    fn from(value: StoredAggregation) -> Self {
        match value {
            StoredAggregation::Count => Self::Count,
            StoredAggregation::Sum => Self::Sum,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredWindow {
    Minute,
    Hour,
    Day,
}

impl From<WindowSize> for StoredWindow {
    fn from(value: WindowSize) -> Self {
        match value {
            WindowSize::Minute => Self::Minute,
            WindowSize::Hour => Self::Hour,
            WindowSize::Day => Self::Day,
        }
    }
}

impl From<StoredWindow> for WindowSize {
    fn from(value: StoredWindow) -> Self {
        match value {
            StoredWindow::Minute => Self::Minute,
            StoredWindow::Hour => Self::Hour,
            StoredWindow::Day => Self::Day,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MeterDefinitionRecord {
    id: String,
    event_type: String,
    aggregation: StoredAggregation,
    window_size: StoredWindow,
    active_from_unix_ms: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum UsageValueRecord {
    Count {},
    Sum { quantity: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageEventRecord {
    id: String,
    meter_id: String,
    event_type: String,
    subject_id: String,
    occurred_at_unix_ms: String,
    observed_at_unix_ms: String,
    value: UsageValueRecord,
}

pub enum MeteringFact {
    Definition(MeterDefinition),
    Event(UsageEvent),
}

fn encode<T: Serialize>(kind: &str, payload: T) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: kind.to_owned(),
        payload,
    })
    .map_err(|err| CodecError::InvalidPayload(err.to_string()))?;
    if bytes.len() > MAX_METER_FACT_BYTES {
        return Err(CodecError::InvalidPayload(
            "metering fact exceeds 1MiB".to_owned(),
        ));
    }
    Ok(bytes)
}

/// Store the original definition, never a computed aggregate.
pub fn encode_meter_definition(definition: &MeterDefinition) -> Result<Vec<u8>, CodecError> {
    encode(
        DEFINITION_KIND,
        MeterDefinitionRecord {
            id: definition.id().as_str().to_owned(),
            event_type: definition.event_type().as_str().to_owned(),
            aggregation: definition.aggregation().into(),
            window_size: definition.window_size().into(),
            active_from_unix_ms: definition
                .active_from_unix_ms()
                .map(|time| time.to_string()),
        },
    )
}

/// Usage quantities are decimal strings, not floating-point JSON values or
/// currency minor units. The Count and Sum shapes remain explicitly distinct.
pub fn encode_usage_event(event: &UsageEvent) -> Result<Vec<u8>, CodecError> {
    encode(
        USAGE_KIND,
        UsageEventRecord {
            id: event.id().as_str().to_owned(),
            meter_id: event.meter_id().as_str().to_owned(),
            event_type: event.event_type().as_str().to_owned(),
            subject_id: event.subject_id().as_str().to_owned(),
            occurred_at_unix_ms: event.occurred_at_unix_ms().to_string(),
            observed_at_unix_ms: event.observed_at_unix_ms().to_string(),
            value: match event.value() {
                UsageValue::Count => UsageValueRecord::Count {},
                UsageValue::Sum(quantity) => UsageValueRecord::Sum {
                    quantity: quantity.to_string(),
                },
            },
        },
    )
}

/// Interpret a single versioned record, rejecting unknown schema/type and
/// malformed data before invoking the checked domain constructors.
pub fn decode_metering_fact(bytes: &[u8]) -> Result<MeteringFact, CodecError> {
    if bytes.len() > MAX_METER_FACT_BYTES {
        return Err(CodecError::InvalidPayload(
            "metering fact exceeds 1MiB".to_owned(),
        ));
    }
    let header: Envelope<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if header.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(header.schema_version));
    }

    match header.record_type.as_str() {
        DEFINITION_KIND => {
            let record: Envelope<MeterDefinitionRecord> = serde_json::from_slice(bytes)
                .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
            let p = record.payload;
            let definition = MeterDefinition::new(
                MeterId::new(p.id).map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
                EventType::new(p.event_type)
                    .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
                p.aggregation.into(),
                p.window_size.into(),
                p.active_from_unix_ms
                    .as_deref()
                    .map(parse_i64_exact)
                    .transpose()?,
            )
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?;
            Ok(MeteringFact::Definition(definition))
        }
        USAGE_KIND => {
            let record: Envelope<UsageEventRecord> = serde_json::from_slice(bytes)
                .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
            let p = record.payload;
            let value = match p.value {
                UsageValueRecord::Count {} => UsageValue::Count,
                UsageValueRecord::Sum { quantity } => UsageValue::Sum(parse_i128_exact(&quantity)?),
            };
            let event = UsageEvent::new(
                UsageEventId::new(p.id).map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
                MeterId::new(p.meter_id).map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
                EventType::new(p.event_type)
                    .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
                SubjectId::new(p.subject_id)
                    .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
                parse_i64_exact(&p.occurred_at_unix_ms)?,
                parse_i64_exact(&p.observed_at_unix_ms)?,
                value,
            )
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?;
            Ok(MeteringFact::Event(event))
        }
        kind => Err(CodecError::UnsupportedRecordKind(kind.to_owned())),
    }
}

/// Facts must arrive in canonical dependency order: definition before usage.
/// Duplicate identical events remain idempotent; conflicts fail via the domain.
pub fn replay_metering<'a>(
    facts: impl IntoIterator<Item = &'a [u8]>,
) -> Result<MeteringEngine, CodecError> {
    let mut engine = MeteringEngine::new();
    for record in facts {
        match decode_metering_fact(record)? {
            MeteringFact::Definition(definition) => {
                engine
                    .register_meter(definition)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
            }
            MeteringFact::Event(event) => {
                engine
                    .ingest(event)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
            }
        }
    }
    Ok(engine)
}
