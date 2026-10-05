#![allow(clippy::unwrap_used)]

use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundId, Membership, MembershipId,
    MembershipRole, MembershipStatus, Organization, OrganizationId, Party, PartyId, PartyKind,
};
use cofi_disbursements::{
    BeneficiaryReference, CreationOutcome, DestinationReference, DisbursementCreation,
    DisbursementEngine, DisbursementError, DisbursementEventId, DisbursementId, DisbursementStatus,
    DisbursementSubmission, DisbursementTerminalEvent, FailureCode, ProviderEventReference,
    ProviderRequestReference, ProviderSettlementReference, SubmissionOutcome, TerminalOutcome,
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
use cofi_spending::{
    ApprovedFundSpendEvent, FundSpendBridge, FundSpendError, FundSpendEventId, FundSpendId,
};
fn usd() -> Currency {
    Currency::new("USD").unwrap()
}
fn org(v: &str) -> OrganizationId {
    OrganizationId::new(v).unwrap()
}
fn community(v: &str) -> CommunityId {
    CommunityId::new(v).unwrap()
}
fn fund(v: &str) -> FundId {
    FundId::new(v).unwrap()
}
fn party(v: &str) -> PartyId {
    PartyId::new(v).unwrap()
}
fn membership(v: &str) -> MembershipId {
    MembershipId::new(v).unwrap()
}
fn account(v: &str) -> AccountId {
    AccountId::new(v).unwrap()
}
fn scope(v: &str) -> LedgerScopeId {
    LedgerScopeId::new(v).unwrap()
}
fn policy_id(v: &str) -> SpendingApprovalPolicyId {
    SpendingApprovalPolicyId::new(v).unwrap()
}
fn proposal_id(v: &str) -> SpendingProposalId {
    SpendingProposalId::new(v).unwrap()
}
fn disbursement_id(v: &str) -> DisbursementId {
    DisbursementId::new(v).unwrap()
}
fn event_id(v: &str) -> DisbursementEventId {
    DisbursementEventId::new(v).unwrap()
}
fn beneficiary(v: &str) -> BeneficiaryReference {
    BeneficiaryReference::new(v).unwrap()
}
fn destination(v: &str) -> DestinationReference {
    DestinationReference::new(v).unwrap()
}
fn request(v: &str) -> ProviderRequestReference {
    ProviderRequestReference::new(v).unwrap()
}
fn provider_event(v: &str) -> ProviderEventReference {
    ProviderEventReference::new(v).unwrap()
}
fn settlement(v: &str) -> ProviderSettlementReference {
    ProviderSettlementReference::new(v).unwrap()
}
fn failure(v: &str) -> FailureCode {
    FailureCode::new(v).unwrap()
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

fn creation(event: &str, id: &str, created_at: i64) -> DisbursementCreation {
    DisbursementCreation::new(
        event_id(event),
        disbursement_id(id),
        beneficiary("beneficiary-1"),
        destination("destination-token-1"),
        created_at,
    )
}

fn submission(
    event: &str,
    id: &str,
    request_ref: &str,
    submitted_at: i64,
) -> DisbursementSubmission {
    DisbursementSubmission::new(
        event_id(event),
        disbursement_id(id),
        request(request_ref),
        submitted_at,
    )
}

fn create_ready(
    engine: &mut DisbursementEngine,
    fixture: &Fixture,
    spend: &ApprovedFundSpendEvent,
    id: &str,
    event: &str,
    created_at: i64,
) {
    assert_eq!(
        engine
            .create(
                &fixture.registry,
                &fixture.governance,
                &fixture.ledger,
                spend,
                creation(event, id, created_at),
            )
            .unwrap(),
        CreationOutcome::Created
    );
}

fn submit_ready(
    engine: &mut DisbursementEngine,
    id: &str,
    event: &str,
    request_ref: &str,
    submitted_at: i64,
) {
    assert_eq!(
        engine
            .submit(submission(event, id, request_ref, submitted_at))
            .unwrap(),
        SubmissionOutcome::Submitted
    );
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
fn canonical_spend_creates_ready_disbursement_without_ledger_effect() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let mut engine = DisbursementEngine::new();
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-1",
        "create-1",
        1_500,
    );
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    assert_eq!(record.status(), DisbursementStatus::Ready);
    assert_eq!(record.spend().spend_id().as_str(), "spend-1");
    assert_eq!(record.spend().proposal_id().as_str(), "proposal-1");
    assert_eq!(record.spend().amount_minor(), 100);
    assert_eq!(record.spend().purpose_reference(), "vendor:invoice-1");
    assert_eq!(record.beneficiary_reference().as_str(), "beneficiary-1");
    assert_eq!(
        record.destination_reference().as_str(),
        "destination-token-1"
    );
    assert_eq!(ledger_snapshot(&fixture), before);
}

#[test]
fn uncommitted_spend_is_rejected_without_state_or_ledger_effect() {
    let mut fixture = fixture();
    let spend3 = authorize_spend(&fixture.registry, &mut fixture.governance, 3, 90, 1_700);
    let before = ledger_snapshot(&fixture);
    let mut engine = DisbursementEngine::new();
    let error = engine
        .create(
            &fixture.registry,
            &fixture.governance,
            &fixture.ledger,
            &spend3,
            creation("create-3", "d-3", 1_700),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        DisbursementError::SpendVerification(FundSpendError::UncommittedSpend(_))
    ));
    assert!(engine.disbursement(&disbursement_id("d-3")).is_none());
    assert_eq!(ledger_snapshot(&fixture), before);
}

