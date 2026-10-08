//! Checked original spending approval vote source facts.
//! Restores quorum ONLY through original GovernanceEngine::approve. It
//! neither imports authorization DTOs nor posts an approved fund spend.

use crate::governance_proposal::replay_governance_proposals;
use crate::{CodecError, parse_i64_exact};
use cofi_community::{CommunityRegistry, PartyId};
use cofi_governance::{
    GovernanceEngine, SpendingApproval, SpendingApprovalEventId, SpendingApprovalId,
    SpendingProposalId,
};
use serde::{Deserialize, Serialize};

const VERSION: u64 = 1;
const KIND: &str = "governance.approval";
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
struct ApprovalRecord {
    source_event_id: String,
    id: String,
    proposal_id: String,
    approver_party_id: String,
    approved_at_unix_ms: String,
}
fn invalid<E: std::fmt::Display>(e: E) -> CodecError {
    CodecError::InvalidDomain(e.to_string())
}
impl ApprovalRecord {
    fn from_domain(a: &SpendingApproval) -> Self {
        Self {
            source_event_id: a.source_event_id().as_str().to_owned(),
            id: a.id().as_str().to_owned(),
            proposal_id: a.proposal_id().as_str().to_owned(),
            approver_party_id: a.approver_party_id().as_str().to_owned(),
            approved_at_unix_ms: a.approved_at_unix_ms().to_string(),
        }
    }
    fn checked(self) -> Result<SpendingApproval, CodecError> {
        Ok(SpendingApproval::new(
            SpendingApprovalEventId::new(self.source_event_id).map_err(invalid)?,
            SpendingApprovalId::new(self.id).map_err(invalid)?,
            SpendingProposalId::new(self.proposal_id).map_err(invalid)?,
            PartyId::new(self.approver_party_id).map_err(invalid)?,
            parse_i64_exact(&self.approved_at_unix_ms)?,
        ))
    }
}
pub fn encode_governance_approval(a: &SpendingApproval) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: ApprovalRecord::from_domain(a),
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "approval fact exceeds 1MiB".into(),
        ));
    }
    if decode_governance_approval(&bytes)? != *a {
        return Err(CodecError::Replay(
            "approval record fails checked roundtrip".into(),
        ));
    }
    Ok(bytes)
}
pub fn decode_governance_approval(bytes: &[u8]) -> Result<SpendingApproval, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "approval fact exceeds 1MiB".into(),
        ));
    }
    let env: Envelope<ApprovalRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if env.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(env.schema_version));
    }
    if env.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(env.record_type));
    }
    env.payload.checked()
}
/// Original accepted policies → proposals → votes, respecting source order.
/// Only canonical GovernanceEngine methods derive voting indexes, proposal
/// status, approver identities and ApprovedSpendingAuthorization at quorum.
pub fn replay_governance_approvals<'a, 'b, 'c>(
    policies: impl IntoIterator<Item = &'a [u8]>,
    proposals: impl IntoIterator<Item = &'b [u8]>,
    approvals: impl IntoIterator<Item = &'c [u8]>,
    community: &CommunityRegistry,
) -> Result<GovernanceEngine, CodecError> {
    let mut engine = replay_governance_proposals(policies, proposals, community)?;
    for bytes in approvals {
        engine
            .approve(community, decode_governance_approval(bytes)?)
            .map_err(|e| CodecError::Replay(e.to_string()))?;
    }
    Ok(engine)
}
