//! Checked immutable spending approval policy source facts.
//!
//! The original GovernanceEngine owns version monotonicity and the original
//! CommunityRegistry validates community/fund/org/currency linkage.
//! A registered policy alone is NOT an approved proposal or a spend mandate.

use std::collections::BTreeMap;

use cofi_community::{CommunityId, CommunityRegistry, FundId, MembershipRole, OrganizationId};
use cofi_governance::{GovernanceEngine, SpendingApprovalPolicy, SpendingApprovalPolicyId};
use cofi_ledger::Currency;
use serde::{Deserialize, Serialize};

use crate::{CodecError, parse_i128_exact};

const VERSION: u64 = 1;
const KIND: &str = "governance.policy";
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    Owner,
    Admin,
    Treasurer,
    Member,
}
impl From<MembershipRole> for Role {
    fn from(value: MembershipRole) -> Self {
        match value {
            MembershipRole::Owner => Self::Owner,
            MembershipRole::Admin => Self::Admin,
            MembershipRole::Treasurer => Self::Treasurer,
            MembershipRole::Member => Self::Member,
        }
    }
}
impl From<Role> for MembershipRole {
    fn from(value: Role) -> Self {
        match value {
            Role::Owner => Self::Owner,
            Role::Admin => Self::Admin,
            Role::Treasurer => Self::Treasurer,
            Role::Member => Self::Member,
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyRecord {
    id: String,
    version: u32,
    organization_id: String,
    community_id: String,
    fund_id: String,
    currency: String,
    max_amount_minor: String,
    required_approvals: u16,
    eligible_roles: Vec<Role>,
}
fn invalid<E: std::fmt::Display>(error: E) -> CodecError {
    CodecError::InvalidDomain(error.to_string())
}
impl PolicyRecord {
    fn from_domain(p: &SpendingApprovalPolicy) -> Self {
        Self {
            id: p.id().as_str().to_owned(),
            version: p.version(),
            organization_id: p.organization_id().as_str().to_owned(),
            community_id: p.community_id().as_str().to_owned(),
            fund_id: p.fund_id().as_str().to_owned(),
            currency: p.currency().code().to_owned(),
            max_amount_minor: p.max_amount_minor().to_string(),
            required_approvals: p.required_approvals(),
            eligible_roles: p.eligible_roles().iter().copied().map(Into::into).collect(),
        }
    }
    fn checked(self) -> Result<SpendingApprovalPolicy, CodecError> {
        SpendingApprovalPolicy::new(
            SpendingApprovalPolicyId::new(self.id).map_err(invalid)?,
            self.version,
            OrganizationId::new(self.organization_id).map_err(invalid)?,
            CommunityId::new(self.community_id).map_err(invalid)?,
            FundId::new(self.fund_id).map_err(invalid)?,
            Currency::new(&self.currency).map_err(invalid)?,
            parse_i128_exact(&self.max_amount_minor)?,
            self.required_approvals,
            self.eligible_roles.into_iter().map(Into::into).collect(),
        )
        .map_err(invalid)
    }
}

/// Versioned immutable accepted policy snapshot; all fields re-checked
/// against the original domain constructor, not private map imports.
pub fn encode_governance_policy(policy: &SpendingApprovalPolicy) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: PolicyRecord::from_domain(policy),
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "governance policy exceeds 1MiB".to_owned(),
        ));
    }
    if decode_governance_policy(&bytes)? != *policy {
        return Err(CodecError::Replay(
            "governance policy is not canonical".to_owned(),
        ));
    }
    Ok(bytes)
}

pub fn decode_governance_policy(bytes: &[u8]) -> Result<SpendingApprovalPolicy, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "governance policy exceeds 1MiB".to_owned(),
        ));
    }
    let env: Envelope<PolicyRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if env.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(env.schema_version));
    }
    if env.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(env.record_type));
    }
    let wire_roles: Vec<MembershipRole> = env
        .payload
        .eligible_roles
        .iter()
        .copied()
        .map(Into::into)
        .collect();
    let policy = env.payload.checked()?;
    if policy.eligible_roles() != wire_roles.as_slice() {
        return Err(CodecError::InvalidPayload(
            "policy eligible roles must be unique and in original canonical order".into(),
        ));
    }
    Ok(policy)
}

/// Replay supplied policy registrations in their original accepted order.
/// The genuine GovernanceEngine enforces version monotonicity and links to
/// the independently restored CommunityRegistry. Changed same ID/version
/// is refused; byte-equivalent duplicate records are idempotent.
///
/// This cannot authenticate input completeness, cutoff or tenant identity.
pub fn replay_governance_policies<'a>(
    records: impl IntoIterator<Item = &'a [u8]>,
    community: &CommunityRegistry,
) -> Result<GovernanceEngine, CodecError> {
    let mut governance = GovernanceEngine::new();
    let mut accepted: BTreeMap<(String, u32), Vec<u8>> = BTreeMap::new();
    for bytes in records {
        let policy = decode_governance_policy(bytes)?;
        let key = (policy.id().as_str().to_owned(), policy.version());
        let canonical = encode_governance_policy(&policy)?;
        if let Some(previous) = accepted.get(&key) {
            if previous != &canonical {
                return Err(CodecError::Replay(
                    "policy ID/version reused with altered terms".into(),
                ));
            }
            continue;
        }
        governance
            .register_policy(community, policy)
            .map_err(|e| CodecError::Replay(e.to_string()))?;
        accepted.insert(key, canonical);
    }
    Ok(governance)
}
