#![allow(clippy::unwrap_used)]

use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundId, Membership, MembershipId,
    MembershipRole, MembershipStatus, Organization, OrganizationId, Party, PartyId, PartyKind,
};
use cofi_disbursements::{
    BeneficiaryReference, DestinationReference, DisbursementCreation, DisbursementEngine,
    DisbursementEventId, DisbursementId, DisbursementStatus, DisbursementSubmission,
    DisbursementTerminalEvent, FailureCode, ProviderEventReference, ProviderRequestReference,
    ProviderSettlementReference, TerminalOutcome,
};
use cofi_governance::{
    GovernanceEngine, SpendingApproval, SpendingApprovalEventId, SpendingApprovalId,
    SpendingApprovalPolicy, SpendingApprovalPolicyId, SpendingProposal, SpendingProposalEventId,
    SpendingProposalId,
};
use cofi_ledger::{
    Account, AccountId, AccountKind, Currency, EntryMetadata, JournalEntry, JournalEntryId, Ledger,
    LedgerScopeId, Posting, Side,
};
use cofi_provider_contract::{
    CommunityReference, FundReference, OrganizationReference, PROVIDER_IDEMPOTENCY_NAMESPACE,
    ProposalReference, ProviderContract, ProviderContractError, ProviderIdempotencyKey,
    ProviderObservation,
};
use cofi_spending::{ApprovedFundSpendEvent, FundSpendBridge, FundSpendEventId, FundSpendId};

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}
fn org(value: &str) -> OrganizationId {
    OrganizationId::new(value).unwrap()
}
fn community(value: &str) -> CommunityId {
    CommunityId::new(value).unwrap()
}
fn fund(value: &str) -> FundId {
    FundId::new(value).unwrap()
}
fn party(value: &str) -> PartyId {
    PartyId::new(value).unwrap()
}
fn membership(value: &str) -> MembershipId {
    MembershipId::new(value).unwrap()
}
fn account(value: &str) -> AccountId {
    AccountId::new(value).unwrap()
}
fn scope(value: &str) -> LedgerScopeId {
    LedgerScopeId::new(value).unwrap()
}
fn policy_id(value: &str) -> SpendingApprovalPolicyId {
    SpendingApprovalPolicyId::new(value).unwrap()
}
fn proposal_id(value: &str) -> SpendingProposalId {
    SpendingProposalId::new(value).unwrap()
}
fn disbursement_id(value: &str) -> DisbursementId {
    DisbursementId::new(value).unwrap()
}
fn event_id(value: &str) -> DisbursementEventId {
    DisbursementEventId::new(value).unwrap()
}
fn beneficiary(value: &str) -> BeneficiaryReference {
    BeneficiaryReference::new(value).unwrap()
}
fn destination(value: &str) -> DestinationReference {
    DestinationReference::new(value).unwrap()
}
fn request(value: &str) -> ProviderRequestReference {
    ProviderRequestReference::new(value).unwrap()
}
fn provider_event(value: &str) -> ProviderEventReference {
    ProviderEventReference::new(value).unwrap()
}
fn settlement(value: &str) -> ProviderSettlementReference {
    ProviderSettlementReference::new(value).unwrap()
}
fn failure(value: &str) -> FailureCode {
    FailureCode::new(value).unwrap()
}

fn register_account(ledger: &mut Ledger, id: &str, kind: AccountKind) {
    ledger
        .register_account(Account::new(account(id), scope("org-1"), kind, usd()))
        .unwrap();
}

fn seed_fund(ledger: &mut Ledger) {
    let metadata = EntryMetadata::new(
        Some("seed-fund".to_owned()),
        Some("seed-fund-event".to_owned()),
    )
    .unwrap()
    .with_business_key(Some("seed-fund-business".to_owned()))
    .unwrap();
    ledger
        .commit(
            JournalEntry::new(
                JournalEntryId::new("seed-fund-entry").unwrap(),
                vec![
                    Posting::new(account("fund-account"), usd(), Side::Debit, 1_000).unwrap(),
                    Posting::new(account("opening-equity"), usd(), Side::Credit, 1_000).unwrap(),
                ],
                500,
                500,
                metadata,
            )
            .unwrap(),
        )
        .unwrap();
}

