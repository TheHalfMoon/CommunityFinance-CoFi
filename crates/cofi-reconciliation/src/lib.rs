use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_disbursements::{
    Disbursement, DisbursementId, DisbursementStatus, DisbursementTerminalEvent,
    ProviderRequestReference,
};
use cofi_provider_contract::{
    ProviderContract, ProviderContractError, ProviderObservation, ProviderObservationKind,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReconciliationCaseId(String);

impl ReconciliationCaseId {
    pub fn new(value: impl Into<String>) -> Result<Self, ReconciliationError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ReconciliationError::EmptyIdentifier(
                "reconciliation_case_id",
            ));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationCase {
    id: ReconciliationCaseId,
    disbursement_id: DisbursementId,
    provider_request_reference: ProviderRequestReference,
    reconciled_at_unix_ms: i64,
}

impl ReconciliationCase {
    #[must_use]
    pub const fn new(
        id: ReconciliationCaseId,
        disbursement_id: DisbursementId,
        provider_request_reference: ProviderRequestReference,
        reconciled_at_unix_ms: i64,
    ) -> Self {
        Self {
            id,
            disbursement_id,
            provider_request_reference,
            reconciled_at_unix_ms,
        }
    }
    #[must_use]
    pub const fn id(&self) -> &ReconciliationCaseId {
        &self.id
    }
    #[must_use]
    pub const fn disbursement_id(&self) -> &DisbursementId {
        &self.disbursement_id
    }
    #[must_use]
    pub const fn provider_request_reference(&self) -> &ProviderRequestReference {
        &self.provider_request_reference
    }
    #[must_use]
    pub const fn reconciled_at_unix_ms(&self) -> i64 {
        self.reconciled_at_unix_ms
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscrepancyKind {
    AcceptedAfterTerminal,
    TerminalStatusMismatch,
    ProviderEventReferenceMismatch,
    SettlementReferenceMismatch,
    FailureCodeMismatch,
    TerminalTimestampMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconciliationOutcome {
    PendingAgreement {
        case_id: ReconciliationCaseId,
    },
    ProviderAhead {
        case_id: ReconciliationCaseId,
        proposed_terminal_event: DisbursementTerminalEvent,
    },
    TerminalAgreement {
        case_id: ReconciliationCaseId,
        status: DisbursementStatus,
    },
    Discrepancy {
        case_id: ReconciliationCaseId,
        kind: DiscrepancyKind,
    },
}
#[derive(Debug, Clone, Copy, Default)]
pub struct ReconciliationEngine;

impl ReconciliationEngine {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn reconcile(
        &self,
        case: &ReconciliationCase,
        disbursement: &Disbursement,
        observation: &ProviderObservation,
    ) -> Result<ReconciliationOutcome, ReconciliationError> {
        if disbursement.status() == DisbursementStatus::Ready {
            return Err(ReconciliationError::ReadyDisbursement);
        }
        if case.disbursement_id() != disbursement.id() {
            return Err(ReconciliationError::CaseBindingMismatch("disbursement_id"));
        }
        if disbursement.provider_request_reference() != Some(case.provider_request_reference()) {
            return Err(ReconciliationError::CaseBindingMismatch(
                "provider_request_reference",
            ));
        }
        ProviderContract::new()
            .validate_observation(disbursement, observation)
            .map_err(ReconciliationError::ProviderContract)?;

        if case.reconciled_at_unix_ms() < observation.occurred_at_unix_ms() {
            return Err(ReconciliationError::ReconciliationBeforeObservation {
                reconciled_at_unix_ms: case.reconciled_at_unix_ms(),
                observation_at_unix_ms: observation.occurred_at_unix_ms(),
            });
        }

        match disbursement.status() {
            DisbursementStatus::Ready => Err(ReconciliationError::ReadyDisbursement),
            DisbursementStatus::Submitted => {
                self.reconcile_submitted(case, disbursement, observation)
            }
            DisbursementStatus::Settled | DisbursementStatus::Failed => {
                self.reconcile_terminal(case, disbursement, observation)
            }
        }
    }
    fn reconcile_submitted(
        &self,
        case: &ReconciliationCase,
        disbursement: &Disbursement,
        observation: &ProviderObservation,
    ) -> Result<ReconciliationOutcome, ReconciliationError> {
        match observation.kind() {
            ProviderObservationKind::Accepted { .. } => {
                Ok(ReconciliationOutcome::PendingAgreement {
                    case_id: case.id().clone(),
                })
            }
            ProviderObservationKind::Settled { .. } | ProviderObservationKind::Failed { .. } => {
                let proposed_terminal_event = ProviderContract::new()
                    .to_terminal_event(disbursement, observation)
                    .map_err(ReconciliationError::ProviderContract)?;
                Ok(ReconciliationOutcome::ProviderAhead {
                    case_id: case.id().clone(),
                    proposed_terminal_event,
                })
            }
        }
    }

    fn reconcile_terminal(
        &self,
        case: &ReconciliationCase,
        disbursement: &Disbursement,
        observation: &ProviderObservation,
    ) -> Result<ReconciliationOutcome, ReconciliationError> {
        let discrepancy = |kind| ReconciliationOutcome::Discrepancy {
            case_id: case.id().clone(),
            kind,
        };

        match observation.kind() {
            ProviderObservationKind::Accepted { .. } => {
                Ok(discrepancy(DiscrepancyKind::AcceptedAfterTerminal))
            }
            ProviderObservationKind::Settled {
                provider_event_reference,
                settlement_reference,
                ..
            } => {
                if disbursement.status() != DisbursementStatus::Settled {
                    return Ok(discrepancy(DiscrepancyKind::TerminalStatusMismatch));
                }
                if disbursement.provider_event_reference() != Some(provider_event_reference) {
                    return Ok(discrepancy(DiscrepancyKind::ProviderEventReferenceMismatch));
                }
                if disbursement.provider_settlement_reference() != Some(settlement_reference) {
                    return Ok(discrepancy(DiscrepancyKind::SettlementReferenceMismatch));
                }
                if disbursement.terminal_at_unix_ms() != Some(observation.occurred_at_unix_ms()) {
                    return Ok(discrepancy(DiscrepancyKind::TerminalTimestampMismatch));
                }
                Ok(ReconciliationOutcome::TerminalAgreement {
                    case_id: case.id().clone(),
                    status: DisbursementStatus::Settled,
                })
            }
            ProviderObservationKind::Failed {
                provider_event_reference,
                failure_code,
                ..
            } => {
                if disbursement.status() != DisbursementStatus::Failed {
                    return Ok(discrepancy(DiscrepancyKind::TerminalStatusMismatch));
                }
                if disbursement.provider_event_reference() != Some(provider_event_reference) {
                    return Ok(discrepancy(DiscrepancyKind::ProviderEventReferenceMismatch));
                }
                if disbursement.failure_code() != Some(failure_code) {
                    return Ok(discrepancy(DiscrepancyKind::FailureCodeMismatch));
                }
                if disbursement.terminal_at_unix_ms() != Some(observation.occurred_at_unix_ms()) {
                    return Ok(discrepancy(DiscrepancyKind::TerminalTimestampMismatch));
                }
                Ok(ReconciliationOutcome::TerminalAgreement {
                    case_id: case.id().clone(),
                    status: DisbursementStatus::Failed,
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconciliationError {
    EmptyIdentifier(&'static str),
    ProviderContract(ProviderContractError),
    CaseBindingMismatch(&'static str),
    ReadyDisbursement,
    ReconciliationBeforeObservation {
        reconciled_at_unix_ms: i64,
        observation_at_unix_ms: i64,
    },
}
impl Display for ReconciliationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(label) => write!(formatter, "{label} must not be empty"),
            Self::ProviderContract(error) => write!(formatter, "provider contract: {error}"),
            Self::CaseBindingMismatch(field) => {
                write!(
                    formatter,
                    "reconciliation case {field} does not match canonical disbursement"
                )
            }
            Self::ReadyDisbursement => {
                write!(
                    formatter,
                    "ready disbursement has no submitted provider request"
                )
            }
            Self::ReconciliationBeforeObservation {
                reconciled_at_unix_ms,
                observation_at_unix_ms,
            } => write!(
                formatter,
                "reconciliation at {reconciled_at_unix_ms} precedes provider observation at {observation_at_unix_ms}"
            ),
        }
    }
}

impl Error for ReconciliationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ProviderContract(error) => Some(error),
            _ => None,
        }
    }
}
