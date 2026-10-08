//! Versioned immutable revenue split rule facts for G001.
//! The canonical CoFi RevenueSplitRule constructor owns the
//! 10_000-basis-point total, uniqueness and sorting invariants.
//! This codec does not execute a distribution or prove rule authorization.

use crate::CodecError;
use cofi_community::{BasisPoints, FundId, RevenueSplitLeg, RevenueSplitRule, RevenueSplitRuleId};
use serde::{Deserialize, Serialize};

const VERSION: u64 = 1;
const KIND: &str = "community.revenue_split_rule";
const MAX_RECORD_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredLeg {
    destination_fund_id: String,
    basis_points: u16,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRule {
    id: String,
    version: u32,
    legs: Vec<StoredLeg>,
}

/// Encode only a domain-checked immutable split rule.
pub fn encode_revenue_split_rule(rule: &RevenueSplitRule) -> Result<Vec<u8>, CodecError> {
    let legs = rule
        .legs()
        .iter()
        .map(|leg| StoredLeg {
            destination_fund_id: leg.destination_fund_id().as_str().to_owned(),
            basis_points: leg.basis_points().get(),
        })
        .collect();
    let envelope = Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: StoredRule {
            id: rule.id().as_str().to_owned(),
            version: rule.version(),
            legs,
        },
    };
    let bytes =
        serde_json::to_vec(&envelope).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(CodecError::InvalidPayload(
            "revenue split rule record exceeds 1MiB".to_owned(),
        ));
    }
    Ok(bytes)
}

/// Reconstitute via checked BasisPoints and RevenueSplitRule constructors.
/// No unchecked rule or private domain state is imported from JSON.
pub fn decode_revenue_split_rule(bytes: &[u8]) -> Result<RevenueSplitRule, CodecError> {
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(CodecError::InvalidPayload(
            "revenue split rule record exceeds 1MiB".to_owned(),
        ));
    }
    let envelope: Envelope<StoredRule> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if envelope.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(envelope.schema_version));
    }
    if envelope.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(envelope.record_type));
    }
    let rule = envelope.payload;
    let legs = rule
        .legs
        .into_iter()
        .map(|leg| {
            let fund = FundId::new(leg.destination_fund_id)
                .map_err(|e| CodecError::InvalidDomain(e.to_string()))?;
            let basis = BasisPoints::new(leg.basis_points)
                .map_err(|e| CodecError::InvalidDomain(e.to_string()))?;
            Ok(RevenueSplitLeg::new(fund, basis))
        })
        .collect::<Result<Vec<_>, CodecError>>()?;
    RevenueSplitRule::new(
        RevenueSplitRuleId::new(rule.id).map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
        rule.version,
        legs,
    )
    .map_err(|e| CodecError::InvalidDomain(e.to_string()))
}