fn add_member(registry: &mut CommunityRegistry, id: &str, role: MembershipRole) {
    registry
        .register_party(Party::new(party(id), org("org-1"), PartyKind::Person))
        .unwrap();
    registry
        .register_membership(Membership::new(
            membership(&format!("membership-{id}")),
            party(id),
            community("community-1"),
            role,
            MembershipStatus::Active,
        ))
        .unwrap();
}

struct Fixture {
    registry: CommunityRegistry,
    governance: GovernanceEngine,
    ledger: Ledger,
    spend1: ApprovedFundSpendEvent,
    spend2: ApprovedFundSpendEvent,
}

fn fixture() -> Fixture {
    let mut ledger = Ledger::new();
    register_account(&mut ledger, "fund-account", AccountKind::Asset);
    register_account(&mut ledger, "expense-account", AccountKind::Expense);
    register_account(&mut ledger, "opening-equity", AccountKind::Equity);
    seed_fund(&mut ledger);

    let mut registry = CommunityRegistry::new();
    registry
        .register_organization(Organization::new(org("org-1")))
        .unwrap();
    registry
        .register_community(Community::new(community("community-1"), org("org-1")))
        .unwrap();
    registry
        .register_fund(
            Fund::new(
                fund("fund-1"),
                community("community-1"),
                account("fund-account"),
                usd(),
            ),
            &ledger,
        )
        .unwrap();
    add_member(&mut registry, "owner", MembershipRole::Owner);
    add_member(&mut registry, "treasurer", MembershipRole::Treasurer);

    let mut governance = GovernanceEngine::new();
    governance
        .register_policy(
            &registry,
            SpendingApprovalPolicy::new(
                policy_id("policy-1"),
                1,
                org("org-1"),
                community("community-1"),
                fund("fund-1"),
                usd(),
                1_000,
                2,
                vec![MembershipRole::Owner, MembershipRole::Treasurer],
            )
            .unwrap(),
        )
        .unwrap();

    let spend1 = authorize_spend(&registry, &mut governance, 1, 100, 1_500);
    let spend2 = authorize_spend(&registry, &mut governance, 2, 120, 1_600);
    FundSpendBridge::new()
        .apply(&registry, &governance, &spend1, &mut ledger)
        .unwrap();
    FundSpendBridge::new()
        .apply(&registry, &governance, &spend2, &mut ledger)
        .unwrap();

    Fixture {
        registry,
        governance,
        ledger,
        spend1,
        spend2,
    }
}

fn authorize_spend(
    registry: &CommunityRegistry,
    governance: &mut GovernanceEngine,
    number: u8,
    amount: i128,
    executed_at: i64,
) -> ApprovedFundSpendEvent {
    let proposal = format!("proposal-{number}");
    governance
        .submit_proposal(
            registry,
            SpendingProposal::new(
                SpendingProposalEventId::new(format!("proposal-event-{number}")).unwrap(),
                proposal_id(&proposal),
                policy_id("policy-1"),
                1,
                party("owner"),
                org("org-1"),
                community("community-1"),
                fund("fund-1"),
                usd(),
                amount,
                format!("vendor:invoice-{number}"),
                1_000,
                2_000,
            )
            .unwrap(),
        )
        .unwrap();

    for (suffix, who, approved_at) in [
        ("owner", "owner", executed_at - 100),
        ("treasurer", "treasurer", executed_at - 50),
    ] {
        governance
            .approve(
                registry,
                SpendingApproval::new(
                    SpendingApprovalEventId::new(format!("approval-event-{number}-{suffix}"))
                        .unwrap(),
                    SpendingApprovalId::new(format!("approval-{number}-{suffix}")).unwrap(),
                    proposal_id(&proposal),
                    party(who),
                    approved_at,
                ),
            )
            .unwrap();
    }

    ApprovedFundSpendEvent::new(
        FundSpendEventId::new(format!("spend-event-{number}")).unwrap(),
        FundSpendId::new(format!("spend-{number}")).unwrap(),
        proposal_id(&proposal),
        org("org-1"),
        community("community-1"),
        fund("fund-1"),
        usd(),
        amount,
        format!("vendor:invoice-{number}"),
        account("expense-account"),
        executed_at,
        executed_at,
    )
    .unwrap()
}
fn create_ready(
    engine: &mut DisbursementEngine,
    fixture: &Fixture,
    spend: &ApprovedFundSpendEvent,
    id: &str,
    created_at: i64,
) {
    let creation = DisbursementCreation::new(
        event_id(&format!("create-{id}")),
        disbursement_id(id),
        beneficiary(&format!("beneficiary-{id}")),
        destination(&format!("destination-{id}")),
        created_at,
    );
    engine
        .create(
            &fixture.registry,
            &fixture.governance,
            &fixture.ledger,
            spend,
            creation,
        )
        .unwrap();
}

