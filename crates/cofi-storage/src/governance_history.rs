//! Read-only replay of a caller-supplied, causally ordered governance history.
//!
//! This adapter removes the grouped-stream ordering blind spot in the G001
//! governance codecs. It is NOT source authentication, an external witness,
//! a tenant-global accepted sequence, or a durable consumed-authority index.
//! The caller must independently prove source identity, order and completeness.

use std::collections::BTreeSet;

use cofi_community::CommunityRegistry;
use cofi_governance::{ApprovalOutcome, GovernanceEngine, ProposalSubmissionOutcome};
use cofi_ledger::Ledger;
use cofi_spending::VerifiedFundSpend;

use crate::CodecError;
use crate::governance_approval::decode_governance_approval;
use crate::governance_fund_spend::decode_governance_fund_spend;
use crate::governance_policy::decode_governance_policy;
use crate::governance_proposal::decode_governance_proposal;

/// An accepted source fact at its asserted original position in one stream.
/// Unlike idempotent client commands, repeated facts are not accepted here.
#[derive(Debug, Clone, Copy)]
pub enum GovernanceHistoryFact<'a> {
    Policy(&'a [u8]),
    Proposal(&'a [u8]),
    Approval(&'a [u8]),
    FundSpend(&'a [u8]),
}

/// Original checked governance projections and read-only verified spend receipts.
#[derive(Debug)]
pub struct VerifiedGovernanceHistory {
    governance: GovernanceEngine,
    verified_spends: Vec<VerifiedFundSpend>,
}

impl VerifiedGovernanceHistory {
    #[must_use]
    pub const fn governance(&self) -> &GovernanceEngine {
        &self.governance
    }

    #[must_use]
    pub fn verified_spends(&self) -> &[VerifiedFundSpend] {
        &self.verified_spends
    }
}

/// Reconstruct policy/proposal/vote admission and verify preexisting spending
/// journals in the *supplied* inter-kind order, using only original domain APIs.
///
/// The returned state does not attest that omitted or coherently forged records
/// were discovered. An independently trusted, durable tenant source checkpoint
/// is a separate G001/G002+ prerequisite before production use.
pub fn verify_ordered_governance_history<'a>(
    facts: impl IntoIterator<Item = GovernanceHistoryFact<'a>>,
    community: &CommunityRegistry,
    ledger: &Ledger,
) -> Result<VerifiedGovernanceHistory, CodecError> {
    let mut governance = GovernanceEngine::new();
    let mut verified_spends = Vec::new();
    let mut spend_sources = BTreeSet::new();
    let mut spend_ids = BTreeSet::new();
    let mut consumed_proposals = BTreeSet::new();

    for fact in facts {
        match fact {
            GovernanceHistoryFact::Policy(bytes) => {
                let policy = decode_governance_policy(bytes)?;
                governance
                    .register_policy(community, policy)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
            }
            GovernanceHistoryFact::Proposal(bytes) => {
                let proposal = decode_governance_proposal(bytes)?;
                let result = governance
                    .submit_proposal(community, proposal)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
                if result != ProposalSubmissionOutcome::Submitted {
                    return Err(CodecError::Replay(
                        "duplicate accepted governance proposal fact".into(),
                    ));
                }
            }
            GovernanceHistoryFact::Approval(bytes) => {
                let approval = decode_governance_approval(bytes)?;
                let result = governance
                    .approve(community, approval)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
                if !matches!(result, ApprovalOutcome::Recorded { .. }) {
                    return Err(CodecError::Replay(
                        "duplicate accepted governance approval fact".into(),
                    ));
                }
            }
            GovernanceHistoryFact::FundSpend(bytes) => {
                let spend = decode_governance_fund_spend(bytes)?;
                if !spend_sources.insert(spend.source_event_id().as_str().to_owned())
                    || !spend_ids.insert(spend.spend_id().as_str().to_owned())
                    || !consumed_proposals.insert(spend.proposal_id().as_str().to_owned())
                {
                    return Err(CodecError::Replay(
                        "duplicate accepted spend source, spend ID or consumed proposal".into(),
                    ));
                }
                let receipt = cofi_spending::FundSpendBridge::new()
                    .verify_committed(community, &governance, &spend, ledger)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
                verified_spends.push(receipt);
            }
        }
    }

    Ok(VerifiedGovernanceHistory {
        governance,
        verified_spends,
    })
}
