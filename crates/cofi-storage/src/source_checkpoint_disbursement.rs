//! Bounded source-order consistency for a caller-declared genesis lifecycle.
//!
//! No disbursement creation event contains a spend ID or a tenant identity.
//! This check cannot prove creation-to-spend causality, tenant attribution,
//! provider authenticity, immutable custody or source stream completeness.
//! Exact domain replay with independently authenticated source remains required.

use std::collections::{BTreeMap, BTreeSet};

use cofi_disbursements::TerminalKind;

use crate::CodecError;
use crate::disbursement_creation::decode_disbursement_creation;
use crate::disbursement_lifecycle::{decode_disbursement_submission, decode_disbursement_terminal};
use crate::source_checkpoint::SupportedSourceKind;

#[derive(Default)]
pub(crate) struct GenesisDisbursementLineage {
    stages: BTreeMap<String, Stage>,
    provider_requests: BTreeSet<String>,
    provider_events: BTreeSet<String>,
    provider_settlements: BTreeSet<String>,
}

enum Stage {
    Created(i64),
    Submitted(i64),
    Terminal,
}

fn invalid(reason: &str) -> CodecError {
    CodecError::Replay(reason.to_owned())
}

impl GenesisDisbursementLineage {
    pub(crate) fn check(
        &mut self,
        kind: SupportedSourceKind,
        bytes: &[u8],
    ) -> Result<(), CodecError> {
        match kind {
            SupportedSourceKind::Creation => {
                let creation = decode_disbursement_creation(bytes)?;
                if self
                    .stages
                    .insert(
                        creation.id().as_str().to_owned(),
                        Stage::Created(creation.created_at_unix_ms()),
                    )
                    .is_some()
                {
                    return Err(invalid("duplicate original disbursement creation identity"));
                }
            }
            SupportedSourceKind::Submission => {
                let submission = decode_disbursement_submission(bytes)?;
                let stage = self
                    .stages
                    .get_mut(submission.disbursement_id().as_str())
                    .ok_or_else(|| invalid("genesis submission has no earlier creation"))?;
                match stage {
                    Stage::Created(created_at)
                        if submission.submitted_at_unix_ms() >= *created_at => {}
                    Stage::Created(_) => {
                        return Err(invalid("genesis submission before creation timestamp"));
                    }
                    Stage::Submitted(_) | Stage::Terminal => {
                        return Err(invalid("duplicate or late genesis submission transition"));
                    }
                }
                if !self
                    .provider_requests
                    .insert(submission.provider_request_reference().as_str().to_owned())
                {
                    return Err(invalid(
                        "reused original provider request reference in genesis",
                    ));
                }
                *stage = Stage::Submitted(submission.submitted_at_unix_ms());
            }
            SupportedSourceKind::Terminal => {
                let terminal = decode_disbursement_terminal(bytes)?;
                let stage = self
                    .stages
                    .get_mut(terminal.disbursement_id().as_str())
                    .ok_or_else(|| {
                        invalid("genesis terminal has no earlier creation/submission")
                    })?;
                match stage {
                    Stage::Submitted(at) if terminal.terminal_at_unix_ms() >= *at => {}
                    Stage::Submitted(_) => {
                        return Err(invalid("genesis terminal before submission timestamp"));
                    }
                    Stage::Created(_) => {
                        return Err(invalid("genesis terminal before original submission"));
                    }
                    Stage::Terminal => {
                        return Err(invalid("duplicate genesis terminal transition"));
                    }
                }
                if !self
                    .provider_events
                    .insert(terminal.provider_event_reference().as_str().to_owned())
                {
                    return Err(invalid(
                        "reused original provider terminal event reference in genesis",
                    ));
                }
                if let TerminalKind::Settled {
                    settlement_reference,
                } = terminal.kind()
                {
                    if !self
                        .provider_settlements
                        .insert(settlement_reference.as_str().to_owned())
                    {
                        return Err(invalid(
                            "reused original provider settlement reference in genesis",
                        ));
                    }
                }
                *stage = Stage::Terminal;
            }
            SupportedSourceKind::Policy
            | SupportedSourceKind::Proposal
            | SupportedSourceKind::Approval
            | SupportedSourceKind::FundSpend => {}
        }
        Ok(())
    }
}
