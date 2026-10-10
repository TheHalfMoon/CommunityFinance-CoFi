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
use cofi_provider_contract::{ProviderContractError, ProviderObservation};
use cofi_reconciliation::{
    DiscrepancyKind, ReconciliationCase, ReconciliationCaseId, ReconciliationEngine,
    ReconciliationError, ReconciliationOutcome,
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

fn case(id: &str, disbursement: &str, request_ref: &str, reconciled_at: i64) -> ReconciliationCase {
    ReconciliationCase::new(
        ReconciliationCaseId::new(id).unwrap(),
        disbursement_id(disbursement),
        request(request_ref),
        reconciled_at,
    )
}

fn accepted(id: &str, request_ref: &str, at: i64) -> ProviderObservation {
    ProviderObservation::accepted(
        disbursement_id(id),
        request(request_ref),
        provider_event(&format!("accepted-{id}")),
        at,
    )
}

fn settled_observation(id: &str, request_ref: &str, suffix: &str, at: i64) -> ProviderObservation {
    ProviderObservation::settled(
        disbursement_id(id),
        request(request_ref),
        event_id(&format!("terminal-{suffix}")),
        provider_event(&format!("provider-event-{suffix}")),
        settlement(&format!("settlement-{suffix}")),
        at,
    )
}

fn failed_observation(id: &str, request_ref: &str, suffix: &str, at: i64) -> ProviderObservation {
    ProviderObservation::failed(
        disbursement_id(id),
        request(request_ref),
        event_id(&format!("terminal-{suffix}")),
        provider_event(&format!("provider-event-{suffix}")),
        failure(&format!("failure-{suffix}")),
        at,
    )
}

fn submitted_engine(
    fixture: &Fixture,
    spend: &ApprovedFundSpendEvent,
    id: &str,
    at: i64,
) -> DisbursementEngine {
    let mut engine = DisbursementEngine::new();
    create_ready(&mut engine, fixture, spend, id, at);
    submit_ready(&mut engine, id, &format!("request-{id}"), at + 10);
    engine
}

#[test]
fn caller_supplied_case_and_observation_can_only_recompute_untrusted_outcome() {
    use cofi_storage::provider_observation::{
        encode_provider_observation, recompute_untrusted_reconciliation,
    };
    use cofi_storage::reconciliation_case::encode_reconciliation_case;

    let fixture = fixture();
    let ledger_before = ledger_snapshot(&fixture);
    let engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    let original_disbursement = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let original_case = case("case-untrusted", "d-1", "request-d-1", 1_530);
    let case_bytes = encode_reconciliation_case(&original_case).unwrap();
    for observation in [
        accepted("d-1", "request-d-1", 1_520),
        settled_observation("d-1", "request-d-1", "1", 1_520),
        failed_observation("d-1", "request-d-1", "1", 1_520),
    ] {
        let bytes = encode_provider_observation(&observation).unwrap();
        let replay =
            recompute_untrusted_reconciliation(&case_bytes, &bytes, original_disbursement).unwrap();
        let expected = ReconciliationEngine::new()
            .reconcile(&original_case, original_disbursement, &observation)
            .unwrap();
        assert_eq!(replay.outcome(), &expected);
        // A replay of entirely caller-supplied records must NEVER become
        // authenticated provider evidence or admitted terminal settlement.
        assert!(replay.require_independent_source_authentication().is_err());
    }
    let foreign = accepted("d-other", "request-d-1", 1_520);
    assert!(
        recompute_untrusted_reconciliation(
            &case_bytes,
            &encode_provider_observation(&foreign).unwrap(),
            original_disbursement,
        )
        .is_err()
    );
    let foreign_request = accepted("d-1", "request-other", 1_520);
    assert!(
        recompute_untrusted_reconciliation(
            &case_bytes,
            &encode_provider_observation(&foreign_request).unwrap(),
            original_disbursement,
        )
        .is_err()
    );
    let late = accepted("d-1", "request-d-1", 1_531);
    assert!(
        recompute_untrusted_reconciliation(
            &case_bytes,
            &encode_provider_observation(&late).unwrap(),
            original_disbursement,
        )
        .is_err()
    );
    // Even when the case and observation agree with each other, the original
    // engine must independently reject mismatched canonical disbursement IDs.
    let other_engine = submitted_engine(&fixture, &fixture.spend2, "d-2", 1_700);
    let other_disbursement = other_engine.disbursement(&disbursement_id("d-2")).unwrap();
    assert!(
        recompute_untrusted_reconciliation(
            &case_bytes,
            &encode_provider_observation(&accepted("d-1", "request-d-1", 1_520)).unwrap(),
            other_disbursement,
        )
        .is_err()
    );
    assert_eq!(
        original_disbursement.status(),
        DisbursementStatus::Submitted
    );
    assert_eq!(ledger_snapshot(&fixture), ledger_before);
}

#[test]
fn reconciliation_case_id_rejects_empty_values() {
    assert!(ReconciliationCaseId::new("").is_err());
    assert!(ReconciliationCaseId::new("   ").is_err());
}

#[test]
fn submitted_plus_accepted_is_pending_agreement_and_side_effect_free() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = accepted("d-1", "request-d-1", 1_520);
    let reconciliation = ReconciliationEngine::new();
    let first = reconciliation
        .reconcile(
            &case("case-1", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap();
    let second = reconciliation
        .reconcile(
            &case("case-1", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first,
        ReconciliationOutcome::PendingAgreement {
            case_id: ReconciliationCaseId::new("case-1").unwrap(),
        }
    );
    assert_eq!(record.status(), DisbursementStatus::Submitted);
    assert_eq!(ledger_snapshot(&fixture), before);
}

#[test]
fn submitted_plus_settled_is_provider_ahead_with_exact_terminal_command() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = settled_observation("d-1", "request-d-1", "1", 1_520);
    let expected = cofi_provider_contract::ProviderContract::new()
        .to_terminal_event(record, &observation)
        .unwrap();
    let outcome = ReconciliationEngine::new()
        .reconcile(
            &case("case-settled-ahead", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap();
    assert_eq!(
        outcome,
        ReconciliationOutcome::ProviderAhead {
            case_id: ReconciliationCaseId::new("case-settled-ahead").unwrap(),
            proposed_terminal_event: expected,
        }
    );
    assert_eq!(record.status(), DisbursementStatus::Submitted);
    assert_eq!(ledger_snapshot(&fixture), before);
}

#[test]
fn submitted_plus_failed_is_provider_ahead_with_exact_terminal_command() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = failed_observation("d-1", "request-d-1", "1", 1_520);
    let expected = cofi_provider_contract::ProviderContract::new()
        .to_terminal_event(record, &observation)
        .unwrap();
    let outcome = ReconciliationEngine::new()
        .reconcile(
            &case("case-failed-ahead", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap();
    assert_eq!(
        outcome,
        ReconciliationOutcome::ProviderAhead {
            case_id: ReconciliationCaseId::new("case-failed-ahead").unwrap(),
            proposed_terminal_event: expected,
        }
    );
    assert_eq!(record.status(), DisbursementStatus::Submitted);
    assert_eq!(ledger_snapshot(&fixture), before);
}

#[test]
fn exact_settled_observation_is_terminal_agreement() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let mut engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    let observation = settled_observation("d-1", "request-d-1", "1", 1_520);
    let terminal = {
        let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
        cofi_provider_contract::ProviderContract::new()
            .to_terminal_event(record, &observation)
            .unwrap()
    };
    assert_eq!(
        engine.record_terminal(terminal).unwrap(),
        TerminalOutcome::Settled
    );
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let outcome = ReconciliationEngine::new()
        .reconcile(
            &case("case-settled", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap();
    assert_eq!(
        outcome,
        ReconciliationOutcome::TerminalAgreement {
            case_id: ReconciliationCaseId::new("case-settled").unwrap(),
            status: DisbursementStatus::Settled,
        }
    );
    assert_eq!(ledger_snapshot(&fixture), before);
}

#[test]
fn exact_failed_observation_is_terminal_agreement() {
    let fixture = fixture();
    let before = ledger_snapshot(&fixture);
    let mut engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    let observation = failed_observation("d-1", "request-d-1", "1", 1_520);
    let terminal = {
        let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
        cofi_provider_contract::ProviderContract::new()
            .to_terminal_event(record, &observation)
            .unwrap()
    };
    assert_eq!(
        engine.record_terminal(terminal).unwrap(),
        TerminalOutcome::Failed
    );
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let outcome = ReconciliationEngine::new()
        .reconcile(
            &case("case-failed", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap();
    assert_eq!(
        outcome,
        ReconciliationOutcome::TerminalAgreement {
            case_id: ReconciliationCaseId::new("case-failed").unwrap(),
            status: DisbursementStatus::Failed,
        }
    );
    assert_eq!(ledger_snapshot(&fixture), before);
}

#[test]
fn terminal_plus_accepted_is_discrepancy() {
    let fixture = fixture();
    let mut engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    engine
        .record_terminal(DisbursementTerminalEvent::settled(
            event_id("terminal-canonical"),
            disbursement_id("d-1"),
            provider_event("provider-event-canonical"),
            settlement("settlement-canonical"),
            1_520,
        ))
        .unwrap();
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = accepted("d-1", "request-d-1", 1_520);
    let outcome = ReconciliationEngine::new()
        .reconcile(
            &case("case-accepted-terminal", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap();
    assert_eq!(
        outcome,
        ReconciliationOutcome::Discrepancy {
            case_id: ReconciliationCaseId::new("case-accepted-terminal").unwrap(),
            kind: DiscrepancyKind::AcceptedAfterTerminal,
        }
    );
}

#[test]
fn terminal_kind_mismatch_is_discrepancy() {
    let fixture = fixture();
    let mut engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    engine
        .record_terminal(DisbursementTerminalEvent::settled(
            event_id("terminal-canonical"),
            disbursement_id("d-1"),
            provider_event("provider-event-canonical"),
            settlement("settlement-canonical"),
            1_520,
        ))
        .unwrap();
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = failed_observation("d-1", "request-d-1", "provider-failed", 1_520);
    let outcome = ReconciliationEngine::new()
        .reconcile(
            &case("case-kind-mismatch", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap();
    assert_eq!(
        outcome,
        ReconciliationOutcome::Discrepancy {
            case_id: ReconciliationCaseId::new("case-kind-mismatch").unwrap(),
            kind: DiscrepancyKind::TerminalStatusMismatch,
        }
    );
}

#[test]
fn settled_provider_event_mismatch_is_discrepancy() {
    let fixture = fixture();
    let mut engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    engine
        .record_terminal(DisbursementTerminalEvent::settled(
            event_id("terminal-canonical"),
            disbursement_id("d-1"),
            provider_event("provider-event-canonical"),
            settlement("settlement-canonical"),
            1_520,
        ))
        .unwrap();
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = ProviderObservation::settled(
        disbursement_id("d-1"),
        request("request-d-1"),
        event_id("terminal-observed"),
        provider_event("provider-event-different"),
        settlement("settlement-canonical"),
        1_520,
    );
    let outcome = ReconciliationEngine::new()
        .reconcile(
            &case("case-event-mismatch", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap();
    assert_eq!(
        outcome,
        ReconciliationOutcome::Discrepancy {
            case_id: ReconciliationCaseId::new("case-event-mismatch").unwrap(),
            kind: DiscrepancyKind::ProviderEventReferenceMismatch,
        }
    );
}

#[test]
fn settled_reference_mismatch_is_discrepancy() {
    let fixture = fixture();
    let mut engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    engine
        .record_terminal(DisbursementTerminalEvent::settled(
            event_id("terminal-canonical"),
            disbursement_id("d-1"),
            provider_event("provider-event-canonical"),
            settlement("settlement-canonical"),
            1_520,
        ))
        .unwrap();
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = ProviderObservation::settled(
        disbursement_id("d-1"),
        request("request-d-1"),
        event_id("terminal-observed"),
        provider_event("provider-event-canonical"),
        settlement("settlement-different"),
        1_520,
    );
    let outcome = ReconciliationEngine::new()
        .reconcile(
            &case("case-settlement-mismatch", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap();
    assert_eq!(
        outcome,
        ReconciliationOutcome::Discrepancy {
            case_id: ReconciliationCaseId::new("case-settlement-mismatch").unwrap(),
            kind: DiscrepancyKind::SettlementReferenceMismatch,
        }
    );
}

#[test]
fn failed_code_mismatch_is_discrepancy() {
    let fixture = fixture();
    let mut engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    engine
        .record_terminal(DisbursementTerminalEvent::failed(
            event_id("terminal-canonical"),
            disbursement_id("d-1"),
            provider_event("provider-event-canonical"),
            failure("failure-canonical"),
            1_520,
        ))
        .unwrap();
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = ProviderObservation::failed(
        disbursement_id("d-1"),
        request("request-d-1"),
        event_id("terminal-observed"),
        provider_event("provider-event-canonical"),
        failure("failure-different"),
        1_520,
    );
    let outcome = ReconciliationEngine::new()
        .reconcile(
            &case("case-failure-mismatch", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap();
    assert_eq!(
        outcome,
        ReconciliationOutcome::Discrepancy {
            case_id: ReconciliationCaseId::new("case-failure-mismatch").unwrap(),
            kind: DiscrepancyKind::FailureCodeMismatch,
        }
    );
}

#[test]
fn terminal_timestamp_mismatch_is_discrepancy() {
    let fixture = fixture();
    let mut engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    engine
        .record_terminal(DisbursementTerminalEvent::settled(
            event_id("terminal-canonical"),
            disbursement_id("d-1"),
            provider_event("provider-event-canonical"),
            settlement("settlement-canonical"),
            1_520,
        ))
        .unwrap();
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = ProviderObservation::settled(
        disbursement_id("d-1"),
        request("request-d-1"),
        event_id("terminal-observed"),
        provider_event("provider-event-canonical"),
        settlement("settlement-canonical"),
        1_521,
    );
    let outcome = ReconciliationEngine::new()
        .reconcile(
            &case("case-time-mismatch", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap();
    assert_eq!(
        outcome,
        ReconciliationOutcome::Discrepancy {
            case_id: ReconciliationCaseId::new("case-time-mismatch").unwrap(),
            kind: DiscrepancyKind::TerminalTimestampMismatch,
        }
    );
}

#[test]
fn ready_disbursement_fails_closed_before_provider_reconciliation() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(&mut engine, &fixture, &fixture.spend1, "d-1", 1_500);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = accepted("d-1", "not-submitted", 1_520);
    let error = ReconciliationEngine::new()
        .reconcile(
            &case("case-ready", "d-1", "not-submitted", 1_530),
            record,
            &observation,
        )
        .unwrap_err();
    assert_eq!(error, ReconciliationError::ReadyDisbursement);
}

#[test]
fn reconciliation_case_binding_mismatch_fails_closed() {
    let fixture = fixture();
    let engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = accepted("d-1", "request-d-1", 1_520);
    let wrong_id = ReconciliationEngine::new()
        .reconcile(
            &case("case-wrong-id", "d-2", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap_err();
    assert_eq!(
        wrong_id,
        ReconciliationError::CaseBindingMismatch("disbursement_id")
    );
}

#[test]
fn reconciliation_case_request_binding_mismatch_fails_closed() {
    let fixture = fixture();
    let engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = accepted("d-1", "request-d-1", 1_520);
    let error = ReconciliationEngine::new()
        .reconcile(
            &case("case-wrong-request", "d-1", "other-request", 1_530),
            record,
            &observation,
        )
        .unwrap_err();
    assert_eq!(
        error,
        ReconciliationError::CaseBindingMismatch("provider_request_reference")
    );
}

#[test]
fn wrong_provider_observation_binding_fails_closed() {
    let fixture = fixture();
    let mut engine = DisbursementEngine::new();
    create_ready(&mut engine, &fixture, &fixture.spend1, "d-1", 1_500);
    submit_ready(&mut engine, "d-1", "request-d-1", 1_510);
    create_ready(&mut engine, &fixture, &fixture.spend2, "d-2", 1_600);
    submit_ready(&mut engine, "d-2", "request-d-2", 1_610);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = accepted("d-2", "request-d-2", 1_620);
    let error = ReconciliationEngine::new()
        .reconcile(
            &case("case-cross", "d-1", "request-d-1", 1_630),
            record,
            &observation,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ReconciliationError::ProviderContract(ProviderContractError::ObservationBindingMismatch(
            "disbursement_id"
        ))
    ));
}

#[test]
fn provider_observation_before_submission_fails_closed() {
    let fixture = fixture();
    let engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = accepted("d-1", "request-d-1", 1_509);
    let error = ReconciliationEngine::new()
        .reconcile(
            &case("case-early-provider", "d-1", "request-d-1", 1_530),
            record,
            &observation,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ReconciliationError::ProviderContract(ProviderContractError::ObservationBeforeSubmission {
            submitted_at_unix_ms: 1_510,
            occurred_at_unix_ms: 1_509,
        })
    ));
}

#[test]
fn reconciliation_timestamp_cannot_precede_provider_observation() {
    let fixture = fixture();
    let engine = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    let record = engine.disbursement(&disbursement_id("d-1")).unwrap();
    let observation = accepted("d-1", "request-d-1", 1_520);
    let error = ReconciliationEngine::new()
        .reconcile(
            &case("case-too-early", "d-1", "request-d-1", 1_519),
            record,
            &observation,
        )
        .unwrap_err();
    assert_eq!(
        error,
        ReconciliationError::ReconciliationBeforeObservation {
            reconciled_at_unix_ms: 1_519,
            observation_at_unix_ms: 1_520,
        }
    );
}
