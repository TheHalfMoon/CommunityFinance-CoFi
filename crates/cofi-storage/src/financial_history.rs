//! Checked replay of one caller-asserted accepted governance and disbursement stream.
//!
//! This checks cross-domain causal order, NOT an independently authenticated
//! tenant event log, an external source witness, or a durable database boundary.
//! Only canonical domain engines rebuild authority and disbursement state.
//! No ledger posting, network call, provider dispatch or money effect occurs.

use std::collections::{BTreeMap, BTreeSet};

use cofi_community::CommunityRegistry;
use cofi_disbursements::{CreationOutcome, DisbursementEngine, SubmissionOutcome, TerminalOutcome};
use cofi_governance::{ApprovalOutcome, GovernanceEngine, ProposalSubmissionOutcome};
use cofi_ledger::Ledger;
use cofi_spending::{ApprovedFundSpendEvent, FundSpendBridge, VerifiedFundSpend};

use crate::CodecError;
use crate::disbursement_creation::decode_disbursement_creation;
use crate::disbursement_lifecycle::{decode_disbursement_submission, decode_disbursement_terminal};
use crate::governance_approval::decode_governance_approval;
use crate::governance_fund_spend::decode_governance_fund_spend;
use crate::governance_policy::decode_governance_policy;
use crate::governance_proposal::decode_governance_proposal;

/// One accepted historical source fact in caller-asserted inter-kind order.
/// Creation refers to the source ID of a *previously encountered* fund spend.
#[derive(Debug, Clone, Copy)]
pub enum AcceptedFinancialFact<'a> {
    Policy(&'a [u8]),
    Proposal(&'a [u8]),
    Approval(&'a [u8]),
    FundSpend(&'a [u8]),
    Creation {
        creation: &'a [u8],
        spend_source_event_id: &'a str,
    },
    Submission(&'a [u8]),
    Terminal(&'a [u8]),
}

/// Domain-native state rebuilt from an internally consistent supplied stream.
#[derive(Debug)]
pub struct VerifiedFinancialHistory {
    governance: GovernanceEngine,
    disbursements: DisbursementEngine,
    verified_spends: Vec<VerifiedFundSpend>,
}

impl VerifiedFinancialHistory {
    #[must_use]
    pub const fn governance(&self) -> &GovernanceEngine {
        &self.governance
    }

    #[must_use]
    pub const fn disbursements(&self) -> &DisbursementEngine {
        &self.disbursements
    }

    #[must_use]
    pub fn verified_spends(&self) -> &[VerifiedFundSpend] {
        &self.verified_spends
    }
}

/// Fail closed on impossible cross-domain accepted ordering and repeated facts.
///
/// This does not prove source completeness, real-world chronology, custody,
/// provider signatures, reference ledger authenticity, or tenant root/cutoff.
/// No original domain command retry is changed: only a duplicated *accepted
/// source fact* is rejected when the original domain reports Replayed.
pub fn replay_accepted_financial_history<'a>(
    facts: impl IntoIterator<Item = AcceptedFinancialFact<'a>>,
    community: &CommunityRegistry,
    ledger: &Ledger,
) -> Result<VerifiedFinancialHistory, CodecError> {
    let mut governance = GovernanceEngine::new();
    let mut disbursements = DisbursementEngine::new();
    let mut verified_spends = Vec::new();
    let mut accepted_spend_sources: BTreeMap<String, ApprovedFundSpendEvent> = BTreeMap::new();
    let mut spend_ids = BTreeSet::new();
    let mut consumed_proposals = BTreeSet::new();

    for fact in facts {
        match fact {
            AcceptedFinancialFact::Policy(bytes) => {
                governance
                    .register_policy(community, decode_governance_policy(bytes)?)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
            }
            AcceptedFinancialFact::Proposal(bytes) => {
                let outcome = governance
                    .submit_proposal(community, decode_governance_proposal(bytes)?)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
                if outcome != ProposalSubmissionOutcome::Submitted {
                    return Err(CodecError::Replay(
                        "duplicate accepted governance proposal fact".into(),
                    ));
                }
            }
            AcceptedFinancialFact::Approval(bytes) => {
                let outcome = governance
                    .approve(community, decode_governance_approval(bytes)?)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
                if !matches!(outcome, ApprovalOutcome::Recorded { .. }) {
                    return Err(CodecError::Replay(
                        "duplicate accepted governance approval fact".into(),
                    ));
                }
            }
            AcceptedFinancialFact::FundSpend(bytes) => {
                let spend = decode_governance_fund_spend(bytes)?;
                let source = spend.source_event_id().as_str().to_owned();
                if accepted_spend_sources.contains_key(&source)
                    || spend_ids.contains(spend.spend_id().as_str())
                    || consumed_proposals.contains(spend.proposal_id().as_str())
                {
                    return Err(CodecError::Replay(
                        "duplicate accepted spend source, spend ID or consumed proposal".into(),
                    ));
                }
                let receipt = FundSpendBridge::new()
                    .verify_committed(community, &governance, &spend, ledger)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
                spend_ids.insert(spend.spend_id().as_str().to_owned());
                consumed_proposals.insert(spend.proposal_id().as_str().to_owned());
                accepted_spend_sources.insert(source, spend);
                verified_spends.push(receipt);
            }
            AcceptedFinancialFact::Creation {
                creation,
                spend_source_event_id,
            } => {
                let spend = accepted_spend_sources
                    .get(spend_source_event_id)
                    .ok_or_else(|| {
                        CodecError::Replay(
                            "disbursement creation precedes its accepted spend source".into(),
                        )
                    })?;
                let outcome = disbursements
                    .create(
                        community,
                        &governance,
                        ledger,
                        spend,
                        decode_disbursement_creation(creation)?,
                    )
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
                if outcome != CreationOutcome::Created {
                    return Err(CodecError::Replay(
                        "duplicate accepted disbursement creation fact".into(),
                    ));
                }
            }
            AcceptedFinancialFact::Submission(bytes) => {
                let outcome = disbursements
                    .submit(decode_disbursement_submission(bytes)?)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
                if outcome != SubmissionOutcome::Submitted {
                    return Err(CodecError::Replay(
                        "duplicate accepted disbursement submission fact".into(),
                    ));
                }
            }
            AcceptedFinancialFact::Terminal(bytes) => {
                let outcome = disbursements
                    .record_terminal(decode_disbursement_terminal(bytes)?)
                    .map_err(|e| CodecError::Replay(e.to_string()))?;
                if !matches!(outcome, TerminalOutcome::Settled | TerminalOutcome::Failed) {
                    return Err(CodecError::Replay(
                        "duplicate accepted disbursement terminal fact".into(),
                    ));
                }
            }
        }
    }

    Ok(VerifiedFinancialHistory {
        governance,
        disbursements,
        verified_spends,
    })
}