fn submit_ready(
    engine: &mut DisbursementEngine,
    id: &str,
    request_reference: &str,
    submitted_at: i64,
) {
    engine
        .submit(DisbursementSubmission::new(
            event_id(&format!("submit-{id}")),
            disbursement_id(id),
            request(request_reference),
            submitted_at,
        ))
        .unwrap();
}

fn ledger_snapshot(fixture: &Fixture) -> (usize, u128, u128) {
    let balance = fixture.ledger.balance(&account("fund-account")).unwrap();
    (
        fixture.ledger.entry_count(),
        balance.debits(),
        balance.credits(),
    )
}

#[test]
fn submitted_disbursement_builds_exact_deterministic_request() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let mut engine = DisbursementEngine::new();
    create_ready(&mut engine, &fixture, &fixture.spend1, "d-1", 1_500);
    submit_ready(&mut engine, "d-1", "request-1", 1_510);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let contract = ProviderContract::new();
    let request1 = contract.build_request(record).unwrap();
    let request2 = contract.build_request(record).unwrap();

    assert_eq!(request1, request2);
    assert_eq!(request1.disbursement_id().as_str(), "d-1");
    assert_eq!(request1.spend_id().as_str(), "spend-1");
    assert_eq!(
        request1.journal_entry_id().as_str(),
        "spending:fund-spend:spend-1"
    );
    assert_eq!(request1.proposal_reference().as_str(), "proposal-1");
    assert_eq!(request1.organization_reference().as_str(), "org-1");
    assert_eq!(request1.community_reference().as_str(), "community-1");
    assert_eq!(request1.fund_reference().as_str(), "fund-1");
    assert_eq!(request1.currency(), usd());
    assert_eq!(request1.amount_minor(), 100);
    assert_eq!(request1.purpose_reference(), "vendor:invoice-1");
    assert_eq!(request1.beneficiary_reference().as_str(), "beneficiary-d-1");
    assert_eq!(request1.destination_reference().as_str(), "destination-d-1");
    assert_eq!(request1.provider_request_reference().as_str(), "request-1");
    assert_eq!(request1.submitted_at_unix_ms(), 1_510);
    assert_eq!(
        request1.idempotency_key().as_str(),
        "cofi-provider:v1:d3:d-1:s7:spend-1:j27:spending:fund-spend:spend-1"
    );
    assert_eq!(ledger_snapshot(&fixture), before);
    assert_eq!(record.status(), DisbursementStatus::Submitted);
}

#[test]
fn distinct_disbursements_have_distinct_stable_idempotency_keys() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(&mut engine, &fixture, &fixture.spend1, "d-1", 1_500);
    create_ready(&mut engine, &fixture, &fixture.spend2, "d-2", 1_600);
    submit_ready(&mut engine, "d-1", "request-1", 1_510);
    submit_ready(&mut engine, "d-2", "request-2", 1_610);

    let contract = ProviderContract::new();
    let first = contract
        .build_request(engine.disbursement(&disbursement_id("d-1")).unwrap())
        .unwrap();
    let second = contract
        .build_request(engine.disbursement(&disbursement_id("d-2")).unwrap())
        .unwrap();
    assert_ne!(first.idempotency_key(), second.idempotency_key());
    assert_eq!(PROVIDER_IDEMPOTENCY_NAMESPACE, "cofi-provider:v1");
    assert_eq!(
        contract
            .build_request(engine.disbursement(&disbursement_id("d-1")).unwrap())
            .unwrap()
            .idempotency_key(),
        first.idempotency_key()
    );
}