#[test]
fn exact_creation_replay_is_idempotent_even_after_lifecycle_advances() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    let command = creation("create-1", "d-1", 1_500);
    assert_eq!(
        engine
            .create(
                &fixture.registry,
                &fixture.governance,
                &fixture.ledger,
                &fixture.spend1,
                command.clone()
            )
            .unwrap(),
        CreationOutcome::Created
    );
    submit_ready(&mut engine, "d-1", "submit-1", "request-1", 1_510);
    assert_eq!(
        engine
            .create(
                &fixture.registry,
                &fixture.governance,
                &fixture.ledger,
                &fixture.spend1,
                command,
            )
            .unwrap(),
        CreationOutcome::Replayed
    );
    assert_eq!(
        engine
            .disbursement(&disbursement_id("d-1"))
            .unwrap()
            .status(),
        DisbursementStatus::Submitted
    );
}

#[test]
fn one_spend_cannot_create_second_disbursement_history() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-1",
        "create-1",
        1_500,
    );
    let error = engine
        .create(
            &fixture.registry,
            &fixture.governance,
            &fixture.ledger,
            &fixture.spend1,
            creation("create-2", "d-2", 1_501),
        )
        .unwrap_err();
    assert!(matches!(error, DisbursementError::SpendAlreadyBound { .. }));
    assert!(engine.disbursement(&disbursement_id("d-2")).is_none());
}

#[test]
fn creation_id_event_and_timing_conflicts_fail_closed() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-1",
        "create-1",
        1_500,
    );

    let id_conflict = engine
        .create(
            &fixture.registry,
            &fixture.governance,
            &fixture.ledger,
            &fixture.spend2,
            creation("create-2", "d-1", 1_600),
        )
        .unwrap_err();
    assert!(matches!(
        id_conflict,
        DisbursementError::DisbursementIdConflict(_)
    ));

    let event_conflict = engine
        .create(
            &fixture.registry,
            &fixture.governance,
            &fixture.ledger,
            &fixture.spend2,
            creation("create-1", "d-2", 1_600),
        )
        .unwrap_err();
    assert!(matches!(
        event_conflict,
        DisbursementError::CreationEventConflict(_)
    ));

    let early = engine
        .create(
            &fixture.registry,
            &fixture.governance,
            &fixture.ledger,
            &fixture.spend2,
            creation("create-early", "d-early", 1_599),
        )
        .unwrap_err();
    assert!(matches!(
        early,
        DisbursementError::CreationBeforeSpend { .. }
    ));
}

