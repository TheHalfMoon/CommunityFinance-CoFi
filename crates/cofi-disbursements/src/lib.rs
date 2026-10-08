use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_community::CommunityRegistry;
use cofi_governance::GovernanceEngine;
use cofi_ledger::Ledger;
use cofi_spending::{
    ApprovedFundSpendEvent, FundSpendBridge, FundSpendError, FundSpendId, VerifiedFundSpend,
};

macro_rules! disbursement_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, DisbursementError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(DisbursementError::EmptyIdentifier($label));
                }
                Ok(Self(value))
            }
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

disbursement_id!(DisbursementId, "disbursement_id");
disbursement_id!(DisbursementEventId, "disbursement_event_id");
disbursement_id!(BeneficiaryReference, "beneficiary_reference");
disbursement_id!(DestinationReference, "destination_reference");
disbursement_id!(ProviderRequestReference, "provider_request_reference");
disbursement_id!(ProviderEventReference, "provider_event_reference");
disbursement_id!(ProviderSettlementReference, "provider_settlement_reference");
disbursement_id!(FailureCode, "failure_code");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisbursementCreation {
    source_event_id: DisbursementEventId,
    id: DisbursementId,
    beneficiary_reference: BeneficiaryReference,
    destination_reference: DestinationReference,
    created_at_unix_ms: i64,
}
impl DisbursementCreation {
    #[must_use]
    pub const fn new(
        source_event_id: DisbursementEventId,
        id: DisbursementId,
        beneficiary_reference: BeneficiaryReference,
        destination_reference: DestinationReference,
        created_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            id,
            beneficiary_reference,
            destination_reference,
            created_at_unix_ms,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &DisbursementId {
        &self.id
    }

    /// Immutable accepted creation source event identity.
    #[must_use]
    pub const fn source_event_id(&self) -> &DisbursementEventId {
        &self.source_event_id
    }
    #[must_use]
    pub const fn beneficiary_reference(&self) -> &BeneficiaryReference {
        &self.beneficiary_reference
    }
    #[must_use]
    pub const fn destination_reference(&self) -> &DestinationReference {
        &self.destination_reference
    }
    #[must_use]
    pub const fn created_at_unix_ms(&self) -> i64 {
        self.created_at_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisbursementSubmission {
    source_event_id: DisbursementEventId,
    disbursement_id: DisbursementId,
    provider_request_reference: ProviderRequestReference,
    submitted_at_unix_ms: i64,
}

impl DisbursementSubmission {
    #[must_use]
    pub const fn new(
        source_event_id: DisbursementEventId,
        disbursement_id: DisbursementId,
        provider_request_reference: ProviderRequestReference,
        submitted_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            disbursement_id,
            provider_request_reference,
            submitted_at_unix_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalKind {
    Settled {
        settlement_reference: ProviderSettlementReference,
    },
    Failed {
        failure_code: FailureCode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisbursementTerminalEvent {
    source_event_id: DisbursementEventId,
    disbursement_id: DisbursementId,
    provider_event_reference: ProviderEventReference,
    kind: TerminalKind,
    terminal_at_unix_ms: i64,
}

impl DisbursementTerminalEvent {
    #[must_use]
    pub const fn settled(
        source_event_id: DisbursementEventId,
        disbursement_id: DisbursementId,
        provider_event_reference: ProviderEventReference,
        settlement_reference: ProviderSettlementReference,
        terminal_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            disbursement_id,
            provider_event_reference,
            kind: TerminalKind::Settled {
                settlement_reference,
            },
            terminal_at_unix_ms,
        }
    }

    #[must_use]
    pub const fn failed(
        source_event_id: DisbursementEventId,
        disbursement_id: DisbursementId,
        provider_event_reference: ProviderEventReference,
        failure_code: FailureCode,
        terminal_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            disbursement_id,
            provider_event_reference,
            kind: TerminalKind::Failed { failure_code },
            terminal_at_unix_ms,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> &TerminalKind {
        &self.kind
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisbursementStatus {
    Ready,
    Submitted,
    Settled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disbursement {
    creation: DisbursementCreation,
    spend: VerifiedFundSpend,
    status: DisbursementStatus,
    provider_request_reference: Option<ProviderRequestReference>,
    submitted_at_unix_ms: Option<i64>,
    provider_event_reference: Option<ProviderEventReference>,
    provider_settlement_reference: Option<ProviderSettlementReference>,
    failure_code: Option<FailureCode>,
    terminal_at_unix_ms: Option<i64>,
}

impl Disbursement {
    #[must_use]
    pub const fn id(&self) -> &DisbursementId {
        self.creation.id()
    }
    #[must_use]
    pub const fn spend(&self) -> &VerifiedFundSpend {
        &self.spend
    }
    #[must_use]
    pub const fn status(&self) -> DisbursementStatus {
        self.status
    }
    #[must_use]
    pub const fn beneficiary_reference(&self) -> &BeneficiaryReference {
        &self.creation.beneficiary_reference
    }
    #[must_use]
    pub const fn destination_reference(&self) -> &DestinationReference {
        &self.creation.destination_reference
    }
    #[must_use]
    pub const fn provider_request_reference(&self) -> Option<&ProviderRequestReference> {
        self.provider_request_reference.as_ref()
    }
    #[must_use]
    pub const fn submitted_at_unix_ms(&self) -> Option<i64> {
        self.submitted_at_unix_ms
    }
    #[must_use]
    pub const fn provider_event_reference(&self) -> Option<&ProviderEventReference> {
        self.provider_event_reference.as_ref()
    }
    #[must_use]
    pub const fn provider_settlement_reference(&self) -> Option<&ProviderSettlementReference> {
        self.provider_settlement_reference.as_ref()
    }
    #[must_use]
    pub const fn terminal_at_unix_ms(&self) -> Option<i64> {
        self.terminal_at_unix_ms
    }
    #[must_use]
    pub const fn failure_code(&self) -> Option<&FailureCode> {
        self.failure_code.as_ref()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationOutcome {
    Created,
    Replayed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionOutcome {
    Submitted,
    Replayed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalOutcome {
    Settled,
    Failed,
    Replayed { status: DisbursementStatus },
}

#[derive(Debug, Clone, Default)]
pub struct DisbursementEngine {
    disbursements: BTreeMap<DisbursementId, Disbursement>,
    creation_events: BTreeMap<DisbursementEventId, DisbursementId>,
    submission_events: BTreeMap<DisbursementEventId, DisbursementSubmission>,
    terminal_events: BTreeMap<DisbursementEventId, DisbursementTerminalEvent>,
    spend_index: BTreeMap<FundSpendId, DisbursementId>,
    provider_request_index: BTreeMap<ProviderRequestReference, DisbursementId>,
    provider_event_index: BTreeMap<ProviderEventReference, DisbursementId>,
    provider_settlement_index: BTreeMap<ProviderSettlementReference, DisbursementId>,
}

impl DisbursementEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn disbursement(&self, id: &DisbursementId) -> Option<&Disbursement> {
        self.disbursements.get(id)
    }

    pub fn create(
        &mut self,
        registry: &CommunityRegistry,
        governance: &GovernanceEngine,
        ledger: &Ledger,
        spend_event: &ApprovedFundSpendEvent,
        creation: DisbursementCreation,
    ) -> Result<CreationOutcome, DisbursementError> {
        let verified = FundSpendBridge::new()
            .verify_committed(registry, governance, spend_event, ledger)
            .map_err(DisbursementError::SpendVerification)?;
        if let Some(existing_id) = self.creation_events.get(&creation.source_event_id) {
            let existing =
                self.disbursements
                    .get(existing_id)
                    .ok_or(DisbursementError::InternalInvariant(
                        "creation event points to missing record",
                    ))?;
            return if existing.creation == creation && existing.spend == verified {
                Ok(CreationOutcome::Replayed)
            } else {
                Err(DisbursementError::CreationEventConflict(
                    creation.source_event_id.clone(),
                ))
            };
        }
        if self
            .submission_events
            .contains_key(&creation.source_event_id)
            || self.terminal_events.contains_key(&creation.source_event_id)
        {
            return Err(DisbursementError::EventIdentityConflict(
                creation.source_event_id.clone(),
            ));
        }
        if self.disbursements.contains_key(creation.id()) {
            return Err(DisbursementError::DisbursementIdConflict(
                creation.id.clone(),
            ));
        }
        if let Some(existing_id) = self.spend_index.get(verified.spend_id()) {
            return Err(DisbursementError::SpendAlreadyBound {
                spend_id: verified.spend_id().clone(),
                disbursement_id: existing_id.clone(),
            });
        }
        if creation.created_at_unix_ms < verified.executed_at_unix_ms() {
            return Err(DisbursementError::CreationBeforeSpend {
                spend_executed_at_unix_ms: verified.executed_at_unix_ms(),
                created_at_unix_ms: creation.created_at_unix_ms,
            });
        }

        let id = creation.id.clone();
        let source_event_id = creation.source_event_id.clone();
        let spend_id = verified.spend_id().clone();
        let record = Disbursement {
            creation,
            spend: verified,
            status: DisbursementStatus::Ready,
            provider_request_reference: None,
            submitted_at_unix_ms: None,
            provider_event_reference: None,
            provider_settlement_reference: None,
            failure_code: None,
            terminal_at_unix_ms: None,
        };
        self.creation_events.insert(source_event_id, id.clone());
        self.spend_index.insert(spend_id, id.clone());
        self.disbursements.insert(id, record);
        Ok(CreationOutcome::Created)
    }

    pub fn submit(
        &mut self,
        submission: DisbursementSubmission,
    ) -> Result<SubmissionOutcome, DisbursementError> {
        if let Some(existing) = self.submission_events.get(&submission.source_event_id) {
            return if existing == &submission {
                Ok(SubmissionOutcome::Replayed)
            } else {
                Err(DisbursementError::SubmissionEventConflict(
                    submission.source_event_id.clone(),
                ))
            };
        }
        if self
            .creation_events
            .contains_key(&submission.source_event_id)
            || self
                .terminal_events
                .contains_key(&submission.source_event_id)
        {
            return Err(DisbursementError::EventIdentityConflict(
                submission.source_event_id.clone(),
            ));
        }
        if let Some(existing_id) = self
            .provider_request_index
            .get(&submission.provider_request_reference)
        {
            return Err(DisbursementError::ProviderRequestReferenceConflict {
                reference: submission.provider_request_reference.clone(),
                disbursement_id: existing_id.clone(),
            });
        }
        let record = self
            .disbursements
            .get(&submission.disbursement_id)
            .ok_or_else(|| {
                DisbursementError::UnknownDisbursement(submission.disbursement_id.clone())
            })?;
        if record.status != DisbursementStatus::Ready {
            return Err(DisbursementError::InvalidTransition {
                disbursement_id: submission.disbursement_id.clone(),
                from: record.status,
                attempted: "submit",
            });
        }
        if submission.submitted_at_unix_ms < record.creation.created_at_unix_ms {
            return Err(DisbursementError::SubmissionBeforeCreation {
                created_at_unix_ms: record.creation.created_at_unix_ms,
                submitted_at_unix_ms: submission.submitted_at_unix_ms,
            });
        }
        let id = submission.disbursement_id.clone();
        let source_event_id = submission.source_event_id.clone();
        let provider_request_reference = submission.provider_request_reference.clone();
        self.submission_events
            .insert(source_event_id, submission.clone());
        self.provider_request_index
            .insert(provider_request_reference.clone(), id.clone());
        let record =
            self.disbursements
                .get_mut(&id)
                .ok_or(DisbursementError::InternalInvariant(
                    "validated disbursement disappeared",
                ))?;
        record.status = DisbursementStatus::Submitted;
        record.provider_request_reference = Some(provider_request_reference);
        record.submitted_at_unix_ms = Some(submission.submitted_at_unix_ms);
        Ok(SubmissionOutcome::Submitted)
    }

    pub fn record_terminal(
        &mut self,
        event: DisbursementTerminalEvent,
    ) -> Result<TerminalOutcome, DisbursementError> {
        if let Some(existing) = self.terminal_events.get(&event.source_event_id) {
            return if existing == &event {
                let status = terminal_status(&event.kind);
                Ok(TerminalOutcome::Replayed { status })
            } else {
                Err(DisbursementError::TerminalEventConflict(
                    event.source_event_id.clone(),
                ))
            };
        }
        if self.creation_events.contains_key(&event.source_event_id)
            || self.submission_events.contains_key(&event.source_event_id)
        {
            return Err(DisbursementError::EventIdentityConflict(
                event.source_event_id.clone(),
            ));
        }
        if let Some(existing_id) = self
            .provider_event_index
            .get(&event.provider_event_reference)
        {
            return Err(DisbursementError::ProviderEventReferenceConflict {
                reference: event.provider_event_reference.clone(),
                disbursement_id: existing_id.clone(),
            });
        }
        if let TerminalKind::Settled {
            settlement_reference,
        } = &event.kind
        {
            if let Some(existing_id) = self.provider_settlement_index.get(settlement_reference) {
                return Err(DisbursementError::ProviderSettlementReferenceConflict {
                    reference: settlement_reference.clone(),
                    disbursement_id: existing_id.clone(),
                });
            }
        }
        let record = self
            .disbursements
            .get(&event.disbursement_id)
            .ok_or_else(|| DisbursementError::UnknownDisbursement(event.disbursement_id.clone()))?;
        if record.status != DisbursementStatus::Submitted {
            return Err(DisbursementError::InvalidTransition {
                disbursement_id: event.disbursement_id.clone(),
                from: record.status,
                attempted: "terminal",
            });
        }
        let submitted_at_unix_ms =
            record
                .submitted_at_unix_ms
                .ok_or(DisbursementError::InternalInvariant(
                    "submitted record has no submission timestamp",
                ))?;
        if event.terminal_at_unix_ms < submitted_at_unix_ms {
            return Err(DisbursementError::TerminalBeforeSubmission {
                submitted_at_unix_ms,
                terminal_at_unix_ms: event.terminal_at_unix_ms,
            });
        }

        let id = event.disbursement_id.clone();
        let source_event_id = event.source_event_id.clone();
        let provider_event_reference = event.provider_event_reference.clone();
        let terminal_at_unix_ms = event.terminal_at_unix_ms;
        let status = terminal_status(&event.kind);
        let settlement_reference = match &event.kind {
            TerminalKind::Settled {
                settlement_reference,
            } => Some(settlement_reference.clone()),
            TerminalKind::Failed { .. } => None,
        };
        let failure_code = match &event.kind {
            TerminalKind::Failed { failure_code } => Some(failure_code.clone()),
            TerminalKind::Settled { .. } => None,
        };
        self.terminal_events.insert(source_event_id, event);
        self.provider_event_index
            .insert(provider_event_reference.clone(), id.clone());
        if let Some(reference) = settlement_reference.as_ref() {
            self.provider_settlement_index
                .insert(reference.clone(), id.clone());
        }
        let record =
            self.disbursements
                .get_mut(&id)
                .ok_or(DisbursementError::InternalInvariant(
                    "validated disbursement disappeared",
                ))?;
        record.status = status;
        record.provider_event_reference = Some(provider_event_reference);
        record.provider_settlement_reference = settlement_reference;
        record.failure_code = failure_code;
        record.terminal_at_unix_ms = Some(terminal_at_unix_ms);
        Ok(match status {
            DisbursementStatus::Settled => TerminalOutcome::Settled,
            DisbursementStatus::Failed => TerminalOutcome::Failed,
            DisbursementStatus::Ready | DisbursementStatus::Submitted => {
                return Err(DisbursementError::InternalInvariant(
                    "terminal event produced non-terminal status",
                ));
            }
        })
    }
}

fn terminal_status(kind: &TerminalKind) -> DisbursementStatus {
    match kind {
        TerminalKind::Settled { .. } => DisbursementStatus::Settled,
        TerminalKind::Failed { .. } => DisbursementStatus::Failed,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisbursementError {
    EmptyIdentifier(&'static str),
    SpendVerification(FundSpendError),
    CreationEventConflict(DisbursementEventId),
    SubmissionEventConflict(DisbursementEventId),
    TerminalEventConflict(DisbursementEventId),
    EventIdentityConflict(DisbursementEventId),
    DisbursementIdConflict(DisbursementId),
    SpendAlreadyBound {
        spend_id: FundSpendId,
        disbursement_id: DisbursementId,
    },
    CreationBeforeSpend {
        spend_executed_at_unix_ms: i64,
        created_at_unix_ms: i64,
    },
    UnknownDisbursement(DisbursementId),
    ProviderRequestReferenceConflict {
        reference: ProviderRequestReference,
        disbursement_id: DisbursementId,
    },
    SubmissionBeforeCreation {
        created_at_unix_ms: i64,
        submitted_at_unix_ms: i64,
    },
    ProviderEventReferenceConflict {
        reference: ProviderEventReference,
        disbursement_id: DisbursementId,
    },
    ProviderSettlementReferenceConflict {
        reference: ProviderSettlementReference,
        disbursement_id: DisbursementId,
    },
    InvalidTransition {
        disbursement_id: DisbursementId,
        from: DisbursementStatus,
        attempted: &'static str,
    },
    TerminalBeforeSubmission {
        submitted_at_unix_ms: i64,
        terminal_at_unix_ms: i64,
    },
    InternalInvariant(&'static str),
}

impl Display for DisbursementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(label) => write!(formatter, "{label} must not be empty"),
            Self::SpendVerification(error) => {
                write!(formatter, "spend verification failed: {error}")
            }
            Self::CreationEventConflict(id) => {
                write!(formatter, "creation event {} conflicts", id.as_str())
            }
            Self::SubmissionEventConflict(id) => {
                write!(formatter, "submission event {} conflicts", id.as_str())
            }
            Self::TerminalEventConflict(id) => {
                write!(formatter, "terminal event {} conflicts", id.as_str())
            }
            Self::EventIdentityConflict(id) => {
                write!(formatter, "event identity {} is already used", id.as_str())
            }
            Self::DisbursementIdConflict(id) => {
                write!(formatter, "disbursement id {} conflicts", id.as_str())
            }
            Self::SpendAlreadyBound {
                spend_id,
                disbursement_id,
            } => write!(
                formatter,
                "spend {} is already bound to disbursement {}",
                spend_id.as_str(),
                disbursement_id.as_str()
            ),
            Self::CreationBeforeSpend {
                spend_executed_at_unix_ms,
                created_at_unix_ms,
            } => write!(
                formatter,
                "disbursement creation {created_at_unix_ms} precedes spend execution {spend_executed_at_unix_ms}"
            ),
            Self::UnknownDisbursement(id) => {
                write!(formatter, "unknown disbursement {}", id.as_str())
            }
            Self::ProviderRequestReferenceConflict {
                reference,
                disbursement_id,
            } => write!(
                formatter,
                "provider request {} is already bound to disbursement {}",
                reference.as_str(),
                disbursement_id.as_str()
            ),
            Self::SubmissionBeforeCreation {
                created_at_unix_ms,
                submitted_at_unix_ms,
            } => write!(
                formatter,
                "submission {submitted_at_unix_ms} precedes creation {created_at_unix_ms}"
            ),
            Self::ProviderEventReferenceConflict {
                reference,
                disbursement_id,
            } => write!(
                formatter,
                "provider event {} is already bound to disbursement {}",
                reference.as_str(),
                disbursement_id.as_str()
            ),
            Self::ProviderSettlementReferenceConflict {
                reference,
                disbursement_id,
            } => write!(
                formatter,
                "provider settlement {} is already bound to disbursement {}",
                reference.as_str(),
                disbursement_id.as_str()
            ),
            Self::InvalidTransition {
                disbursement_id,
                from,
                attempted,
            } => write!(
                formatter,
                "cannot {attempted} disbursement {} from {from:?}",
                disbursement_id.as_str()
            ),
            Self::TerminalBeforeSubmission {
                submitted_at_unix_ms,
                terminal_at_unix_ms,
            } => write!(
                formatter,
                "terminal event {terminal_at_unix_ms} precedes submission {submitted_at_unix_ms}"
            ),
            Self::InternalInvariant(message) => write!(formatter, "internal invariant: {message}"),
        }
    }
}
impl Error for DisbursementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SpendVerification(error) => Some(error),
            _ => None,
        }
    }
}
