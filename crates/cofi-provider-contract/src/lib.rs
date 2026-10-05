use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_disbursements::{
    BeneficiaryReference, DestinationReference, Disbursement, DisbursementEventId, DisbursementId,
    DisbursementStatus, DisbursementTerminalEvent, FailureCode, ProviderEventReference,
    ProviderRequestReference, ProviderSettlementReference,
};
use cofi_ledger::{Currency, JournalEntryId};
use cofi_spending::FundSpendId;

pub const PROVIDER_IDEMPOTENCY_NAMESPACE: &str = "cofi-provider:v1";

macro_rules! contract_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ProviderContractError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(ProviderContractError::EmptyIdentifier($label));
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

contract_id!(ProviderIdempotencyKey, "provider_idempotency_key");
contract_id!(ProposalReference, "proposal_reference");
contract_id!(OrganizationReference, "organization_reference");
contract_id!(CommunityReference, "community_reference");
contract_id!(FundReference, "fund_reference");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDisbursementRequest {
    disbursement_id: DisbursementId,
    spend_id: FundSpendId,
    journal_entry_id: JournalEntryId,
    proposal_reference: ProposalReference,
    organization_reference: OrganizationReference,
    community_reference: CommunityReference,
    fund_reference: FundReference,
    currency: Currency,
    amount_minor: i128,
    purpose_reference: String,
    beneficiary_reference: BeneficiaryReference,
    destination_reference: DestinationReference,
    provider_request_reference: ProviderRequestReference,
    idempotency_key: ProviderIdempotencyKey,
    submitted_at_unix_ms: i64,
}

impl ProviderDisbursementRequest {
    #[must_use]
    pub const fn disbursement_id(&self) -> &DisbursementId {
        &self.disbursement_id
    }
    #[must_use]
    pub const fn spend_id(&self) -> &FundSpendId {
        &self.spend_id
    }
    #[must_use]
    pub const fn journal_entry_id(&self) -> &JournalEntryId {
        &self.journal_entry_id
    }
    #[must_use]
    pub const fn proposal_reference(&self) -> &ProposalReference {
        &self.proposal_reference
    }
    #[must_use]
    pub const fn organization_reference(&self) -> &OrganizationReference {
        &self.organization_reference
    }
    #[must_use]
    pub const fn community_reference(&self) -> &CommunityReference {
        &self.community_reference
    }
    #[must_use]
    pub const fn fund_reference(&self) -> &FundReference {
        &self.fund_reference
    }
    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }
    #[must_use]
    pub const fn amount_minor(&self) -> i128 {
        self.amount_minor
    }
    #[must_use]
    pub fn purpose_reference(&self) -> &str {
        &self.purpose_reference
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
    pub const fn provider_request_reference(&self) -> &ProviderRequestReference {
        &self.provider_request_reference
    }
    #[must_use]
    pub const fn idempotency_key(&self) -> &ProviderIdempotencyKey {
        &self.idempotency_key
    }
    #[must_use]
    pub const fn submitted_at_unix_ms(&self) -> i64 {
        self.submitted_at_unix_ms
    }
}

