//! Versioned authorized-rating source evidence with checked reconstruction.
//!
//! This adapter NEVER creates an AuthorizedRating from a deserialized DTO.
//! It reconstructs the original subscription and original accepted rating,
//! then calls the canonical AuthorizedRatingRegistry::rate. Source stream
//! completeness, scope authentication and atomic persistence remain blocked.

use std::collections::BTreeMap;

use cofi_rating::{RatedCharge, RatingEventId, RatingRequest};
use cofi_rating_authorization::{
    AuthorizedRating, AuthorizedRatingRegistry, AuthorizedRatingRequest,
};
use cofi_subscriptions::{SubscriptionRegistry, SubscriptionRequest};
use serde::{Deserialize, Serialize};

use crate::CodecError;
use crate::rating::{decode_accepted_rating_request, replay_rating_acceptances};
use crate::subscription::{decode_subscription_request, encode_subscription_request};

const VERSION: u64 = 1;
const KIND: &str = "authorized.rating";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorizedRatingRecord {
    subscription: serde_json::Value,
    rating: serde_json::Value,
    expected_subscription_id: String,
    expected_rating_event_id: String,
    expected_organization_scope: String,
    expected_charge_id: String,
    expected_source_event_ids: Vec<String>,
}

fn typed_record(bytes: &[u8]) -> Result<AuthorizedRatingRecord, CodecError> {
    let header: Envelope<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if header.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(header.schema_version));
    }
    if header.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(header.record_type));
    }
    let parsed: Envelope<AuthorizedRatingRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    Ok(parsed.payload)
}

struct CheckedSources {
    subscription: SubscriptionRequest,
    request: RatingRequest,
    expected_charge: RatedCharge,
    authorized: AuthorizedRating,
}

fn source_event_ids(rating: &serde_json::Value) -> Result<Vec<String>, CodecError> {
    let events = rating
        .pointer("/payload/usage_events")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| CodecError::InvalidPayload("missing rated source events".to_owned()))?;
    events
        .iter()
        .map(|event| {
            event
                .pointer("/payload/id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| {
                    CodecError::InvalidPayload("missing original usage event ID".to_owned())
                })
        })
        .collect()
}

fn checked_sources(record: &AuthorizedRatingRecord) -> Result<CheckedSources, CodecError> {
    let subscription_bytes = serde_json::to_vec(&record.subscription)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let rating_bytes = serde_json::to_vec(&record.rating)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if source_event_ids(&record.rating)? != record.expected_source_event_ids {
        return Err(CodecError::Replay(
            "authorized rating source event identities changed".to_owned(),
        ));
    }
    let subscription = decode_subscription_request(&subscription_bytes)?;
    let request = decode_accepted_rating_request(&rating_bytes)?;
    let original_ratings = replay_rating_acceptances([rating_bytes.as_slice()])?;
    let expected_charge = original_ratings
        .charge_for_event(request.event_id())
        .ok_or_else(|| CodecError::Replay("accepted rating source event not present".to_owned()))?
        .clone();

    if subscription.subscription_id().as_str() != record.expected_subscription_id
        || request.event_id().as_str() != record.expected_rating_event_id
        || subscription.organization_scope().as_str() != record.expected_organization_scope
        || expected_charge.id().as_str() != record.expected_charge_id
    {
        return Err(CodecError::Replay(
            "authorized rating source lineage differs from original record".to_owned(),
        ));
    }

    let mut subscriptions = SubscriptionRegistry::new();
    subscriptions
        .create(subscription.clone())
        .map_err(|e| CodecError::Replay(e.to_string()))?;
    let mut ratings = AuthorizedRatingRegistry::new();
    let authorized_request = AuthorizedRatingRequest::new(
        request.event_id().clone(),
        subscription.organization_scope().clone(),
        request.billing_customer_id().clone(),
        request.aggregate().clone(),
        request.rated_at_unix_ms(),
    );
    let accepted = ratings
        .rate(authorized_request, &subscriptions)
        .map_err(|e| CodecError::Replay(e.to_string()))?
        .authorization()
        .clone();
    if accepted.charge() != &expected_charge {
        return Err(CodecError::Replay(
            "subscription-authorized rating differs from accepted charge receipt".to_owned(),
        ));
    }
    Ok(CheckedSources {
        subscription,
        request,
        expected_charge,
        authorized: accepted,
    })
}

/// Encodes an original authorization only after it is independently
/// reconstructed via the canonical subscription and rating-authorization
/// registries. This cannot establish upstream source completeness.
pub fn encode_authorized_rating(
    subscription: &SubscriptionRequest,
    rating_receipt: &[u8],
    accepted: &AuthorizedRating,
) -> Result<Vec<u8>, CodecError> {
    let subscription_json: serde_json::Value =
        serde_json::from_slice(&encode_subscription_request(subscription)?)
            .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let rating_json: serde_json::Value = serde_json::from_slice(rating_receipt)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let source_ids = source_event_ids(&rating_json)?;
    let record = AuthorizedRatingRecord {
        subscription: subscription_json,
        rating: rating_json,
        expected_subscription_id: subscription.subscription_id().as_str().to_owned(),
        expected_rating_event_id: accepted.charge().rating_event_id().as_str().to_owned(),
        expected_organization_scope: subscription.organization_scope().as_str().to_owned(),
        expected_charge_id: accepted.charge().id().as_str().to_owned(),
        expected_source_event_ids: source_ids,
    };
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: record,
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if decode_authorized_rating(&bytes)? != *accepted {
        return Err(CodecError::Replay(
            "accepted authorization mismatches canonical replay of source evidence".to_owned(),
        ));
    }
    Ok(bytes)
}

pub fn decode_authorized_rating(bytes: &[u8]) -> Result<AuthorizedRating, CodecError> {
    Ok(checked_sources(&typed_record(bytes)?)?.authorized)
}

/// Check the full original source snapshot identity for exact replay as well
/// as the domain's event and subscription acceptance guards. A changed
/// subscription for the same rating ID must never silently replay.
pub fn replay_authorized_ratings<'a>(
    facts: impl IntoIterator<Item = &'a [u8]>,
) -> Result<AuthorizedRatingRegistry, CodecError> {
    let mut accepted: BTreeMap<RatingEventId, Vec<u8>> = BTreeMap::new();
    let mut subscriptions = SubscriptionRegistry::new();
    let mut ratings = AuthorizedRatingRegistry::new();
    for bytes in facts {
        let record = typed_record(bytes)?;
        let checked = checked_sources(&record)?;
        let canonical =
            serde_json::to_vec(&record).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
        if let Some(existing) = accepted.get(checked.request.event_id()) {
            if existing != &canonical {
                return Err(CodecError::Replay(
                    "accepted rating identity reused with a changed authorization source"
                        .to_owned(),
                ));
            }
        }
        subscriptions
            .create(checked.subscription.clone())
            .map_err(|e| CodecError::Replay(e.to_string()))?;
        let request = AuthorizedRatingRequest::new(
            checked.request.event_id().clone(),
            checked.subscription.organization_scope().clone(),
            checked.request.billing_customer_id().clone(),
            checked.request.aggregate().clone(),
            checked.request.rated_at_unix_ms(),
        );
        let result = ratings
            .rate(request, &subscriptions)
            .map_err(|e| CodecError::Replay(e.to_string()))?;
        if result.authorization() != &checked.authorized
            || result.authorization().charge() != &checked.expected_charge
        {
            return Err(CodecError::Replay(
                "authorized rating changed under combined replay".to_owned(),
            ));
        }
        accepted.insert(checked.request.event_id().clone(), canonical);
    }
    Ok(ratings)
}