#[test]
fn valid_submission_and_exact_replay_have_zero_ledger_effect() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let mut engine = DisbursementEngine::new();
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-1",
        "create-1",
        1_500,
    );
    let submit = submission("submit-1", "d-1", "request-1", 1_510);
    assert_eq!(
        engine.submit(submit.clone()).unwrap(),
        SubmissionOutcome::Submitted
    );
    assert_eq!(engine.submit(submit).unwrap(), SubmissionOutcome::Replayed);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    assert_eq!(record.status(), DisbursementStatus::Submitted);
    assert_eq!(
        record.provider_request_reference().unwrap().as_str(),
        "request-1"
    );
    assert_eq!(ledger_snapshot(&fixture), before);
}

#[test]
fn provider_request_reference_is_unique_across_disbursements() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-1",
        "create-1",
        1_500,
    );
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend2,
        "d-2",
        "create-2",
        1_600,
    );
    submit_ready(&mut engine, "d-1", "submit-1", "shared-request", 1_510);
    let error = engine
        .submit(submission("submit-2", "d-2", "shared-request", 1_610))
        .unwrap_err();
    assert!(matches!(
        error,
        DisbursementError::ProviderRequestReferenceConflict { .. }
    ));
    assert_eq!(
        engine
            .disbursement(&disbursement_id("d-2"))
            .unwrap()
            .status(),
        DisbursementStatus::Ready
    );
}
#[test]
fn submission_event_identity_and_timing_conflicts_fail_closed() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-1",
        "create-1",
        1_500,
    );
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend2,
        "d-2",
        "create-2",
        1_600,
    );

    let early = engine
        .submit(submission("submit-early", "d-1", "request-early", 1_499))
        .unwrap_err();
    assert!(matches!(
        early,
        DisbursementError::SubmissionBeforeCreation { .. }
    ));

    submit_ready(&mut engine, "d-1", "submit-1", "request-1", 1_510);
    let conflict = engine
        .submit(submission("submit-1", "d-2", "request-2", 1_610))
        .unwrap_err();
    assert!(matches!(
        conflict,
        DisbursementError::SubmissionEventConflict(_)
    ));

    let cross_kind = engine
        .submit(submission("create-2", "d-2", "request-3", 1_610))
        .unwrap_err();
    assert!(matches!(
        cross_kind,
        DisbursementError::EventIdentityConflict(_)
    ));
}
#[test]
fn settled_terminal_and_exact_replay_are_idempotent() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let mut engine = DisbursementEngine::new();
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-1",
        "create-1",
        1_500,
    );
    submit_ready(&mut engine, "d-1", "submit-1", "request-1", 1_510);
    let terminal = DisbursementTerminalEvent::settled(
        event_id("terminal-1"),
        disbursement_id("d-1"),
        provider_event("provider-event-1"),
        settlement("settlement-1"),
        1_520,
    );
    assert_eq!(
        engine.record_terminal(terminal.clone()).unwrap(),
        TerminalOutcome::Settled
    );
    assert_eq!(
        engine.record_terminal(terminal).unwrap(),
        TerminalOutcome::Replayed {
            status: DisbursementStatus::Settled
        }
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
fn failed_terminal_and_exact_replay_are_idempotent() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-1",
        "create-1",
        1_500,
    );
    submit_ready(&mut engine, "d-1", "submit-1", "request-1", 1_510);
    let terminal = DisbursementTerminalEvent::failed(
        event_id("terminal-fail-1"),
        disbursement_id("d-1"),
        provider_event("provider-event-fail-1"),
        failure("provider_declined"),
        1_520,
    );
    assert_eq!(
        engine.record_terminal(terminal.clone()).unwrap(),
        TerminalOutcome::Failed
    );
    assert_eq!(
        engine.record_terminal(terminal).unwrap(),
        TerminalOutcome::Replayed {
            status: DisbursementStatus::Failed
        }
    );
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    assert_eq!(record.status(), DisbursementStatus::Failed);
    assert_eq!(record.failure_code().unwrap().as_str(), "provider_declined");
}
#[test]
fn terminal_requires_submission_and_monotonic_time() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-1",
        "create-1",
        1_500,
    );
    let before_submit = DisbursementTerminalEvent::settled(
        event_id("terminal-before-submit"),
        disbursement_id("d-1"),
        provider_event("provider-event-before-submit"),
        settlement("settlement-before-submit"),
        1_510,
    );
    assert!(matches!(
        engine.record_terminal(before_submit),
        Err(DisbursementError::InvalidTransition { .. })
    ));

    submit_ready(&mut engine, "d-1", "submit-1", "request-1", 1_520);
    let early = DisbursementTerminalEvent::settled(
        event_id("terminal-early"),
        disbursement_id("d-1"),
        provider_event("provider-event-early"),
        settlement("settlement-early"),
        1_519,
    );
    assert!(matches!(
        engine.record_terminal(early),
        Err(DisbursementError::TerminalBeforeSubmission { .. })
    ));
    assert_eq!(
        engine
            .disbursement(&disbursement_id("d-1"))
            .unwrap()
            .status(),
        DisbursementStatus::Submitted
    );
}