#[test]
fn ready_settled_and_failed_states_cannot_build_provider_requests() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    let contract = ProviderContract::new();
    create_ready(&mut engine, &fixture, &fixture.spend1, "d-1", 1_500);

    let ready_error = contract
        .build_request(engine.disbursement(&disbursement_id("d-1")).unwrap())
        .unwrap_err();
    assert!(matches!(
        ready_error,
        ProviderContractError::InvalidDisbursementStatus {
            status: DisbursementStatus::Ready,
            ..
        }
    ));
    submit_ready(&mut engine, "d-1", "request-1", 1_510);
    engine
        .record_terminal(DisbursementTerminalEvent::settled(
            event_id("terminal-d-1"),
            disbursement_id("d-1"),
            provider_event("provider-event-d-1"),
            settlement("settlement-d-1"),
            1_520,
        ))
        .unwrap();
    let settled_error = contract
        .build_request(engine.disbursement(&disbursement_id("d-1")).unwrap())
        .unwrap_err();
    assert!(matches!(
        settled_error,
        ProviderContractError::InvalidDisbursementStatus {
            status: DisbursementStatus::Settled,
            ..
        }
    ));

    create_ready(&mut engine, &fixture, &fixture.spend2, "d-2", 1_600);
    submit_ready(&mut engine, "d-2", "request-2", 1_610);
    engine
        .record_terminal(DisbursementTerminalEvent::failed(
            event_id("terminal-d-2"),
            disbursement_id("d-2"),
            provider_event("provider-event-d-2"),
            failure("declined"),
            1_620,
        ))
        .unwrap();
    let failed_error = contract
        .build_request(engine.disbursement(&disbursement_id("d-2")).unwrap())
        .unwrap_err();
    assert!(matches!(
        failed_error,
        ProviderContractError::InvalidDisbursementStatus {
            status: DisbursementStatus::Failed,
            ..
        }
    ));
}

#[test]
fn request_validation_rejects_a_different_canonical_disbursement_snapshot() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(&mut engine, &fixture, &fixture.spend1, "d-1", 1_500);
    create_ready(&mut engine, &fixture, &fixture.spend2, "d-2", 1_600);
    submit_ready(&mut engine, "d-1", "request-1", 1_510);
    submit_ready(&mut engine, "d-2", "request-2", 1_610);
    let contract = ProviderContract::new();
    let request1 = contract
        .build_request(engine.disbursement(&disbursement_id("d-1")).unwrap())
        .unwrap();
    let error = contract
        .validate_request(
            engine.disbursement(&disbursement_id("d-2")).unwrap(),
            &request1,
        )
        .unwrap_err();
    assert_eq!(error, ProviderContractError::RequestSnapshotMismatch);
}

#[test]
fn accepted_observation_validates_binding_but_is_not_terminal() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let mut engine = DisbursementEngine::new();
    create_ready(&mut engine, &fixture, &fixture.spend1, "d-1", 1_500);
    submit_ready(&mut engine, "d-1", "request-1", 1_510);
    let observation = ProviderObservation::accepted(
        disbursement_id("d-1"),
        request("request-1"),
        provider_event("accepted-1"),
        1_515,
    );
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let contract = ProviderContract::new();
    contract.validate_observation(record, &observation).unwrap();
    assert_eq!(
        contract
            .to_terminal_event(record, &observation)
            .unwrap_err(),
        ProviderContractError::AcceptedObservationIsNotTerminal
    );
    assert_eq!(record.status(), DisbursementStatus::Submitted);
    assert_eq!(ledger_snapshot(&fixture), before);
}

#[test]
fn settled_observation_converts_deterministically_and_applies_to_p12() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let mut engine = DisbursementEngine::new();
    create_ready(&mut engine, &fixture, &fixture.spend1, "d-1", 1_500);
    submit_ready(&mut engine, "d-1", "request-1", 1_510);
    let observation = ProviderObservation::settled(
        disbursement_id("d-1"),
        request("request-1"),
        event_id("provider-terminal-1"),
        provider_event("provider-event-1"),
        settlement("settlement-1"),
        1_520,
    );
    let contract = ProviderContract::new();
    let terminal1 = {
        let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
        contract.to_terminal_event(record, &observation).unwrap()
    };
    let terminal2 = {
        let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
        contract.to_terminal_event(record, &observation).unwrap()
    };
    assert_eq!(terminal1, terminal2);
    assert_eq!(
        engine.record_terminal(terminal1).unwrap(),
        TerminalOutcome::Settled
    );
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    assert_eq!(record.status(), DisbursementStatus::Settled);
    assert_eq!(
        record.provider_settlement_reference().unwrap().as_str(),
        "settlement-1"
    );
    assert_eq!(ledger_snapshot(&fixture), before);
}

