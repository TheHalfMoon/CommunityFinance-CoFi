//! Checked historical pricing plan and accepted rating evidence.
//!
//! Each acceptance binds a frozen subset of original usage facts, a checked
//! meter definition, the immutable plan snapshot, and the original charge.
//! This does not authenticate snapshot completeness against a durable upstream
//! event log; G002-G004 must atomically bind accepted history to a stream
//! checkpoint before production hydration can trust it.

use std::collections::BTreeMap;

use cofi_billing::BillingCustomerId;
use cofi_ledger::Currency;
use cofi_metering::{MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageEvent};
use cofi_rating::{
    RatePlan, RatedCharge, RatingEventId, RatingOutcome, RatingPlanId, RatingRegistry,
    RatingRequest,
};
use serde::{Deserialize, Serialize};

use crate::metering::{
    MeteringFact, decode_metering_fact, encode_meter_definition, encode_usage_event,
};
use crate::{CodecError, parse_i64_exact, parse_i128_exact};

const VERSION: u64 = 1;
const PLAN_KIND: &str = "rate.plan";
const ACCEPTANCE_KIND: &str = "rating.acceptance";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RatePlanRecord {
    id: String,
    meter_id: String,
    currency: String,
    unit_price_minor: String,
    included_units: String,
    effective_from_unix_ms: String,
    effective_until_unix_ms: Option<String>,
}

impl RatePlanRecord {
    fn from_domain(plan: &RatePlan) -> Self {
        Self {
            id: plan.id().as_str().to_owned(),
            meter_id: plan.meter_id().as_str().to_owned(),
            currency: plan.currency().code().to_owned(),
            unit_price_minor: plan.unit_price_minor().to_string(),
            included_units: plan.included_units().to_string(),
            effective_from_unix_ms: plan.effective_from_unix_ms().to_string(),
            effective_until_unix_ms: plan.effective_until_unix_ms().map(|v| v.to_string()),
        }
    }