#[test]
fn terminal_state_is_immutable_except_exact_replay() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-1",
        "create-1",
        1_500,
    );
    submit_ready(&mut engine, "d-1", "submit-1", "request-1", 1_510);
    engine
        .record_terminal(DisbursementTerminalEvent::settled(
            event_id("terminal-1"),
            disbursement_id("d-1"),
            provider_event("provider-event-1"),
            settlement("settlement-1"),
            1_520,
        ))
        .unwrap();
    let error = engine
        .record_terminal(DisbursementTerminalEvent::failed(
            event_id("terminal-2"),
            disbursement_id("d-1"),
            provider_event("provider-event-2"),
            failure("late_failure"),
            1_530,
        ))
        .unwrap_err();
    assert!(matches!(error, DisbursementError::InvalidTransition { .. }));
    assert_eq!(
        engine
            .disbursement(&disbursement_id("d-1"))
            .unwrap()
            .status(),
        DisbursementStatus::Settled
    );
}

#[test]
fn provider_terminal_references_are_unique_across_histories() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-1",
        "create-1",
        1_500,
    );
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend2,
        "d-2",
        "create-2",
        1_600,
    );
    submit_ready(&mut engine, "d-1", "submit-1", "request-1", 1_510);
    submit_ready(&mut engine, "d-2", "submit-2", "request-2", 1_610);
    engine
        .record_terminal(DisbursementTerminalEvent::settled(
            event_id("terminal-1"),
            disbursement_id("d-1"),
            provider_event("shared-provider-event"),
            settlement("shared-settlement"),
            1_520,
        ))
        .unwrap();
    let event_conflict = engine
        .record_terminal(DisbursementTerminalEvent::failed(
            event_id("terminal-2"),
            disbursement_id("d-2"),
            provider_event("shared-provider-event"),
            failure("declined"),
            1_620,
        ))
        .unwrap_err();
    assert!(matches!(
        event_conflict,
        DisbursementError::ProviderEventReferenceConflict { .. }
    ));

    let settlement_conflict = engine
        .record_terminal(DisbursementTerminalEvent::settled(
            event_id("terminal-3"),
            disbursement_id("d-2"),
            provider_event("provider-event-unique"),
            settlement("shared-settlement"),
            1_620,
        ))
        .unwrap_err();
    assert!(matches!(
        settlement_conflict,
        DisbursementError::ProviderSettlementReferenceConflict { .. }
    ));
    assert_eq!(
        engine
            .disbursement(&disbursement_id("d-2"))
            .unwrap()
            .status(),
        DisbursementStatus::Submitted
    );
}

