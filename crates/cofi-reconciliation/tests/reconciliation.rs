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
        use cofi_storage::provider_observation::UntrustedReconciliationDiagnostic as D;
        let expected_diagnostic = match expected {
            ReconciliationOutcome::PendingAgreement { .. } => D::PendingAgreement,
            ReconciliationOutcome::ProviderAhead { .. } => D::ProviderAhead,
            ReconciliationOutcome::TerminalAgreement { .. } => D::TerminalAgreement,
            ReconciliationOutcome::Discrepancy { .. } => D::Discrepancy,
        };
        assert_eq!(replay.diagnostic(), expected_diagnostic);
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
fn untrusted_first_time_batch_requires_distinct_cases_and_provider_events() {
    use cofi_storage::provider_observation::{
        UntrustedReconciliationRecord, encode_provider_observation,
        recompute_untrusted_first_time_reconciliation_batch,
    };
    use cofi_storage::reconciliation_case::encode_reconciliation_case;

    let fixture = fixture();
    let ledger_before = ledger_snapshot(&fixture);
    let engine1 = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    let engine2 = submitted_engine(&fixture, &fixture.spend2, "d-2", 1_700);
    let d1 = engine1.disbursement(&disbursement_id("d-1")).unwrap();
    let d2 = engine2.disbursement(&disbursement_id("d-2")).unwrap();
    let case1 = case("case-1", "d-1", "request-d-1", 1_530);
    let case2 = case("case-2", "d-2", "request-d-2", 1_730);
    let case1_bytes = encode_reconciliation_case(&case1).unwrap();
    let case2_bytes = encode_reconciliation_case(&case2).unwrap();
    let obs1 = accepted("d-1", "request-d-1", 1_520);
    let obs2 = accepted("d-2", "request-d-2", 1_720);
    let obs1_bytes = encode_provider_observation(&obs1).unwrap();
    let obs2_bytes = encode_provider_observation(&obs2).unwrap();
    let r1 = UntrustedReconciliationRecord {
        case_bytes: &case1_bytes,
        observation_bytes: &obs1_bytes,
        disbursement: d1,
    };
    let r2 = UntrustedReconciliationRecord {
        case_bytes: &case2_bytes,
        observation_bytes: &obs2_bytes,
        disbursement: d2,
    };
    let replay = recompute_untrusted_first_time_reconciliation_batch(&[r1, r2]).unwrap();
    assert_eq!(replay.outcomes().len(), 2);
    // Original-engine domain parity remains checked through the diagnostic
    // category only. Untrusted callers must not extract terminal commands.
    use cofi_storage::provider_observation::UntrustedReconciliationDiagnostic as D;
    assert_eq!(replay.outcomes()[0].diagnostic(), D::PendingAgreement);
    assert_eq!(replay.outcomes()[1].diagnostic(), D::PendingAgreement);
    assert!(matches!(
        ReconciliationEngine::new().reconcile(&case1, d1, &obs1),
        Ok(ReconciliationOutcome::PendingAgreement { .. })
    ));
    assert!(matches!(
        ReconciliationEngine::new().reconcile(&case2, d2, &obs2),
        Ok(ReconciliationOutcome::PendingAgreement { .. })
    ));
    assert!(replay.require_independent_source_authentication().is_err());
    assert_eq!(ledger_snapshot(&fixture), ledger_before);
    assert_eq!(d1.status(), DisbursementStatus::Submitted);
    assert_eq!(d2.status(), DisbursementStatus::Submitted);
    // A second case can be otherwise domain-valid but not a first-time case.
    let reused_case = case("case-1", "d-2", "request-d-2", 1_730);
    let reused_case_bytes = encode_reconciliation_case(&reused_case).unwrap();
    let duplicate = UntrustedReconciliationRecord {
        case_bytes: &reused_case_bytes,
        ..r2
    };
    assert!(
        recompute_untrusted_first_time_reconciliation_batch(&[r1, duplicate]).is_err(),
        "one original case identity cannot be accepted twice"
    );
    // A new disbursement/case with a reused original provider event ID also
    // passes a standalone call, but cannot be admitted as a distinct first event.
    let reused_provider = ProviderObservation::accepted(
        disbursement_id("d-2"),
        request("request-d-2"),
        provider_event("accepted-d-1"),
        1_720,
    );
    assert!(
        ReconciliationEngine::new()
            .reconcile(&case2, d2, &reused_provider)
            .is_ok()
    );
    let reused_provider_bytes = encode_provider_observation(&reused_provider).unwrap();
    let duplicate = UntrustedReconciliationRecord {
        observation_bytes: &reused_provider_bytes,
        ..r2
    };
    assert!(
        recompute_untrusted_first_time_reconciliation_batch(&[r1, duplicate]).is_err(),
        "provider event replay across disbursements must be detected"
    );
    // Separate case and provider-event IDs may still target the SAME
    // submitted disbursement. The original one-case engine accepts these
    // independently, but they are not two distinct first-time records.
    let same_disbursement_case = case("case-same-d-1", "d-1", "request-d-1", 1_531);
    let same_case_bytes = encode_reconciliation_case(&same_disbursement_case).unwrap();
    let independent_provider_event = ProviderObservation::accepted(
        disbursement_id("d-1"),
        request("request-d-1"),
        provider_event("accepted-d-1-second"),
        1_521,
    );
    assert!(
        ReconciliationEngine::new()
            .reconcile(&same_disbursement_case, d1, &independent_provider_event)
            .is_ok()
    );
    let independent_observation_bytes =
        encode_provider_observation(&independent_provider_event).unwrap();
    let duplicated_disbursement = UntrustedReconciliationRecord {
        case_bytes: &same_case_bytes,
        observation_bytes: &independent_observation_bytes,
        disbursement: d1,
    };
    assert!(
        recompute_untrusted_first_time_reconciliation_batch(&[r1, duplicated_disbursement,])
            .is_err(),
        "a declared first-time batch must not admit two independently valid observations of one disbursement"
    );
    let too_many = vec![r1; 4097];
    assert!(
        recompute_untrusted_first_time_reconciliation_batch(&too_many)
            .unwrap_err()
            .to_string()
            .contains("record count"),
        "overbudget record count must fail before any domain work"
    );
    let oversized = vec![b' '; 1024 * 1024];
    let oversized_record = UntrustedReconciliationRecord {
        case_bytes: &oversized,
        observation_bytes: &[],
        disbursement: d1,
    };
    let too_many_bytes = vec![oversized_record; 17];
    assert!(
        recompute_untrusted_first_time_reconciliation_batch(&too_many_bytes)
            .unwrap_err()
            .to_string()
            .contains("16 MiB"),
        "overbudget bytes must fail before individual JSON decoding"
    );
    assert!(recompute_untrusted_first_time_reconciliation_batch(&[]).is_err());
}