    fn checked_domain(self) -> Result<RatePlan, CodecError> {
        RatePlan::new(
            RatingPlanId::new(self.id).map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
            MeterId::new(self.meter_id).map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
            Currency::new(&self.currency).map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
            parse_i128_exact(&self.unit_price_minor)?,
            parse_i128_exact(&self.included_units)?,
            parse_i64_exact(&self.effective_from_unix_ms)?,
            self.effective_until_unix_ms
                .as_deref()
                .map(parse_i64_exact)
                .transpose()?,
        )
        .map_err(|e| CodecError::InvalidDomain(e.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChargeReceipt {
    id: String,
    rating_event_id: String,
    plan_id: String,
    meter_id: String,
    subject_id: String,
    billing_customer_id: String,
    window_start_unix_ms: String,
    window_end_unix_ms: String,
    source_event_count: String,
    aggregate_units: String,
    billable_units: String,
    currency: String,
    unit_price_minor: String,
    included_units: String,
    amount_minor: String,
    rated_at_unix_ms: String,
}

impl ChargeReceipt {
    fn from_domain(c: &RatedCharge) -> Self {
        Self {
            id: c.id().as_str().to_owned(),
            rating_event_id: c.rating_event_id().as_str().to_owned(),
            plan_id: c.plan_id().as_str().to_owned(),
            meter_id: c.meter_id().as_str().to_owned(),
            subject_id: c.subject_id().as_str().to_owned(),
            billing_customer_id: c.billing_customer_id().as_str().to_owned(),
            window_start_unix_ms: c.window_start_unix_ms().to_string(),
            window_end_unix_ms: c.window_end_unix_ms().to_string(),
            source_event_count: c.source_event_count().to_string(),
            aggregate_units: c.aggregate_units().to_string(),
            billable_units: c.billable_units().to_string(),
            currency: c.currency().code().to_owned(),
            unit_price_minor: c.unit_price_minor().to_string(),
            included_units: c.included_units().to_string(),
            amount_minor: c.amount_minor().to_string(),
            rated_at_unix_ms: c.rated_at_unix_ms().to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RatingAcceptanceRecord {
    meter: serde_json::Value,
    usage_events: Vec<serde_json::Value>,
    plan: RatePlanRecord,
    rating_event_id: String,
    subject_id: String,
    billing_customer_id: String,
    window_start_unix_ms: String,
    window_end_unix_ms: String,
    rated_at_unix_ms: String,
    charge: ChargeReceipt,
}

/// The caller must supply exactly the source events used at acceptance time.
/// This interface cannot itself prove the event set complete or authenticated.
pub struct RatingAcceptance<'a> {
    pub meter: &'a MeterDefinition,
    pub events: &'a [UsageEvent],
    pub plan: &'a RatePlan,
    pub rating_event_id: &'a RatingEventId,
    pub subject_id: &'a SubjectId,
    pub billing_customer_id: &'a BillingCustomerId,
    pub window_start_unix_ms: i64,
    pub window_end_unix_ms: i64,
    pub rated_at_unix_ms: i64,
    pub accepted_charge: &'a RatedCharge,
}

fn encode<T: Serialize>(kind: &str, payload: T) -> Result<Vec<u8>, CodecError> {
    serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: kind.to_owned(),
        payload,
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))
}

fn typed_record<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
    expected_kind: &str,
) -> Result<T, CodecError> {
    let envelope: Envelope<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if envelope.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(envelope.schema_version));
    }
    if envelope.record_type != expected_kind {
        return Err(CodecError::UnsupportedRecordKind(envelope.record_type));
    }
    let checked: Envelope<T> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    Ok(checked.payload)
}

pub fn encode_rate_plan(plan: &RatePlan) -> Result<Vec<u8>, CodecError> {
    encode(PLAN_KIND, RatePlanRecord::from_domain(plan))
}

pub fn decode_rate_plan(bytes: &[u8]) -> Result<RatePlan, CodecError> {
    typed_record::<RatePlanRecord>(bytes, PLAN_KIND)?.checked_domain()
}

fn embedded_meter(value: &serde_json::Value) -> Result<MeterDefinition, CodecError> {
    let bytes = serde_json::to_vec(value).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    match decode_metering_fact(&bytes)? {
        MeteringFact::Definition(definition) => Ok(definition),
        MeteringFact::Event(_) => Err(CodecError::InvalidPayload(
            "expected meter definition".to_owned(),
        )),
    }
}

fn embedded_usage(value: &serde_json::Value) -> Result<UsageEvent, CodecError> {
    let bytes = serde_json::to_vec(value).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    match decode_metering_fact(&bytes)? {
        MeteringFact::Event(event) => Ok(event),
        MeteringFact::Definition(_) => Err(CodecError::InvalidPayload(
            "expected usage event".to_owned(),
        )),
    }
}

fn checked_rating_request(record: &RatingAcceptanceRecord) -> Result<RatingRequest, CodecError> {
    let definition = embedded_meter(&record.meter)?;
    let plan = record.plan.clone().checked_domain()?;
    let subject = SubjectId::new(record.subject_id.clone())
        .map_err(|e| CodecError::InvalidDomain(e.to_string()))?;
    let customer = BillingCustomerId::new(record.billing_customer_id.clone())
        .map_err(|e| CodecError::InvalidDomain(e.to_string()))?;
    let event_id = RatingEventId::new(record.rating_event_id.clone())
        .map_err(|e| CodecError::InvalidDomain(e.to_string()))?;
    let start = parse_i64_exact(&record.window_start_unix_ms)?;
    let end = parse_i64_exact(&record.window_end_unix_ms)?;
    let rated_at = parse_i64_exact(&record.rated_at_unix_ms)?;

    let mut meter = MeteringEngine::new();
    meter
        .register_meter(definition.clone())
        .map_err(|e| CodecError::Replay(e.to_string()))?;

    // A frozen acceptance cannot contain duplicate source identities, facts
    // from a different subject/window, or observations after its rated time.
    let mut seen_usage = std::collections::BTreeSet::new();
    for encoded in &record.usage_events {
        let event = embedded_usage(encoded)?;
        if !seen_usage.insert(event.id().as_str().to_owned()) {
            return Err(CodecError::Replay(
                "duplicate source usage event in accepted snapshot".to_owned(),
            ));
        }
        if event.meter_id() != definition.id()
            || event.subject_id() != &subject
            || event.occurred_at_unix_ms() < start
            || event.occurred_at_unix_ms() >= end
            || event.observed_at_unix_ms() > rated_at
        {
            return Err(CodecError::Replay(
                "source usage event is outside accepted rating evidence".to_owned(),
            ));
        }
        meter
            .ingest(event)
            .map_err(|e| CodecError::Replay(e.to_string()))?;
    }

    let aggregate = meter
        .aggregate(definition.id(), &subject, start, end)
        .map_err(|e| CodecError::Replay(e.to_string()))?;
    let request = RatingRequest::new(event_id, aggregate, customer, plan, rated_at)
        .map_err(|e| CodecError::InvalidDomain(e.to_string()))?;
    Ok(request)
}

fn reconstruct(
    record: &RatingAcceptanceRecord,
    registry: &mut RatingRegistry,
) -> Result<RatedCharge, CodecError> {
    let request = checked_rating_request(record)?;
    let outcome = registry
        .rate(request)
        .map_err(|e| CodecError::Replay(e.to_string()))?;
    let charge = match outcome {
        RatingOutcome::Rated { charge } | RatingOutcome::Replayed { charge } => charge,
    };
    if ChargeReceipt::from_domain(&charge) != record.charge {
        return Err(CodecError::Replay(
            "accepted charge differs from independently recomputed source evidence".to_owned(),
        ));
    }
    Ok(charge)
}

/// Recover the *original* rating request and verified aggregate from an
/// acceptance record. It first proves that the embedded accepted charge is
/// exactly recomputed through the canonical RatingRegistry before returning
/// the original request for downstream authorization reconstruction.
pub fn decode_accepted_rating_request(bytes: &[u8]) -> Result<RatingRequest, CodecError> {
    let record = typed_record::<RatingAcceptanceRecord>(bytes, ACCEPTANCE_KIND)?;
    reconstruct(&record, &mut RatingRegistry::new())?;
    checked_rating_request(&record)
}

/// Only construct a serialized acceptance if the supplied source facts
/// independently reconstruct the exact original accepted charge.
pub fn encode_rating_acceptance(source: &RatingAcceptance<'_>) -> Result<Vec<u8>, CodecError> {
    let meter = serde_json::from_slice(&encode_meter_definition(source.meter)?)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let usage_events = source
        .events
        .iter()
        .map(|e| {
            serde_json::from_slice(&encode_usage_event(e)?)
                .map_err(|err| CodecError::InvalidPayload(err.to_string()))
        })
        .collect::<Result<Vec<serde_json::Value>, CodecError>>()?;
    let record = RatingAcceptanceRecord {
        meter,
        usage_events,
        plan: RatePlanRecord::from_domain(source.plan),
        rating_event_id: source.rating_event_id.as_str().to_owned(),
        subject_id: source.subject_id.as_str().to_owned(),
        billing_customer_id: source.billing_customer_id.as_str().to_owned(),
        window_start_unix_ms: source.window_start_unix_ms.to_string(),
        window_end_unix_ms: source.window_end_unix_ms.to_string(),
        rated_at_unix_ms: source.rated_at_unix_ms.to_string(),
        charge: ChargeReceipt::from_domain(source.accepted_charge),
    };
    reconstruct(&record, &mut RatingRegistry::new())?;
    encode(ACCEPTANCE_KIND, record)
}

/// Replay independent frozen rating evidence bundles against the canonical
/// registry, rejecting changed duplicate identities even if totals match.
pub fn replay_rating_acceptances<'a>(
    facts: impl IntoIterator<Item = &'a [u8]>,
) -> Result<RatingRegistry, CodecError> {
    let mut registry = RatingRegistry::new();
    let mut accepted: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for bytes in facts {
        let record = typed_record::<RatingAcceptanceRecord>(bytes, ACCEPTANCE_KIND)?;
        let canonical =
            serde_json::to_vec(&record).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
        if let Some(existing) = accepted.get(&record.rating_event_id) {
            if existing != &canonical {
                return Err(CodecError::Replay(
                    "rating event identity reused with changed source snapshot".to_owned(),
                ));
            }
        }
        reconstruct(&record, &mut registry)?;
        accepted.insert(record.rating_event_id, canonical);
    }
    Ok(registry)
}