#[test]
fn terminal_event_identity_conflicts_across_kinds_and_payloads() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-1",
        "create-1",
        1_500,
    );
    submit_ready(&mut engine, "d-1", "submit-1", "request-1", 1_510);

    let cross_kind = engine
        .record_terminal(DisbursementTerminalEvent::failed(
            event_id("submit-1"),
            disbursement_id("d-1"),
            provider_event("provider-event-cross-kind"),
            failure("declined"),
            1_520,
        ))
        .unwrap_err();
    assert!(matches!(
        cross_kind,
        DisbursementError::EventIdentityConflict(_)
    ));

    let original = DisbursementTerminalEvent::failed(
        event_id("terminal-1"),
        disbursement_id("d-1"),
        provider_event("provider-event-1"),
        failure("declined"),
        1_520,
    );
    engine.record_terminal(original).unwrap();
    let conflict = engine
        .record_terminal(DisbursementTerminalEvent::failed(
            event_id("terminal-1"),
            disbursement_id("d-1"),
            provider_event("provider-event-changed"),
            failure("different_reason"),
            1_521,
        ))
        .unwrap_err();
    assert!(matches!(
        conflict,
        DisbursementError::TerminalEventConflict(_)
    ));
}

#[test]
fn opaque_identifier_types_reject_empty_values() {
    assert!(DisbursementId::new(" ").is_err());
    assert!(DisbursementEventId::new("").is_err());
    assert!(BeneficiaryReference::new(" ").is_err());
    assert!(DestinationReference::new("").is_err());
    assert!(ProviderRequestReference::new(" ").is_err());
    assert!(ProviderEventReference::new("").is_err());
    assert!(ProviderSettlementReference::new(" ").is_err());
    assert!(FailureCode::new("").is_err());
}
#[test]
fn rejected_lifecycle_events_do_not_reserve_identities() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    let early_create = creation("create-retry", "d-retry", 1_499);
    assert!(
        engine
            .create(
                &fixture.registry,
                &fixture.governance,
                &fixture.ledger,
                &fixture.spend1,
                early_create
            )
            .is_err()
    );
    create_ready(
        &mut engine,
        &fixture,
        &fixture.spend1,
        "d-retry",
        "create-retry",
        1_500,
    );

    assert!(
        engine
            .submit(submission(
                "submit-retry",
                "d-retry",
                "request-retry",
                1_499
            ))
            .is_err()
    );
    submit_ready(
        &mut engine,
        "d-retry",
        "submit-retry",
        "request-retry",
        1_500,
    );

    let early_terminal = DisbursementTerminalEvent::settled(
        event_id("terminal-retry"),
        disbursement_id("d-retry"),
        provider_event("provider-event-retry"),
        settlement("settlement-retry"),
        1_499,
    );
    assert!(engine.record_terminal(early_terminal).is_err());
    assert_eq!(
        engine
            .record_terminal(DisbursementTerminalEvent::settled(
                event_id("terminal-retry"),
                disbursement_id("d-retry"),
                provider_event("provider-event-retry"),
                settlement("settlement-retry"),
                1_500,
            ))
            .unwrap(),
        TerminalOutcome::Settled
    );
}
#[test]
fn conflicting_p11_snapshot_is_rejected_before_disbursement_creation() {
    let fixture = fixture();
    let conflicting = ApprovedFundSpendEvent::new(
        FundSpendEventId::new("spend-event-1").unwrap(),
        FundSpendId::new("spend-1").unwrap(),
        proposal_id("proposal-1"),
        org("org-1"),
        community("community-1"),
        fund("fund-1"),
        usd(),
        101,
        "vendor:invoice-1",
        account("expense-account"),
        1_500,
        1_500,
    )
    .unwrap();
    let mut engine = DisbursementEngine::new();
    let error = engine
        .create(
            &fixture.registry,
            &fixture.governance,
            &fixture.ledger,
            &conflicting,
            creation("create-conflict", "d-conflict", 1_500),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        DisbursementError::SpendVerification(FundSpendError::AuthorizationSnapshotMismatch(
            "amount"
        ))
    ));
}