fn provider_idempotency_key(
    disbursement_id: &DisbursementId,
    spend_id: &FundSpendId,
    journal_entry_id: &JournalEntryId,
) -> Result<ProviderIdempotencyKey, ProviderContractError> {
    let disbursement = disbursement_id.as_str();
    let spend = spend_id.as_str();
    let journal = journal_entry_id.as_str();
    ProviderIdempotencyKey::new(format!(
        "{}:d{}:{}:s{}:{}:j{}:{}",
        PROVIDER_IDEMPOTENCY_NAMESPACE,
        disbursement.len(),
        disbursement,
        spend.len(),
        spend,
        journal.len(),
        journal,
    ))
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderObservationKind {
    Accepted {
        provider_event_reference: ProviderEventReference,
    },
    Settled {
        lifecycle_event_id: DisbursementEventId,
        provider_event_reference: ProviderEventReference,
        settlement_reference: ProviderSettlementReference,
    },
    Failed {
        lifecycle_event_id: DisbursementEventId,
        provider_event_reference: ProviderEventReference,
        failure_code: FailureCode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderObservation {
    disbursement_id: DisbursementId,
    provider_request_reference: ProviderRequestReference,
    kind: ProviderObservationKind,
    occurred_at_unix_ms: i64,
}

impl ProviderObservation {
    #[must_use]
    pub const fn accepted(
        disbursement_id: DisbursementId,
        provider_request_reference: ProviderRequestReference,
        provider_event_reference: ProviderEventReference,
        occurred_at_unix_ms: i64,
    ) -> Self {
        Self {
            disbursement_id,
            provider_request_reference,
            kind: ProviderObservationKind::Accepted {
                provider_event_reference,
            },
            occurred_at_unix_ms,
        }
    }

    #[must_use]
    pub const fn settled(
        disbursement_id: DisbursementId,
        provider_request_reference: ProviderRequestReference,
        lifecycle_event_id: DisbursementEventId,
        provider_event_reference: ProviderEventReference,
        settlement_reference: ProviderSettlementReference,
        occurred_at_unix_ms: i64,
    ) -> Self {
        Self {
            disbursement_id,
            provider_request_reference,
            kind: ProviderObservationKind::Settled {
                lifecycle_event_id,
                provider_event_reference,
                settlement_reference,
            },
            occurred_at_unix_ms,
        }
    }

    #[must_use]
    pub const fn failed(
        disbursement_id: DisbursementId,
        provider_request_reference: ProviderRequestReference,
        lifecycle_event_id: DisbursementEventId,
        provider_event_reference: ProviderEventReference,
        failure_code: FailureCode,
        occurred_at_unix_ms: i64,
    ) -> Self {
        Self {
            disbursement_id,
            provider_request_reference,
            kind: ProviderObservationKind::Failed {
                lifecycle_event_id,
                provider_event_reference,
                failure_code,
            },
            occurred_at_unix_ms,
        }
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
    pub const fn kind(&self) -> &ProviderObservationKind {
        &self.kind
    }
    #[must_use]
    pub const fn occurred_at_unix_ms(&self) -> i64 {
        self.occurred_at_unix_ms
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProviderContract;

impl ProviderContract {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn build_request(
        &self,
        disbursement: &Disbursement,
    ) -> Result<ProviderDisbursementRequest, ProviderContractError> {
        if disbursement.status() != DisbursementStatus::Submitted {
            return Err(ProviderContractError::InvalidDisbursementStatus {
                disbursement_id: disbursement.id().clone(),
                status: disbursement.status(),
                operation: "build provider request",
            });
        }
        let provider_request_reference = disbursement
            .provider_request_reference()
            .ok_or_else(|| {
                ProviderContractError::MissingProviderRequestReference(disbursement.id().clone())
            })?
            .clone();
        let submitted_at_unix_ms = disbursement.submitted_at_unix_ms().ok_or_else(|| {
            ProviderContractError::MissingSubmissionTimestamp(disbursement.id().clone())
        })?;
        let spend = disbursement.spend();
        let idempotency_key = provider_idempotency_key(
            disbursement.id(),
            spend.spend_id(),
            spend.journal_entry_id(),
        )?;
        Ok(ProviderDisbursementRequest {
            disbursement_id: disbursement.id().clone(),
            spend_id: spend.spend_id().clone(),
            journal_entry_id: spend.journal_entry_id().clone(),
            proposal_reference: ProposalReference::new(spend.proposal_id().as_str())?,
            organization_reference: OrganizationReference::new(spend.organization_id().as_str())?,
            community_reference: CommunityReference::new(spend.community_id().as_str())?,
            fund_reference: FundReference::new(spend.fund_id().as_str())?,
            currency: spend.currency(),
            amount_minor: spend.amount_minor(),
            purpose_reference: spend.purpose_reference().to_owned(),
            beneficiary_reference: disbursement.beneficiary_reference().clone(),
            destination_reference: disbursement.destination_reference().clone(),
            provider_request_reference,
            idempotency_key,
            submitted_at_unix_ms,
        })
    }

    pub fn validate_observation(
        &self,
        disbursement: &Disbursement,
        observation: &ProviderObservation,
    ) -> Result<(), ProviderContractError> {
        if observation.disbursement_id() != disbursement.id() {
            return Err(ProviderContractError::ObservationBindingMismatch(
                "disbursement_id",
            ));
        }
        if disbursement.status() == DisbursementStatus::Ready {
            return Err(ProviderContractError::InvalidDisbursementStatus {
                disbursement_id: disbursement.id().clone(),
                status: disbursement.status(),
                operation: "validate provider observation",
            });
        }
        let expected_request = disbursement.provider_request_reference().ok_or_else(|| {
            ProviderContractError::MissingProviderRequestReference(disbursement.id().clone())
        })?;
        if observation.provider_request_reference() != expected_request {
            return Err(ProviderContractError::ObservationBindingMismatch(
                "provider_request_reference",
            ));
        }
        let submitted_at_unix_ms = disbursement.submitted_at_unix_ms().ok_or_else(|| {
            ProviderContractError::MissingSubmissionTimestamp(disbursement.id().clone())
        })?;
        if observation.occurred_at_unix_ms() < submitted_at_unix_ms {
            return Err(ProviderContractError::ObservationBeforeSubmission {
                submitted_at_unix_ms,
                occurred_at_unix_ms: observation.occurred_at_unix_ms(),
            });
        }
        Ok(())
    }

    pub fn to_terminal_event(
        &self,
        disbursement: &Disbursement,
        observation: &ProviderObservation,
    ) -> Result<DisbursementTerminalEvent, ProviderContractError> {
        self.validate_observation(disbursement, observation)?;
        match observation.kind() {
            ProviderObservationKind::Accepted { .. } => {
                Err(ProviderContractError::AcceptedObservationIsNotTerminal)
            }
            ProviderObservationKind::Settled {
                lifecycle_event_id,
                provider_event_reference,
                settlement_reference,
            } => Ok(DisbursementTerminalEvent::settled(
                lifecycle_event_id.clone(),
                observation.disbursement_id().clone(),
                provider_event_reference.clone(),
                settlement_reference.clone(),
                observation.occurred_at_unix_ms(),
            )),
            ProviderObservationKind::Failed {
                lifecycle_event_id,
                provider_event_reference,
                failure_code,
            } => Ok(DisbursementTerminalEvent::failed(
                lifecycle_event_id.clone(),
                observation.disbursement_id().clone(),
                provider_event_reference.clone(),
                failure_code.clone(),
                observation.occurred_at_unix_ms(),
            )),
        }
    }

    pub fn validate_request(
        &self,
        disbursement: &Disbursement,
        request: &ProviderDisbursementRequest,
    ) -> Result<(), ProviderContractError> {
        let expected = self.build_request(disbursement)?;
        if &expected != request {
            return Err(ProviderContractError::RequestSnapshotMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderContractError {
    EmptyIdentifier(&'static str),
    InvalidDisbursementStatus {
        disbursement_id: DisbursementId,
        status: DisbursementStatus,
        operation: &'static str,
    },
    MissingProviderRequestReference(DisbursementId),
    MissingSubmissionTimestamp(DisbursementId),
    RequestSnapshotMismatch,
    ObservationBindingMismatch(&'static str),
    ObservationBeforeSubmission {
        submitted_at_unix_ms: i64,
        occurred_at_unix_ms: i64,
    },
    AcceptedObservationIsNotTerminal,
}

impl Display for ProviderContractError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(label) => write!(formatter, "{label} must not be empty"),
            Self::InvalidDisbursementStatus {
                disbursement_id,
                status,
                operation,
            } => write!(
                formatter,
                "cannot {operation} for disbursement {} in status {status:?}",
                disbursement_id.as_str()
            ),
            Self::MissingProviderRequestReference(id) => write!(
                formatter,
                "disbursement {} has no provider request reference",
                id.as_str()
            ),
            Self::MissingSubmissionTimestamp(id) => write!(
                formatter,
                "disbursement {} has no submission timestamp",
                id.as_str()
            ),
            Self::RequestSnapshotMismatch => {
                write!(
                    formatter,
                    "provider request does not match canonical disbursement snapshot"
                )
            }
            Self::ObservationBindingMismatch(field) => {
                write!(
                    formatter,
                    "provider observation {field} does not match canonical disbursement"
                )
            }
            Self::ObservationBeforeSubmission {
                submitted_at_unix_ms,
                occurred_at_unix_ms,
            } => write!(
                formatter,
                "provider observation at {occurred_at_unix_ms} precedes submission at {submitted_at_unix_ms}"
            ),
            Self::AcceptedObservationIsNotTerminal => {
                write!(formatter, "accepted provider observation is not terminal")
            }
        }
    }
}

impl Error for ProviderContractError {}