#[test]
fn failed_observation_converts_deterministically_and_applies_to_p12() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let mut engine = DisbursementEngine::new();
    create_ready(&mut engine, &fixture, &fixture.spend1, "d-1", 1_500);
    submit_ready(&mut engine, "d-1", "request-1", 1_510);
    let observation = ProviderObservation::failed(
        disbursement_id("d-1"),
        request("request-1"),
        event_id("provider-terminal-fail-1"),
        provider_event("provider-event-fail-1"),
        failure("provider_declined"),
        1_520,
    );
    let contract = ProviderContract::new();
    let terminal1 = {
        let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
        contract.to_terminal_event(record, &observation).unwrap()
    };
    let terminal2 = {
        let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
        contract.to_terminal_event(record, &observation).unwrap()
    };
    assert_eq!(terminal1, terminal2);
    assert_eq!(
        engine.record_terminal(terminal1).unwrap(),
        TerminalOutcome::Failed
    );
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    assert_eq!(record.status(), DisbursementStatus::Failed);
    assert_eq!(record.failure_code().unwrap().as_str(), "provider_declined");
    assert_eq!(ledger_snapshot(&fixture), before);
}

#[test]
fn wrong_observation_binding_and_pre_submission_time_fail_closed() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let mut engine = DisbursementEngine::new();
    create_ready(&mut engine, &fixture, &fixture.spend1, "d-1", 1_500);
    submit_ready(&mut engine, "d-1", "request-1", 1_510);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let contract = ProviderContract::new();

    let wrong_id = ProviderObservation::accepted(
        disbursement_id("d-other"),
        request("request-1"),
        provider_event("accepted-wrong-id"),
        1_515,
    );
    assert_eq!(
        contract
            .validate_observation(record, &wrong_id)
            .unwrap_err(),
        ProviderContractError::ObservationBindingMismatch("disbursement_id")
    );
    let wrong_request = ProviderObservation::accepted(
        disbursement_id("d-1"),
        request("request-wrong"),
        provider_event("accepted-wrong-request"),
        1_515,
    );
    assert_eq!(
        contract
            .validate_observation(record, &wrong_request)
            .unwrap_err(),
        ProviderContractError::ObservationBindingMismatch("provider_request_reference")
    );

    let early = ProviderObservation::accepted(
        disbursement_id("d-1"),
        request("request-1"),
        provider_event("accepted-early"),
        1_509,
    );
    assert!(matches!(
        contract.validate_observation(record, &early),
        Err(ProviderContractError::ObservationBeforeSubmission {
            submitted_at_unix_ms: 1_510,
            occurred_at_unix_ms: 1_509,
        })
    ));
    assert_eq!(record.status(), DisbursementStatus::Submitted);
    assert_eq!(ledger_snapshot(&fixture), before);
}
#[test]
fn exact_canonical_request_validates_against_same_submitted_record() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let mut engine = DisbursementEngine::new();
    create_ready(&mut engine, &fixture, &fixture.spend1, "d-1", 1_500);
    submit_ready(&mut engine, "d-1", "request-1", 1_510);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let contract = ProviderContract::new();
    let request = contract.build_request(record).unwrap();
    contract.validate_request(record, &request).unwrap();
    assert_eq!(record.status(), DisbursementStatus::Submitted);
    assert_eq!(ledger_snapshot(&fixture), before);
}

#[test]
fn ready_disbursement_rejects_provider_observations() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(&mut engine, &fixture, &fixture.spend1, "d-1", 1_500);
    let observation = ProviderObservation::accepted(
        disbursement_id("d-1"),
        request("request-not-submitted"),
        provider_event("accepted-before-submit"),
        1_500,
    );
    let error = ProviderContract::new()
        .validate_observation(
            engine.disbursement(&disbursement_id("d-1")).unwrap(),
            &observation,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ProviderContractError::InvalidDisbursementStatus {
            status: DisbursementStatus::Ready,
            ..
        }
    ));
}

#[test]
fn all_public_contract_identifiers_reject_empty_values() {
    assert!(ProviderIdempotencyKey::new(" ").is_err());
    assert!(ProposalReference::new("").is_err());
    assert!(OrganizationReference::new(" ").is_err());
    assert!(CommunityReference::new("").is_err());
    assert!(FundReference::new(" ").is_err());
    assert!(ProviderRequestReference::new(" ").is_err());
    assert!(ProviderEventReference::new("").is_err());
    assert!(ProviderSettlementReference::new(" ").is_err());
    assert!(FailureCode::new("").is_err());
}
