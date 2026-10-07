//! Checked, versioned subscription creation facts.
//!
//! The immutable accepted request is the fact. Subscription status, schedule
//! indexes and interval resolution are always derived by the original registry.
//! Production sequencing, tenant isolation and authenticated source lineage
//! are still G002-G004 blockers.

use cofi_billing::BillingCustomerId;
use cofi_ledger::LedgerScopeId;
use cofi_metering::SubjectId;
use cofi_subscriptions::{
    SubscriptionEventId, SubscriptionId, SubscriptionRegistry, SubscriptionRequest,
};
use serde::{Deserialize, Serialize};

use crate::rating::{decode_rate_plan, encode_rate_plan};
use crate::{CodecError, parse_i64_exact};

const VERSION: u64 = 1;
const CREATE_KIND: &str = "subscription.create";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubscriptionCreateRecord {
    event_id: String,
    subscription_id: String,
    organization_scope: String,
    billing_customer_id: String,
    subject_id: String,
    plan: serde_json::Value,
    active_from_unix_ms: String,
    active_until_unix_ms: Option<String>,
}

pub fn encode_subscription_request(request: &SubscriptionRequest) -> Result<Vec<u8>, CodecError> {
    let plan = serde_json::from_slice(&encode_rate_plan(request.plan())?)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let record = SubscriptionCreateRecord {
        event_id: request.event_id().as_str().to_owned(),
        subscription_id: request.subscription_id().as_str().to_owned(),
        organization_scope: request.organization_scope().as_str().to_owned(),
        billing_customer_id: request.billing_customer_id().as_str().to_owned(),
        subject_id: request.subject_id().as_str().to_owned(),
        plan,
        active_from_unix_ms: request.active_from_unix_ms().to_string(),
        active_until_unix_ms: request.active_until_unix_ms().map(|v| v.to_string()),
    };
    serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: CREATE_KIND.to_owned(),
        payload: record,
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))
}

pub fn decode_subscription_request(bytes: &[u8]) -> Result<SubscriptionRequest, CodecError> {
    let header: Envelope<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if header.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(header.schema_version));
    }
    if header.record_type != CREATE_KIND {
        return Err(CodecError::UnsupportedRecordKind(header.record_type));
    }
    let record: Envelope<SubscriptionCreateRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let payload = record.payload;
    let plan_bytes =
        serde_json::to_vec(&payload.plan).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    SubscriptionRequest::new(
        SubscriptionEventId::new(payload.event_id)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        SubscriptionId::new(payload.subscription_id)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        LedgerScopeId::new(payload.organization_scope)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        BillingCustomerId::new(payload.billing_customer_id)
            .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        SubjectId::new(payload.subject_id).map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        decode_rate_plan(&plan_bytes)?,
        parse_i64_exact(&payload.active_from_unix_ms)?,
        payload
            .active_until_unix_ms
            .as_deref()
            .map(parse_i64_exact)
            .transpose()?,
    )
    .map_err(|e| CodecError::InvalidDomain(e.to_string()))
}

/// Recreate every accepted immutable schedule fact through the original
/// registry. Exact replay is harmless; identity or interval conflicts fail.
pub fn replay_subscriptions<'a>(
    facts: impl IntoIterator<Item = &'a [u8]>,
) -> Result<SubscriptionRegistry, CodecError> {
    let mut registry = SubscriptionRegistry::new();
    for record in facts {
        let request = decode_subscription_request(record)?;
        registry
            .create(request)
            .map_err(|e| CodecError::Replay(e.to_string()))?;
    }
    Ok(registry)
}
