//! Checked immutable governance proposal source facts.
//! Rebuild via the original SpendingProposal constructor and GovernanceEngine.
//! No vote, approval, authorization, or money effect is synthesized.

use crate::governance_policy::replay_governance_policies;
use crate::{CodecError, parse_i64_exact, parse_i128_exact};
use cofi_community::{CommunityId, CommunityRegistry, FundId, OrganizationId, PartyId};
use cofi_governance::{
    GovernanceEngine, SpendingApprovalPolicyId, SpendingProposal, SpendingProposalEventId,
    SpendingProposalId,
};
use cofi_ledger::Currency;
use serde::{Deserialize, Serialize};

const VERSION: u64 = 1;
const KIND: &str = "governance.proposal";
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    source_event_id: String,
    id: String,
    policy_id: String,
    policy_version: u32,
    requester_party_id: String,
    organization_id: String,
    community_id: String,
    fund_id: String,
    currency: String,
    amount_minor: String,
    purpose_reference: String,
    created_at_unix_ms: String,
    expires_at_unix_ms: String,
}
fn invalid<E: std::fmt::Display>(error: E) -> CodecError {
    CodecError::InvalidDomain(error.to_string())
}
impl Record {
    fn from_domain(p: &SpendingProposal) -> Self {
        Self {
            source_event_id: p.source_event_id().as_str().to_owned(),
            id: p.id().as_str().to_owned(),
            policy_id: p.policy_id().as_str().to_owned(),
            policy_version: p.policy_version(),
            requester_party_id: p.requester_party_id().as_str().to_owned(),
            organization_id: p.organization_id().as_str().to_owned(),
            community_id: p.community_id().as_str().to_owned(),
            fund_id: p.fund_id().as_str().to_owned(),
            currency: p.currency().code().to_owned(),
            amount_minor: p.amount_minor().to_string(),
            purpose_reference: p.purpose_reference().to_owned(),
            created_at_unix_ms: p.created_at_unix_ms().to_string(),
            expires_at_unix_ms: p.expires_at_unix_ms().to_string(),
        }
    }
    fn checked(self) -> Result<SpendingProposal, CodecError> {
        SpendingProposal::new(
            SpendingProposalEventId::new(self.source_event_id).map_err(invalid)?,
            SpendingProposalId::new(self.id).map_err(invalid)?,
            SpendingApprovalPolicyId::new(self.policy_id).map_err(invalid)?,
            self.policy_version,
            PartyId::new(self.requester_party_id).map_err(invalid)?,
            OrganizationId::new(self.organization_id).map_err(invalid)?,
            CommunityId::new(self.community_id).map_err(invalid)?,
            FundId::new(self.fund_id).map_err(invalid)?,
            Currency::new(&self.currency).map_err(invalid)?,
            parse_i128_exact(&self.amount_minor)?,
            self.purpose_reference,
            parse_i64_exact(&self.created_at_unix_ms)?,
            parse_i64_exact(&self.expires_at_unix_ms)?,
        )
        .map_err(invalid)
    }
}
pub fn encode_governance_proposal(p: &SpendingProposal) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: Record::from_domain(p),
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload("proposal exceeds 1MiB".into()));
    }
    if decode_governance_proposal(&bytes)? != *p {
        return Err(CodecError::Replay(
            "proposal source roundtrip mismatch".into(),
        ));
    }
    Ok(bytes)
}
pub fn decode_governance_proposal(bytes: &[u8]) -> Result<SpendingProposal, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload("proposal exceeds 1MiB".into()));
    }
    let envelope: Envelope<Record> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if envelope.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(envelope.schema_version));
    }
    if envelope.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(envelope.record_type));
    }
    envelope.payload.checked()
}
/// Reapply original policy registrations and proposal submissions in
/// supplied causal order using the original GovernanceEngine.
pub fn replay_governance_proposals<'a, 'b>(
    policies: impl IntoIterator<Item = &'a [u8]>,
    proposals: impl IntoIterator<Item = &'b [u8]>,
    registry: &CommunityRegistry,
) -> Result<GovernanceEngine, CodecError> {
    let mut engine = replay_governance_policies(policies, registry)?;
    for fact in proposals {
        engine
            .submit_proposal(registry, decode_governance_proposal(fact)?)
            .map_err(|e| CodecError::Replay(e.to_string()))?;
    }
    Ok(engine)
}
