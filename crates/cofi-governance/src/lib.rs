use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_community::{
    CommunityId, CommunityRegistry, FundId, MembershipRole, MembershipStatus, OrganizationId,
    PartyId,
};
use cofi_ledger::Currency;

macro_rules! governance_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, GovernanceError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(GovernanceError::EmptyIdentifier($label));
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
governance_id!(SpendingApprovalPolicyId, "spending_approval_policy_id");
governance_id!(SpendingProposalEventId, "spending_proposal_event_id");
governance_id!(SpendingProposalId, "spending_proposal_id");
governance_id!(SpendingApprovalEventId, "spending_approval_event_id");
governance_id!(SpendingApprovalId, "spending_approval_id");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpendingApprovalPolicy {
    id: SpendingApprovalPolicyId,
    version: u32,
    organization_id: OrganizationId,
    community_id: CommunityId,
    fund_id: FundId,
    currency: Currency,
    max_amount_minor: i128,
    required_approvals: u16,
    eligible_roles: Vec<MembershipRole>,
}

fn role_rank(role: MembershipRole) -> u8 {
    match role {
        MembershipRole::Owner => 0,
        MembershipRole::Admin => 1,
        MembershipRole::Treasurer => 2,
        MembershipRole::Member => 3,
    }
}
impl SpendingApprovalPolicy {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: SpendingApprovalPolicyId,
        version: u32,
        organization_id: OrganizationId,
        community_id: CommunityId,
        fund_id: FundId,
        currency: Currency,
        max_amount_minor: i128,
        required_approvals: u16,
        mut eligible_roles: Vec<MembershipRole>,
    ) -> Result<Self, GovernanceError> {
        if version == 0 {
            return Err(GovernanceError::InvalidPolicyVersion);
        }
        if max_amount_minor <= 0 {
            return Err(GovernanceError::InvalidPolicyMaxAmount(max_amount_minor));
        }
        if required_approvals == 0 {
            return Err(GovernanceError::InvalidApprovalQuorum);
        }
        if eligible_roles.is_empty() {
            return Err(GovernanceError::EmptyEligibleRoles);
        }
        eligible_roles.sort_by_key(|role| role_rank(*role));
        eligible_roles.dedup();
        Ok(Self {
            id,
            version,
            organization_id,
            community_id,
            fund_id,
            currency,
            max_amount_minor,
            required_approvals,
            eligible_roles,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &SpendingApprovalPolicyId {
        &self.id
    }
    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }
    #[must_use]
    pub const fn organization_id(&self) -> &OrganizationId {
        &self.organization_id
    }
    #[must_use]
    pub const fn community_id(&self) -> &CommunityId {
        &self.community_id
    }
    #[must_use]
    pub const fn fund_id(&self) -> &FundId {
        &self.fund_id
    }
    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }
    #[must_use]
    pub const fn max_amount_minor(&self) -> i128 {
        self.max_amount_minor
    }
    #[must_use]
    pub const fn required_approvals(&self) -> u16 {
        self.required_approvals
    }
    #[must_use]
    pub fn eligible_roles(&self) -> &[MembershipRole] {
        &self.eligible_roles
    }

    #[must_use]
    pub fn role_is_eligible(&self, role: MembershipRole) -> bool {
        self.eligible_roles.contains(&role)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpendingProposal {
    source_event_id: SpendingProposalEventId,
    id: SpendingProposalId,
    policy_id: SpendingApprovalPolicyId,
    policy_version: u32,
    requester_party_id: PartyId,
    organization_id: OrganizationId,
    community_id: CommunityId,
    fund_id: FundId,
    currency: Currency,
    amount_minor: i128,
    purpose_reference: String,
    created_at_unix_ms: i64,
    expires_at_unix_ms: i64,
}

impl SpendingProposal {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_event_id: SpendingProposalEventId,
        id: SpendingProposalId,
        policy_id: SpendingApprovalPolicyId,
        policy_version: u32,
        requester_party_id: PartyId,
        organization_id: OrganizationId,
        community_id: CommunityId,
        fund_id: FundId,
        currency: Currency,
        amount_minor: i128,
        purpose_reference: impl Into<String>,
        created_at_unix_ms: i64,
        expires_at_unix_ms: i64,
    ) -> Result<Self, GovernanceError> {
        let purpose_reference = purpose_reference.into();
        if amount_minor <= 0 {
            return Err(GovernanceError::InvalidProposalAmount(amount_minor));
        }
        if purpose_reference.trim().is_empty() {
            return Err(GovernanceError::EmptyPurposeReference);
        }
        if expires_at_unix_ms <= created_at_unix_ms {
            return Err(GovernanceError::InvalidProposalExpiry {
                created_at_unix_ms,
                expires_at_unix_ms,
            });
        }
        Ok(Self {
            source_event_id,
            id,
            policy_id,
            policy_version,
            requester_party_id,
            organization_id,
            community_id,
            fund_id,
            currency,
            amount_minor,
            purpose_reference,
            created_at_unix_ms,
            expires_at_unix_ms,
        })
    }

    /// Original accepted proposal source and immutable policy binding.
    #[must_use]
    pub const fn source_event_id(&self) -> &SpendingProposalEventId {
        &self.source_event_id
    }
    #[must_use]
    pub const fn policy_id(&self) -> &SpendingApprovalPolicyId {
        &self.policy_id
    }
    #[must_use]
    pub const fn policy_version(&self) -> u32 {
        self.policy_version
    }
    #[must_use]
    pub const fn requester_party_id(&self) -> &PartyId {
        &self.requester_party_id
    }
    #[must_use]
    pub const fn organization_id(&self) -> &OrganizationId {
        &self.organization_id
    }
    #[must_use]
    pub const fn fund_id(&self) -> &FundId {
        &self.fund_id
    }
    #[must_use]
    pub const fn created_at_unix_ms(&self) -> i64 {
        self.created_at_unix_ms
    }
    #[must_use]
    pub const fn expires_at_unix_ms(&self) -> i64 {
        self.expires_at_unix_ms
    }

    #[must_use]
    pub const fn id(&self) -> &SpendingProposalId {
        &self.id
    }
    #[must_use]
    pub const fn community_id(&self) -> &CommunityId {
        &self.community_id
    }
    #[must_use]
    pub const fn amount_minor(&self) -> i128 {
        self.amount_minor
    }
    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }
    #[must_use]
    pub fn purpose_reference(&self) -> &str {
        &self.purpose_reference
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpendingApproval {
    source_event_id: SpendingApprovalEventId,
    id: SpendingApprovalId,
    proposal_id: SpendingProposalId,
    approver_party_id: PartyId,
    approved_at_unix_ms: i64,
}

impl SpendingApproval {
    #[must_use]
    pub const fn new(
        source_event_id: SpendingApprovalEventId,
        id: SpendingApprovalId,
        proposal_id: SpendingProposalId,
        approver_party_id: PartyId,
        approved_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            id,
            proposal_id,
            approver_party_id,
            approved_at_unix_ms,
        }
    }

    /// Exact original source-event identity and approver record.
    #[must_use]
    pub const fn source_event_id(&self) -> &SpendingApprovalEventId {
        &self.source_event_id
    }
    #[must_use]
    pub const fn proposal_id(&self) -> &SpendingProposalId {
        &self.proposal_id
    }
    #[must_use]
    pub const fn approver_party_id(&self) -> &PartyId {
        &self.approver_party_id
    }
    #[must_use]
    pub const fn approved_at_unix_ms(&self) -> i64 {
        self.approved_at_unix_ms
    }

    #[must_use]
    pub const fn id(&self) -> &SpendingApprovalId {
        &self.id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalStatus {
    Pending,
    Approved,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedSpendingAuthorization {
    proposal_id: SpendingProposalId,
    policy_id: SpendingApprovalPolicyId,
    policy_version: u32,
    organization_id: OrganizationId,
    community_id: CommunityId,
    fund_id: FundId,
    currency: Currency,
    amount_minor: i128,
    purpose_reference: String,
    approver_party_ids: Vec<PartyId>,
    approved_at_unix_ms: i64,
}

impl ApprovedSpendingAuthorization {
    #[must_use]
    pub const fn proposal_id(&self) -> &SpendingProposalId {
        &self.proposal_id
    }
    #[must_use]
    pub const fn policy_id(&self) -> &SpendingApprovalPolicyId {
        &self.policy_id
    }
    #[must_use]
    pub const fn policy_version(&self) -> u32 {
        self.policy_version
    }
    #[must_use]
    pub const fn organization_id(&self) -> &OrganizationId {
        &self.organization_id
    }
    #[must_use]
    pub const fn community_id(&self) -> &CommunityId {
        &self.community_id
    }
    #[must_use]
    pub const fn fund_id(&self) -> &FundId {
        &self.fund_id
    }
    #[must_use]
    pub const fn amount_minor(&self) -> i128 {
        self.amount_minor
    }
    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }
    #[must_use]
    pub fn purpose_reference(&self) -> &str {
        &self.purpose_reference
    }
    #[must_use]
    pub fn approver_party_ids(&self) -> &[PartyId] {
        &self.approver_party_ids
    }
    #[must_use]
    pub const fn approved_at_unix_ms(&self) -> i64 {
        self.approved_at_unix_ms
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyRegistrationOutcome {
    Registered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalSubmissionOutcome {
    Submitted,
    Replayed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Recorded { status: ProposalStatus },
    Replayed { status: ProposalStatus },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProposalRecord {
    proposal: SpendingProposal,
    status: ProposalStatus,
    approvals_by_party: BTreeMap<PartyId, SpendingApprovalId>,
    authorization: Option<ApprovedSpendingAuthorization>,
}

#[derive(Debug, Clone, Default)]
pub struct GovernanceEngine {
    policies: BTreeMap<(SpendingApprovalPolicyId, u32), SpendingApprovalPolicy>,
    highest_policy_version: BTreeMap<SpendingApprovalPolicyId, u32>,
    proposals: BTreeMap<SpendingProposalId, ProposalRecord>,
    proposal_events: BTreeMap<SpendingProposalEventId, SpendingProposalId>,
    approvals: BTreeMap<SpendingApprovalId, SpendingApproval>,
    approval_events: BTreeMap<SpendingApprovalEventId, SpendingApprovalId>,
}
impl GovernanceEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_policy(
        &mut self,
        registry: &CommunityRegistry,
        policy: SpendingApprovalPolicy,
    ) -> Result<PolicyRegistrationOutcome, GovernanceError> {
        validate_policy_boundary(registry, &policy)?;
        let key = (policy.id().clone(), policy.version());
        if self.policies.contains_key(&key) {
            return Err(GovernanceError::DuplicatePolicyVersion {
                policy_id: policy.id().clone(),
                version: policy.version(),
            });
        }
        if let Some(highest) = self.highest_policy_version.get(policy.id()) {
            if policy.version() <= *highest {
                return Err(GovernanceError::NonMonotonicPolicyVersion {
                    policy_id: policy.id().clone(),
                    highest: *highest,
                    attempted: policy.version(),
                });
            }
        }
        self.highest_policy_version
            .insert(policy.id().clone(), policy.version());
        self.policies.insert(key, policy);
        Ok(PolicyRegistrationOutcome::Registered)
    }

    pub fn submit_proposal(
        &mut self,
        registry: &CommunityRegistry,
        proposal: SpendingProposal,
    ) -> Result<ProposalSubmissionOutcome, GovernanceError> {
        if let Some(existing_id) = self.proposal_events.get(&proposal.source_event_id) {
            let existing = &self
                .proposals
                .get(existing_id)
                .ok_or(GovernanceError::InternalInvariant(
                    "proposal event index is stale",
                ))?
                .proposal;
            return if existing == &proposal {
                Ok(ProposalSubmissionOutcome::Replayed)
            } else {
                Err(GovernanceError::ProposalEventConflict(
                    proposal.source_event_id.clone(),
                ))
            };
        }
        if let Some(existing) = self.proposals.get(proposal.id()) {
            return if existing.proposal == proposal {
                Ok(ProposalSubmissionOutcome::Replayed)
            } else {
                Err(GovernanceError::ProposalIdConflict(proposal.id().clone()))
            };
        }
        let policy = self.policy(&proposal.policy_id, proposal.policy_version)?;
        validate_proposal(registry, policy, &proposal)?;
        let proposal_id = proposal.id().clone();
        let source_event_id = proposal.source_event_id.clone();
        let record = ProposalRecord {
            proposal,
            status: ProposalStatus::Pending,
            approvals_by_party: BTreeMap::new(),
            authorization: None,
        };
        self.proposal_events
            .insert(source_event_id, proposal_id.clone());
        self.proposals.insert(proposal_id, record);
        Ok(ProposalSubmissionOutcome::Submitted)
    }

    #[must_use]
    pub fn proposal_count(&self) -> usize {
        self.proposals.len()
    }

    #[must_use]
    pub fn approval_count(&self) -> usize {
        self.approvals.len()
    }

    #[must_use]
    pub fn proposal_status(&self, id: &SpendingProposalId) -> Option<ProposalStatus> {
        self.proposals.get(id).map(|record| record.status)
    }

    #[must_use]
    pub fn authorization(&self, id: &SpendingProposalId) -> Option<&ApprovedSpendingAuthorization> {
        self.proposals.get(id)?.authorization.as_ref()
    }

    pub fn approve(
        &mut self,
        registry: &CommunityRegistry,
        approval: SpendingApproval,
    ) -> Result<ApprovalOutcome, GovernanceError> {
        if let Some(existing_id) = self.approval_events.get(&approval.source_event_id) {
            let existing =
                self.approvals
                    .get(existing_id)
                    .ok_or(GovernanceError::InternalInvariant(
                        "approval event index is stale",
                    ))?;
            if existing != &approval {
                return Err(GovernanceError::ApprovalEventConflict(
                    approval.source_event_id.clone(),
                ));
            }
            let status = self
                .proposal_status(&approval.proposal_id)
                .ok_or_else(|| GovernanceError::UnknownProposal(approval.proposal_id.clone()))?;
            return Ok(ApprovalOutcome::Replayed { status });
        }
        if let Some(existing) = self.approvals.get(approval.id()) {
            if existing != &approval {
                return Err(GovernanceError::ApprovalIdConflict(approval.id().clone()));
            }
            let status = self
                .proposal_status(&approval.proposal_id)
                .ok_or_else(|| GovernanceError::UnknownProposal(approval.proposal_id.clone()))?;
            return Ok(ApprovalOutcome::Replayed { status });
        }
        let record = self
            .proposals
            .get(&approval.proposal_id)
            .ok_or_else(|| GovernanceError::UnknownProposal(approval.proposal_id.clone()))?;
        if record.status == ProposalStatus::Approved {
            return Err(GovernanceError::ProposalAlreadyApproved(
                approval.proposal_id.clone(),
            ));
        }
        let proposal = record.proposal.clone();
        if record
            .approvals_by_party
            .contains_key(&approval.approver_party_id)
        {
            return Err(GovernanceError::DuplicatePartyApproval {
                proposal_id: approval.proposal_id.clone(),
                party_id: approval.approver_party_id.clone(),
            });
        }
        let policy = self
            .policy(&proposal.policy_id, proposal.policy_version)?
            .clone();
        validate_approval(registry, &proposal, &policy, &approval)?;

        let approval_id = approval.id().clone();
        let source_event_id = approval.source_event_id.clone();
        let approver_party_id = approval.approver_party_id.clone();
        let approved_at_unix_ms = self
            .approvals
            .values()
            .filter(|existing| existing.proposal_id == approval.proposal_id)
            .map(|existing| existing.approved_at_unix_ms)
            .chain(std::iter::once(approval.approved_at_unix_ms))
            .max()
            .ok_or(GovernanceError::InternalInvariant(
                "approval quorum timestamp is unavailable",
            ))?;
        self.approvals.insert(approval_id.clone(), approval);
        self.approval_events
            .insert(source_event_id, approval_id.clone());

        let record =
            self.proposals
                .get_mut(&proposal.id)
                .ok_or(GovernanceError::InternalInvariant(
                    "proposal vanished during approval",
                ))?;
        record
            .approvals_by_party
            .insert(approver_party_id, approval_id);
        let required = usize::from(policy.required_approvals());
        if record.approvals_by_party.len() >= required {
            let approver_party_ids = record.approvals_by_party.keys().cloned().collect();
            record.status = ProposalStatus::Approved;
            record.authorization = Some(ApprovedSpendingAuthorization {
                proposal_id: proposal.id.clone(),
                policy_id: proposal.policy_id.clone(),
                policy_version: proposal.policy_version,
                organization_id: proposal.organization_id.clone(),
                community_id: proposal.community_id.clone(),
                fund_id: proposal.fund_id.clone(),
                currency: proposal.currency,
                amount_minor: proposal.amount_minor,
                purpose_reference: proposal.purpose_reference.clone(),
                approver_party_ids,
                approved_at_unix_ms,
            });
        }
        Ok(ApprovalOutcome::Recorded {
            status: record.status,
        })
    }

    fn policy(
        &self,
        id: &SpendingApprovalPolicyId,
        version: u32,
    ) -> Result<&SpendingApprovalPolicy, GovernanceError> {
        self.policies.get(&(id.clone(), version)).ok_or_else(|| {
            GovernanceError::UnknownPolicyVersion {
                policy_id: id.clone(),
                version,
            }
        })
    }
}
fn validate_policy_boundary(
    registry: &CommunityRegistry,
    policy: &SpendingApprovalPolicy,
) -> Result<(), GovernanceError> {
    let community = registry
        .community(policy.community_id())
        .ok_or_else(|| GovernanceError::UnknownCommunity(policy.community_id().clone()))?;
    let fund = registry
        .fund(policy.fund_id())
        .ok_or_else(|| GovernanceError::UnknownFund(policy.fund_id().clone()))?;
    if fund.community_id() != policy.community_id() {
        return Err(GovernanceError::FundCommunityMismatch {
            fund_id: policy.fund_id().clone(),
            expected: policy.community_id().clone(),
            actual: fund.community_id().clone(),
        });
    }
    if community.organization_id() != policy.organization_id() {
        return Err(GovernanceError::OrganizationMismatch {
            expected: policy.organization_id().clone(),
            actual: community.organization_id().clone(),
        });
    }
    if fund.currency() != policy.currency() {
        return Err(GovernanceError::CurrencyMismatch {
            expected: fund.currency(),
            actual: policy.currency(),
        });
    }
    Ok(())
}
fn validate_proposal(
    registry: &CommunityRegistry,
    policy: &SpendingApprovalPolicy,
    proposal: &SpendingProposal,
) -> Result<(), GovernanceError> {
    if proposal.organization_id != policy.organization_id
        || proposal.community_id != policy.community_id
        || proposal.fund_id != policy.fund_id
        || proposal.currency != policy.currency
    {
        return Err(GovernanceError::ProposalPolicyBoundaryMismatch);
    }
    if proposal.amount_minor > policy.max_amount_minor() {
        return Err(GovernanceError::ProposalAmountExceedsPolicy {
            amount: proposal.amount_minor,
            maximum: policy.max_amount_minor(),
        });
    }
    let membership = registry
        .membership_for(&proposal.community_id, &proposal.requester_party_id)
        .ok_or_else(|| GovernanceError::MissingActiveMembership {
            community_id: proposal.community_id.clone(),
            party_id: proposal.requester_party_id.clone(),
        })?;
    if membership.status() != MembershipStatus::Active {
        return Err(GovernanceError::InactiveMembership {
            community_id: proposal.community_id.clone(),
            party_id: proposal.requester_party_id.clone(),
        });
    }
    Ok(())
}
fn validate_approval(
    registry: &CommunityRegistry,
    proposal: &SpendingProposal,
    policy: &SpendingApprovalPolicy,
    approval: &SpendingApproval,
) -> Result<(), GovernanceError> {
    if approval.approved_at_unix_ms < proposal.created_at_unix_ms {
        return Err(GovernanceError::ApprovalBeforeProposalCreation);
    }
    if approval.approved_at_unix_ms > proposal.expires_at_unix_ms {
        return Err(GovernanceError::ApprovalAfterProposalExpiry);
    }
    let membership = registry
        .membership_for(&proposal.community_id, &approval.approver_party_id)
        .ok_or_else(|| GovernanceError::MissingActiveMembership {
            community_id: proposal.community_id.clone(),
            party_id: approval.approver_party_id.clone(),
        })?;
    if membership.status() != MembershipStatus::Active {
        return Err(GovernanceError::InactiveMembership {
            community_id: proposal.community_id.clone(),
            party_id: approval.approver_party_id.clone(),
        });
    }
    if !policy.role_is_eligible(membership.role()) {
        return Err(GovernanceError::IneligibleApprovalRole {
            party_id: approval.approver_party_id.clone(),
            role: membership.role(),
        });
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernanceError {
    EmptyIdentifier(&'static str),
    InvalidPolicyVersion,
    InvalidPolicyMaxAmount(i128),
    InvalidApprovalQuorum,
    EmptyEligibleRoles,
    UnknownCommunity(CommunityId),
    UnknownFund(FundId),
    FundCommunityMismatch {
        fund_id: FundId,
        expected: CommunityId,
        actual: CommunityId,
    },
    OrganizationMismatch {
        expected: OrganizationId,
        actual: OrganizationId,
    },
    CurrencyMismatch {
        expected: Currency,
        actual: Currency,
    },
    DuplicatePolicyVersion {
        policy_id: SpendingApprovalPolicyId,
        version: u32,
    },
    NonMonotonicPolicyVersion {
        policy_id: SpendingApprovalPolicyId,
        highest: u32,
        attempted: u32,
    },
    InvalidProposalAmount(i128),
    EmptyPurposeReference,
    InvalidProposalExpiry {
        created_at_unix_ms: i64,
        expires_at_unix_ms: i64,
    },
    UnknownPolicyVersion {
        policy_id: SpendingApprovalPolicyId,
        version: u32,
    },
    ProposalPolicyBoundaryMismatch,
    ProposalAmountExceedsPolicy {
        amount: i128,
        maximum: i128,
    },
    MissingActiveMembership {
        community_id: CommunityId,
        party_id: PartyId,
    },
    InactiveMembership {
        community_id: CommunityId,
        party_id: PartyId,
    },
    ProposalEventConflict(SpendingProposalEventId),
    ProposalIdConflict(SpendingProposalId),
    UnknownProposal(SpendingProposalId),
    ProposalAlreadyApproved(SpendingProposalId),
    DuplicatePartyApproval {
        proposal_id: SpendingProposalId,
        party_id: PartyId,
    },
    ApprovalEventConflict(SpendingApprovalEventId),
    ApprovalIdConflict(SpendingApprovalId),
    ApprovalBeforeProposalCreation,
    ApprovalAfterProposalExpiry,
    IneligibleApprovalRole {
        party_id: PartyId,
        role: MembershipRole,
    },
    InternalInvariant(&'static str),
}

impl Display for GovernanceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(name) => write!(f, "{name} must not be empty"),
            Self::InvalidPolicyVersion => write!(f, "policy version must be non-zero"),
            Self::InvalidPolicyMaxAmount(value) => {
                write!(f, "policy maximum must be positive: {value}")
            }
            Self::InvalidApprovalQuorum => write!(f, "required approvals must be at least one"),
            Self::EmptyEligibleRoles => write!(f, "eligible approval roles must not be empty"),
            Self::UnknownCommunity(id) => write!(f, "unknown community: {}", id.as_str()),
            Self::UnknownFund(id) => write!(f, "unknown fund: {}", id.as_str()),
            Self::FundCommunityMismatch { fund_id, .. } => write!(
                f,
                "fund {} is not bound to policy community",
                fund_id.as_str()
            ),
            Self::OrganizationMismatch { .. } => write!(
                f,
                "policy organization does not match community organization"
            ),
            Self::CurrencyMismatch { expected, actual } => {
                write!(f, "currency mismatch: expected {expected}, got {actual}")
            }
            Self::DuplicatePolicyVersion { policy_id, version } => write!(
                f,
                "policy {} version {version} already exists",
                policy_id.as_str()
            ),
            Self::NonMonotonicPolicyVersion {
                policy_id,
                highest,
                attempted,
            } => write!(
                f,
                "policy {} version {attempted} must exceed {highest}",
                policy_id.as_str()
            ),
            Self::InvalidProposalAmount(value) => {
                write!(f, "proposal amount must be positive: {value}")
            }
            Self::EmptyPurposeReference => {
                write!(f, "proposal purpose reference must not be empty")
            }
            Self::InvalidProposalExpiry { .. } => {
                write!(f, "proposal expiry must be after creation")
            }
            Self::UnknownPolicyVersion { policy_id, version } => {
                write!(f, "unknown policy {} version {version}", policy_id.as_str())
            }
            Self::ProposalPolicyBoundaryMismatch => {
                write!(f, "proposal boundary does not match policy")
            }
            Self::ProposalAmountExceedsPolicy { amount, maximum } => write!(
                f,
                "proposal amount {amount} exceeds policy maximum {maximum}"
            ),
            Self::MissingActiveMembership {
                party_id,
                community_id,
            } => write!(
                f,
                "party {} is not a member of community {}",
                party_id.as_str(),
                community_id.as_str()
            ),
            Self::InactiveMembership {
                party_id,
                community_id,
            } => write!(
                f,
                "party {} has inactive membership in community {}",
                party_id.as_str(),
                community_id.as_str()
            ),
            Self::ProposalEventConflict(id) => {
                write!(f, "proposal source event conflict: {}", id.as_str())
            }
            Self::ProposalIdConflict(id) => write!(f, "proposal ID conflict: {}", id.as_str()),
            Self::UnknownProposal(id) => write!(f, "unknown proposal: {}", id.as_str()),
            Self::ProposalAlreadyApproved(id) => {
                write!(f, "proposal is already approved: {}", id.as_str())
            }
            Self::DuplicatePartyApproval {
                proposal_id,
                party_id,
            } => write!(
                f,
                "party {} already approved proposal {}",
                party_id.as_str(),
                proposal_id.as_str()
            ),
            Self::ApprovalEventConflict(id) => {
                write!(f, "approval source event conflict: {}", id.as_str())
            }
            Self::ApprovalIdConflict(id) => write!(f, "approval ID conflict: {}", id.as_str()),
            Self::ApprovalBeforeProposalCreation => {
                write!(f, "approval predates proposal creation")
            }
            Self::ApprovalAfterProposalExpiry => write!(f, "approval is after proposal expiry"),
            Self::IneligibleApprovalRole { party_id, role } => write!(
                f,
                "party {} has ineligible approval role {role:?}",
                party_id.as_str()
            ),
            Self::InternalInvariant(message) => {
                write!(f, "internal governance invariant failed: {message}")
            }
        }
    }
}

impl Error for GovernanceError {}