#[test]
fn untrusted_first_time_batch_checks_terminal_and_settlement_consumption() {
    use cofi_storage::provider_observation::{
        UntrustedReconciliationRecord, encode_provider_observation,
        recompute_untrusted_first_time_reconciliation_batch,
    };
    use cofi_storage::reconciliation_case::encode_reconciliation_case;
    let fixture = fixture();
    let engine1 = submitted_engine(&fixture, &fixture.spend1, "d-1", 1_500);
    let engine2 = submitted_engine(&fixture, &fixture.spend2, "d-2", 1_700);
    let d1 = engine1.disbursement(&disbursement_id("d-1")).unwrap();
    let d2 = engine2.disbursement(&disbursement_id("d-2")).unwrap();
    let c1 =
        encode_reconciliation_case(&case("case-terminal-1", "d-1", "request-d-1", 1_530)).unwrap();
    let c2 =
        encode_reconciliation_case(&case("case-terminal-2", "d-2", "request-d-2", 1_730)).unwrap();
    let o1 = encode_provider_observation(&settled_observation(
        "d-1",
        "request-d-1",
        "unique-1",
        1_520,
    ))
    .unwrap();
    let o2 = encode_provider_observation(&settled_observation(
        "d-2",
        "request-d-2",
        "unique-2",
        1_720,
    ))
    .unwrap();
    let r1 = UntrustedReconciliationRecord {
        case_bytes: &c1,
        observation_bytes: &o1,
        disbursement: d1,
    };
    let r2 = UntrustedReconciliationRecord {
        case_bytes: &c2,
        observation_bytes: &o2,
        disbursement: d2,
    };
    assert_eq!(
        recompute_untrusted_first_time_reconciliation_batch(&[r1, r2])
            .unwrap()
            .outcomes()
            .len(),
        2
    );
    use cofi_storage::provider_observation::UntrustedReconciliationDiagnostic as D;
    let settled_batch = recompute_untrusted_first_time_reconciliation_batch(&[r1, r2]).unwrap();
    assert_eq!(settled_batch.outcomes()[0].diagnostic(), D::ProviderAhead);
    assert_eq!(settled_batch.outcomes()[1].diagnostic(), D::ProviderAhead);
    assert!(
        settled_batch
            .require_independent_source_authentication()
            .is_err()
    );

    // Different terminal outcomes remain non-command diagnostics.
    let failed =
        encode_provider_observation(&failed_observation("d-2", "request-d-2", "unique-2", 1_720))
            .unwrap();
    let mixed = recompute_untrusted_first_time_reconciliation_batch(&[
        r1,
        UntrustedReconciliationRecord {
            observation_bytes: &failed,
            ..r2
        },
    ])
    .unwrap();
    assert_eq!(mixed.outcomes()[0].diagnostic(), D::ProviderAhead);
    assert_eq!(mixed.outcomes()[1].diagnostic(), D::ProviderAhead);
    assert!(mixed.require_independent_source_authentication().is_err());

    let duplicate_terminal = ProviderObservation::settled(
        disbursement_id("d-2"),
        request("request-d-2"),
        event_id("terminal-unique-1"),
        provider_event("provider-event-unique-2"),
        settlement("settlement-unique-2"),
        1_720,
    );
    let bytes = encode_provider_observation(&duplicate_terminal).unwrap();
    assert!(
        recompute_untrusted_first_time_reconciliation_batch(&[
            r1,
            UntrustedReconciliationRecord {
                observation_bytes: &bytes,
                ..r2
            }
        ])
        .is_err(),
        "same terminal event cannot appear twice in a first-time batch"
    );
    let duplicate_settlement = ProviderObservation::settled(
        disbursement_id("d-2"),
        request("request-d-2"),
        event_id("terminal-unique-2"),
        provider_event("provider-event-unique-2"),
        settlement("settlement-unique-1"),
        1_720,
    );
    let bytes = encode_provider_observation(&duplicate_settlement).unwrap();
    assert!(
        recompute_untrusted_first_time_reconciliation_batch(&[
            r1,
            UntrustedReconciliationRecord {
                observation_bytes: &bytes,
                ..r2
            }
        ])
        .is_err(),
        "same settlement receipt cannot appear twice in a first-time batch"
    );
    assert!(
        recompute_untrusted_first_time_reconciliation_batch(&[r1, r1]).is_err(),
        "duplicate full record must not be replayed"
    );
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
