#![allow(clippy::unwrap_used)]

use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundId, Membership, MembershipId,
    MembershipRole, MembershipStatus, Organization, OrganizationId, Party, PartyId, PartyKind,
};
use cofi_governance::{
    SpendingApproval, SpendingApprovalEventId, SpendingApprovalId, SpendingApprovalPolicy,
    SpendingApprovalPolicyId, SpendingProposal, SpendingProposalEventId, SpendingProposalId,
};
use cofi_ledger::{Account, AccountId, AccountKind, Currency, Ledger, LedgerScopeId};
use cofi_storage::governance_approval::{encode_governance_approval, replay_governance_approvals};
use cofi_storage::governance_policy::encode_governance_policy;
use cofi_storage::governance_proposal::encode_governance_proposal;
fn community() -> CommunityRegistry {
    let mut ledger = Ledger::new();
    ledger
        .register_account(Account::new(
            AccountId::new("fund-asset").unwrap(),
            LedgerScopeId::new("org-a").unwrap(),
            AccountKind::Asset,
            Currency::new("SAR").unwrap(),
        ))
        .unwrap();
    let mut registry = CommunityRegistry::new();
    registry
        .register_organization(Organization::new(OrganizationId::new("org-a").unwrap()))
        .unwrap();
    registry
        .register_community(Community::new(
            CommunityId::new("community-a").unwrap(),
            OrganizationId::new("org-a").unwrap(),
        ))
        .unwrap();
    registry
        .register_fund(
            Fund::new(
                FundId::new("fund-a").unwrap(),
                CommunityId::new("community-a").unwrap(),
                AccountId::new("fund-asset").unwrap(),
                Currency::new("SAR").unwrap(),
            ),
            &ledger,
        )
        .unwrap();
    for (name, role, status) in [
        ("owner", MembershipRole::Owner, MembershipStatus::Active),
        (
            "treasurer",
            MembershipRole::Treasurer,
            MembershipStatus::Active,
        ),
        (
            "suspended",
            MembershipRole::Treasurer,
            MembershipStatus::Suspended,
        ),
        ("member", MembershipRole::Member, MembershipStatus::Active),
    ] {
        registry
            .register_party(Party::new(
                PartyId::new(name).unwrap(),
                OrganizationId::new("org-a").unwrap(),
                PartyKind::Person,
            ))
            .unwrap();
        registry
            .register_membership(Membership::new(
                MembershipId::new(format!("membership-{name}")).unwrap(),
                PartyId::new(name).unwrap(),
                CommunityId::new("community-a").unwrap(),
                role,
                status,
            ))
            .unwrap();
    }
    registry
}
fn policy() -> SpendingApprovalPolicy {
    SpendingApprovalPolicy::new(
        SpendingApprovalPolicyId::new("policy-a").unwrap(),
        1,
        OrganizationId::new("org-a").unwrap(),
        CommunityId::new("community-a").unwrap(),
        FundId::new("fund-a").unwrap(),
        Currency::new("SAR").unwrap(),
        1000,
        2,
        vec![MembershipRole::Treasurer, MembershipRole::Owner],
    )
    .unwrap()
}
fn proposal() -> SpendingProposal {
    SpendingProposal::new(
        SpendingProposalEventId::new("proposal-event").unwrap(),
        SpendingProposalId::new("proposal-a").unwrap(),
        SpendingApprovalPolicyId::new("policy-a").unwrap(),
        1,
        PartyId::new("owner").unwrap(),
        OrganizationId::new("org-a").unwrap(),
        CommunityId::new("community-a").unwrap(),
        FundId::new("fund-a").unwrap(),
        Currency::new("SAR").unwrap(),
        750,
        "vendor:invoice-7",
        1000,
        2000,
    )
    .unwrap()
}
fn vote(event: &str, id: &str, approver: &str, time: i64) -> SpendingApproval {
    SpendingApproval::new(
        SpendingApprovalEventId::new(event).unwrap(),
        SpendingApprovalId::new(id).unwrap(),
        SpendingProposalId::new("proposal-a").unwrap(),
        PartyId::new(approver).unwrap(),
        time,
    )
}
fn parents() -> (Vec<u8>, Vec<u8>) {
    (
        encode_governance_policy(&policy()).unwrap(),
        encode_governance_proposal(&proposal()).unwrap(),
    )
}

use cofi_ledger::{EntryMetadata, JournalEntry, JournalEntryId, Posting, Side};
use cofi_spending::{
    ApprovedFundSpendEvent, FundSpendBridge, FundSpendEventId, FundSpendId, FundSpendOutcome,
};
use cofi_storage::governance_fund_spend::{
    decode_governance_fund_spend, encode_governance_fund_spend, verify_governance_fund_spend,
    verify_governance_fund_spend_history,
};

fn spend() -> ApprovedFundSpendEvent {
    ApprovedFundSpendEvent::new(
        FundSpendEventId::new("spend-event-1").unwrap(),
        FundSpendId::new("spend-1").unwrap(),
        SpendingProposalId::new("proposal-a").unwrap(),
        OrganizationId::new("org-a").unwrap(),
        CommunityId::new("community-a").unwrap(),
        FundId::new("fund-a").unwrap(),
        Currency::new("SAR").unwrap(),
        750,
        "vendor:invoice-7",
        AccountId::new("expense").unwrap(),
        1600,
        1700,
    )
    .unwrap()
}
fn original_ledger() -> (Ledger, Ledger) {
    let mut ledger = Ledger::new();
    for (name, kind) in [
        ("fund-asset", AccountKind::Asset),
        ("opening-equity", AccountKind::Equity),
        ("expense", AccountKind::Expense),
    ] {
        ledger
            .register_account(Account::new(
                AccountId::new(name).unwrap(),
                LedgerScopeId::new("org-a").unwrap(),
                kind,
                Currency::new("SAR").unwrap(),
            ))
            .unwrap();
    }
    ledger
        .commit(
            JournalEntry::new(
                JournalEntryId::new("seed-a").unwrap(),
                vec![
                    Posting::new(
                        AccountId::new("fund-asset").unwrap(),
                        Currency::new("SAR").unwrap(),
                        Side::Debit,
                        1000,
                    )
                    .unwrap(),
                    Posting::new(
                        AccountId::new("opening-equity").unwrap(),
                        Currency::new("SAR").unwrap(),
                        Side::Credit,
                        1000,
                    )
                    .unwrap(),
                ],
                800,
                900,
                EntryMetadata::new(Some("seed-a".into()), Some("funded-a".into())).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let before = ledger.clone();
    let registry = community();
    let (p, q) = parents();
    let owner = encode_governance_approval(&vote("v-1", "vote-1", "owner", 1500)).unwrap();
    let treasurer = encode_governance_approval(&vote("v-2", "vote-2", "treasurer", 1400)).unwrap();
    let gov = replay_governance_approvals(
        [p.as_slice()],
        [q.as_slice()],
        [owner.as_slice(), treasurer.as_slice()],
        &registry,
    )
    .unwrap();
    assert!(matches!(
        FundSpendBridge::new()
            .apply(&registry, &gov, &spend(), &mut ledger)
            .unwrap(),
        FundSpendOutcome::Committed { .. }
    ));
    (ledger, before)
}
fn sources() -> (Vec<u8>, Vec<u8>, Vec<Vec<u8>>) {
    let (p, q) = parents();
    let votes = [
        encode_governance_approval(&vote("v-1", "vote-1", "owner", 1500)).unwrap(),
        encode_governance_approval(&vote("v-2", "vote-2", "treasurer", 1400)).unwrap(),
    ];
    (p, q, votes.to_vec())
}
#[test]
fn verified_original_quorum_spend_and_ledger_journal_replay() {
    let (ledger, prior) = original_ledger();
    let (p, q, v) = sources();
    let receipt = encode_governance_fund_spend(&spend()).unwrap();
    assert_eq!(decode_governance_fund_spend(&receipt).unwrap(), spend());
    let reference = replay_governance_approvals(
        [p.as_slice()],
        [q.as_slice()],
        v.iter().map(Vec::as_slice),
        &community(),
    )
    .unwrap();
    assert_eq!(
        verify_governance_fund_spend(&receipt, &reference, &community(), &ledger)
            .unwrap()
            .journal_entry_id()
            .as_str(),
        "spending:fund-spend:spend-1"
    );
    assert!(verify_governance_fund_spend(&receipt, &reference, &community(), &prior).is_err());
    assert_eq!(
        verify_governance_fund_spend_history(
            [p.as_slice()],
            [q.as_slice()],
            v.iter().map(Vec::as_slice),
            [receipt.as_slice(), receipt.as_slice()],
            &community(),
            &ledger
        )
        .unwrap(),
        1
    );
    assert_eq!(ledger.entry_count(), 2);
}
#[test]
fn insufficient_quorum_missing_source_and_modified_spend_fail_closed() {
    let (ledger, _) = original_ledger();
    let (p, q, v) = sources();
    let receipt = encode_governance_fund_spend(&spend()).unwrap();
    assert!(
        verify_governance_fund_spend_history(
            [p.as_slice()],
            [q.as_slice()],
            v[..1].iter().map(Vec::as_slice),
            [receipt.as_slice()],
            &community(),
            &ledger
        )
        .is_err()
    );
    assert!(
        verify_governance_fund_spend_history(
            [],
            [q.as_slice()],
            v.iter().map(Vec::as_slice),
            [receipt.as_slice()],
            &community(),
            &ledger
        )
        .is_err()
    );
    let value: serde_json::Value = serde_json::from_slice(&receipt).unwrap();
    for (path, val) in [
        ("/payload/source_event_id", serde_json::json!("wrong-event")),
        ("/payload/spend_id", serde_json::json!("wrong-spend")),
        (
            "/payload/organization_id",
            serde_json::json!("other-tenant"),
        ),
        ("/payload/amount_minor", serde_json::json!("751")),
        ("/payload/currency", serde_json::json!("USD")),
        (
            "/payload/expense_account_id",
            serde_json::json!("fund-asset"),
        ),
        ("/payload/purpose_reference", serde_json::json!("other")),
        ("/payload/executed_at_unix_ms", serde_json::json!("1400")),
        ("/payload/observed_at_unix_ms", serde_json::json!("1800")),
    ] {
        let mut x = value.clone();
        *x.pointer_mut(path).unwrap() = val;
        let bytes = serde_json::to_vec(&x).unwrap();
        assert!(
            verify_governance_fund_spend_history(
                [p.as_slice()],
                [q.as_slice()],
                v.iter().map(Vec::as_slice),
                [bytes.as_slice()],
                &community(),
                &ledger
            )
            .is_err(),
            "{path}"
        );
    }
}
#[test]
fn invalid_schema_conflicting_spend_identity_and_huge_receipt_reject() {
    let (ledger, _) = original_ledger();
    let (p, q, v) = sources();
    let receipt = encode_governance_fund_spend(&spend()).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&receipt).unwrap();
    for (path, val) in [
        ("/schema_version", serde_json::json!(2)),
        ("/record_type", serde_json::json!("other")),
        ("/payload/amount_minor", serde_json::json!("0")),
        ("/payload/amount_minor", serde_json::json!("1.5")),
        (
            "/payload/executed_at_unix_ms",
            serde_json::json!("9223372036854775808"),
        ),
        ("/payload/spend_id", serde_json::json!("")),
    ] {
        let mut x = value.clone();
        *x.pointer_mut(path).unwrap() = val;
        assert!(
            decode_governance_fund_spend(&serde_json::to_vec(&x).unwrap()).is_err(),
            "{path}"
        );
    }
    let mut changed = value.clone();
    changed["payload"]["observed_at_unix_ms"] = serde_json::json!("1701");
    let another = serde_json::to_vec(&changed).unwrap();
    assert!(
        verify_governance_fund_spend_history(
            [p.as_slice()],
            [q.as_slice()],
            v.iter().map(Vec::as_slice),
            [receipt.as_slice(), another.as_slice()],
            &community(),
            &ledger
        )
        .is_err()
    );
    let mut injected = value;
    injected["payload"]["forged"] = serde_json::json!(true);
    assert!(decode_governance_fund_spend(&serde_json::to_vec(&injected).unwrap()).is_err());
    assert!(decode_governance_fund_spend(&vec![b' '; 1024 * 1024 + 1]).is_err());
}

use cofi_storage::governance_history::{GovernanceHistoryFact, verify_ordered_governance_history};

#[test]
fn ordered_governance_stream_preserves_original_quorum_and_journal() {
    let (ledger, _) = original_ledger();
    let (p, q, votes) = sources();
    let receipt = encode_governance_fund_spend(&spend()).unwrap();
    let stream = [
        GovernanceHistoryFact::Policy(p.as_slice()),
        GovernanceHistoryFact::Proposal(q.as_slice()),
        GovernanceHistoryFact::Approval(votes[0].as_slice()),
        GovernanceHistoryFact::Approval(votes[1].as_slice()),
        GovernanceHistoryFact::FundSpend(receipt.as_slice()),
    ];
    let result = verify_ordered_governance_history(stream, &community(), &ledger).unwrap();
    let reference = replay_governance_approvals(
        [p.as_slice()],
        [q.as_slice()],
        votes.iter().map(Vec::as_slice),
        &community(),
    )
    .unwrap();
    let proposal_id = SpendingProposalId::new("proposal-a").unwrap();
    assert_eq!(
        result.governance().proposal_count(),
        reference.proposal_count()
    );
    assert_eq!(
        result.governance().approval_count(),
        reference.approval_count()
    );
    assert_eq!(
        result.governance().authorization(&proposal_id),
        reference.authorization(&proposal_id)
    );
    assert_eq!(result.verified_spends().len(), 1);
    assert_eq!(
        result.verified_spends()[0].journal_entry_id().as_str(),
        "spending:fund-spend:spend-1"
    );
    assert_eq!(ledger.entry_count(), 2);
}

#[test]
fn ordered_governance_stream_rejects_historically_impossible_acceptance_order() {
    let (ledger, _) = original_ledger();
    let (p, q, votes) = sources();
    let receipt = encode_governance_fund_spend(&spend()).unwrap();
    use GovernanceHistoryFact::{Approval, FundSpend, Policy, Proposal};
    let invalid_streams = [
        vec![Proposal(q.as_slice()), Policy(p.as_slice())],
        vec![
            Policy(p.as_slice()),
            Approval(votes[0].as_slice()),
            Proposal(q.as_slice()),
        ],
        vec![
            Policy(p.as_slice()),
            Proposal(q.as_slice()),
            Approval(votes[0].as_slice()),
            FundSpend(receipt.as_slice()),
            Approval(votes[1].as_slice()),
        ],
        vec![
            Policy(p.as_slice()),
            Proposal(q.as_slice()),
            Approval(votes[0].as_slice()),
            Approval(votes[1].as_slice()),
            FundSpend(receipt.as_slice()),
            FundSpend(receipt.as_slice()),
        ],
    ];
    for (idx, stream) in invalid_streams.into_iter().enumerate() {
        assert!(
            verify_ordered_governance_history(stream, &community(), &ledger).is_err(),
            "invalid historical ordering case {idx}"
        );
    }
}

#[test]
fn ordered_governance_stream_rejects_repeated_accepted_events_and_missing_journal() {
    let (ledger, prior) = original_ledger();
    let (p, q, votes) = sources();
    let receipt = encode_governance_fund_spend(&spend()).unwrap();
    use GovernanceHistoryFact::{Approval, FundSpend, Policy, Proposal};
    for stream in [
        vec![Policy(p.as_slice()), Policy(p.as_slice())],
        vec![
            Policy(p.as_slice()),
            Proposal(q.as_slice()),
            Proposal(q.as_slice()),
        ],
        vec![
            Policy(p.as_slice()),
            Proposal(q.as_slice()),
            Approval(votes[0].as_slice()),
            Approval(votes[0].as_slice()),
        ],
    ] {
        assert!(verify_ordered_governance_history(stream, &community(), &ledger).is_err());
    }
    let stream = [
        Policy(p.as_slice()),
        Proposal(q.as_slice()),
        Approval(votes[0].as_slice()),
        Approval(votes[1].as_slice()),
        FundSpend(receipt.as_slice()),
    ];
    assert!(verify_ordered_governance_history(stream, &community(), &prior).is_err());
}

use cofi_disbursements::{
    BeneficiaryReference, CreationOutcome, DestinationReference, DisbursementCreation,
    DisbursementEngine, DisbursementEventId, DisbursementId, DisbursementStatus,
};
use cofi_storage::disbursement_creation::{
    decode_disbursement_creation, encode_disbursement_creation, replay_disbursement_creations,
};

fn original_disbursement_creation(
    event_id: &str,
    disbursement_id: &str,
    beneficiary: &str,
    timestamp: i64,
) -> DisbursementCreation {
    DisbursementCreation::new(
        DisbursementEventId::new(event_id).unwrap(),
        DisbursementId::new(disbursement_id).unwrap(),
        BeneficiaryReference::new(beneficiary).unwrap(),
        DestinationReference::new("destination-one").unwrap(),
        timestamp,
    )
}

fn original_governance_fixture() -> cofi_governance::GovernanceEngine {
    let (p, q, votes) = sources();
    replay_governance_approvals(
        [p.as_slice()],
        [q.as_slice()],
        votes.iter().map(Vec::as_slice),
        &community(),
    )
    .unwrap()
}

#[test]
fn disbursement_creation_replay_matches_original_domain_registry() {
    let (ledger, _) = original_ledger();
    let gov = original_governance_fixture();
    let creation = original_disbursement_creation("create-1", "disb-1", "beneficiary-a", 1800);
    let bytes = encode_disbursement_creation(&creation).unwrap();
    let spend_bytes = encode_governance_fund_spend(&spend()).unwrap();
    assert_eq!(decode_disbursement_creation(&bytes).unwrap(), creation);
    let rebuilt = replay_disbursement_creations(
        [
            (bytes.as_slice(), spend_bytes.as_slice()),
            (bytes.as_slice(), spend_bytes.as_slice()),
        ],
        &community(),
        &gov,
        &ledger,
    )
    .unwrap();
    let mut reference = DisbursementEngine::new();
    assert_eq!(
        reference
            .create(&community(), &gov, &ledger, &spend(), creation.clone())
            .unwrap(),
        CreationOutcome::Created
    );
    assert_eq!(
        rebuilt.disbursement(creation.id()),
        reference.disbursement(creation.id())
    );
    assert_eq!(
        rebuilt.disbursement(creation.id()).unwrap().status(),
        DisbursementStatus::Ready
    );
    assert_eq!(ledger.entry_count(), 2);
}

#[test]
fn disbursement_creation_rejects_reused_spend_changed_source_and_missing_journal() {
    let (ledger, prior) = original_ledger();
    let gov = original_governance_fixture();
    let spend_bytes = encode_governance_fund_spend(&spend()).unwrap();
    let source = encode_disbursement_creation(&original_disbursement_creation(
        "create-1",
        "disb-1",
        "beneficiary-a",
        1800,
    ))
    .unwrap();
    for altered in [
        original_disbursement_creation("create-1", "disb-1", "beneficiary-b", 1800),
        original_disbursement_creation("create-2", "disb-2", "beneficiary-a", 1800),
    ] {
        let changed = encode_disbursement_creation(&altered).unwrap();
        assert!(
            replay_disbursement_creations(
                [
                    (source.as_slice(), spend_bytes.as_slice()),
                    (changed.as_slice(), spend_bytes.as_slice()),
                ],
                &community(),
                &gov,
                &ledger
            )
            .is_err()
        );
    }
    let early = encode_disbursement_creation(&original_disbursement_creation(
        "create-early",
        "disb-early",
        "beneficiary-a",
        1599,
    ))
    .unwrap();
    assert!(
        replay_disbursement_creations(
            [(early.as_slice(), spend_bytes.as_slice())],
            &community(),
            &gov,
            &ledger
        )
        .is_err()
    );
    assert!(
        replay_disbursement_creations(
            [(source.as_slice(), spend_bytes.as_slice())],
            &community(),
            &gov,
            &prior
        )
        .is_err()
    );
}

#[test]
fn disbursement_creation_strict_record_and_times_fail_closed() {
    let original = encode_disbursement_creation(&original_disbursement_creation(
        "create-1",
        "disb-1",
        "beneficiary-a",
        1800,
    ))
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&original).unwrap();
    for (path, bad) in [
        ("/schema_version", serde_json::json!(2)),
        ("/record_type", serde_json::json!("other")),
        ("/payload/id", serde_json::json!("")),
        ("/payload/beneficiary_reference", serde_json::json!(" ")),
        ("/payload/created_at_unix_ms", serde_json::json!("1.2")),
        ("/payload/created_at_unix_ms", serde_json::json!("01800")),
        (
            "/payload/created_at_unix_ms",
            serde_json::json!("9223372036854775808"),
        ),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(path).unwrap() = bad;
        assert!(
            decode_disbursement_creation(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{path}"
        );
    }
    let mut extra = value;
    extra["payload"]["unknown"] = serde_json::json!(true);
    assert!(decode_disbursement_creation(&serde_json::to_vec(&extra).unwrap()).is_err());
    assert!(decode_disbursement_creation(&vec![b' '; 1024 * 1024 + 1]).is_err());
}

use cofi_disbursements::{
    DisbursementSubmission, DisbursementTerminalEvent, ProviderEventReference,
    ProviderRequestReference, ProviderSettlementReference,
};
use cofi_storage::disbursement_lifecycle::{
    DisbursementLifecycleFact, decode_disbursement_submission, decode_disbursement_terminal,
    encode_disbursement_submission, encode_disbursement_terminal, replay_disbursement_lifecycle,
};

fn submitted_event(event_id: &str, reference: &str, at: i64) -> DisbursementSubmission {
    DisbursementSubmission::new(
        DisbursementEventId::new(event_id).unwrap(),
        DisbursementId::new("disb-1").unwrap(),
        ProviderRequestReference::new(reference).unwrap(),
        at,
    )
}

fn terminal_event(event_id: &str, at: i64) -> DisbursementTerminalEvent {
    DisbursementTerminalEvent::settled(
        DisbursementEventId::new(event_id).unwrap(),
        DisbursementId::new("disb-1").unwrap(),
        ProviderEventReference::new("pe-1").unwrap(),
        ProviderSettlementReference::new("settlement-1").unwrap(),
        at,
    )
}

#[test]
fn original_disbursement_submission_and_terminal_state_replays_exactly() {
    let (ledger, _) = original_ledger();
    let governance = original_governance_fixture();
    let creation = original_disbursement_creation("create-1", "disb-1", "beneficiary-a", 1800);
    let submission = submitted_event("submit-1", "request-1", 1900);
    let terminal = terminal_event("terminal-1", 2000);
    let creation_bytes = encode_disbursement_creation(&creation).unwrap();
    let spend_bytes = encode_governance_fund_spend(&spend()).unwrap();
    let submission_bytes = encode_disbursement_submission(&submission).unwrap();
    let terminal_bytes = encode_disbursement_terminal(&terminal).unwrap();
    assert_eq!(
        decode_disbursement_submission(&submission_bytes).unwrap(),
        submission
    );
    assert_eq!(
        decode_disbursement_terminal(&terminal_bytes).unwrap(),
        terminal
    );
    use DisbursementLifecycleFact::{Creation, Submission, Terminal};
    let rebuilt = replay_disbursement_lifecycle(
        [
            Creation {
                creation: creation_bytes.as_slice(),
                spend: spend_bytes.as_slice(),
            },
            Submission(submission_bytes.as_slice()),
            Terminal(terminal_bytes.as_slice()),
        ],
        &community(),
        &governance,
        &ledger,
    )
    .unwrap();
    let mut reference = DisbursementEngine::new();
    reference
        .create(
            &community(),
            &governance,
            &ledger,
            &spend(),
            creation.clone(),
        )
        .unwrap();
    reference.submit(submission).unwrap();
    reference.record_terminal(terminal).unwrap();
    let restored = rebuilt.disbursement(creation.id()).unwrap();
    assert_eq!(Some(restored), reference.disbursement(creation.id()));
    assert_eq!(restored.status(), DisbursementStatus::Settled);
    assert_eq!(
        restored.provider_settlement_reference().unwrap().as_str(),
        "settlement-1"
    );
    assert_eq!(ledger.entry_count(), 2);
}

#[test]
fn disbursement_lifecycle_rejects_missing_predecessor_wrong_order_and_timing() {
    let (ledger, _) = original_ledger();
    let governance = original_governance_fixture();
    let creation = encode_disbursement_creation(&original_disbursement_creation(
        "create-1",
        "disb-1",
        "beneficiary-a",
        1800,
    ))
    .unwrap();
    let spend = encode_governance_fund_spend(&spend()).unwrap();
    let submit =
        encode_disbursement_submission(&submitted_event("submit-1", "request-1", 1900)).unwrap();
    let terminal = encode_disbursement_terminal(&terminal_event("terminal-1", 2000)).unwrap();
    use DisbursementLifecycleFact::{Creation, Submission, Terminal};
    for (i, stream) in [
        vec![Submission(submit.as_slice())],
        vec![Terminal(terminal.as_slice())],
        vec![
            Creation {
                creation: creation.as_slice(),
                spend: spend.as_slice(),
            },
            Terminal(terminal.as_slice()),
            Submission(submit.as_slice()),
        ],
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            replay_disbursement_lifecycle(stream, &community(), &governance, &ledger).is_err(),
            "invalid order {i}"
        );
    }
    let early_submit =
        encode_disbursement_submission(&submitted_event("submit-early", "request-early", 1700))
            .unwrap();
    assert!(
        replay_disbursement_lifecycle(
            [
                Creation {
                    creation: creation.as_slice(),
                    spend: spend.as_slice()
                },
                Submission(early_submit.as_slice())
            ],
            &community(),
            &governance,
            &ledger
        )
        .is_err()
    );
    let early_terminal =
        encode_disbursement_terminal(&terminal_event("terminal-early", 1850)).unwrap();
    assert!(
        replay_disbursement_lifecycle(
            [
                Creation {
                    creation: creation.as_slice(),
                    spend: spend.as_slice()
                },
                Submission(submit.as_slice()),
                Terminal(early_terminal.as_slice())
            ],
            &community(),
            &governance,
            &ledger
        )
        .is_err()
    );
}

#[test]
fn disbursement_lifecycle_rejects_changed_source_and_malformed_records() {
    let (ledger, _) = original_ledger();
    let governance = original_governance_fixture();
    let creation = encode_disbursement_creation(&original_disbursement_creation(
        "create-1",
        "disb-1",
        "beneficiary-a",
        1800,
    ))
    .unwrap();
    let spend = encode_governance_fund_spend(&spend()).unwrap();
    let submit =
        encode_disbursement_submission(&submitted_event("submit-1", "request-1", 1900)).unwrap();
    let changed =
        encode_disbursement_submission(&submitted_event("submit-1", "changed", 1900)).unwrap();
    use DisbursementLifecycleFact::{Creation, Submission};
    assert!(
        replay_disbursement_lifecycle(
            [
                Creation {
                    creation: creation.as_slice(),
                    spend: spend.as_slice()
                },
                Submission(submit.as_slice()),
                Submission(changed.as_slice())
            ],
            &community(),
            &governance,
            &ledger
        )
        .is_err()
    );
    let original: serde_json::Value = serde_json::from_slice(&submit).unwrap();
    for (path, value) in [
        ("/schema_version", serde_json::json!(2)),
        ("/record_type", serde_json::json!("other")),
        ("/payload/submitted_at_unix_ms", serde_json::json!("1.5")),
        (
            "/payload/submitted_at_unix_ms",
            serde_json::json!("9223372036854775808"),
        ),
        ("/payload/source_event_id", serde_json::json!(" ")),
    ] {
        let mut altered = original.clone();
        *altered.pointer_mut(path).unwrap() = value;
        assert!(
            decode_disbursement_submission(&serde_json::to_vec(&altered).unwrap()).is_err(),
            "{path}"
        );
    }
    let mut extra = original;
    extra["payload"]["extra"] = serde_json::json!(true);
    assert!(decode_disbursement_submission(&serde_json::to_vec(&extra).unwrap()).is_err());
    let terminal = encode_disbursement_terminal(&terminal_event("terminal-1", 2000)).unwrap();
    let mut invalid: serde_json::Value = serde_json::from_slice(&terminal).unwrap();
    invalid["payload"]["kind"] = serde_json::json!({"settled":{"settlement_reference":""}});
    assert!(decode_disbursement_terminal(&serde_json::to_vec(&invalid).unwrap()).is_err());
    assert!(decode_disbursement_terminal(&vec![b' '; 1024 * 1024 + 1]).is_err());
}

#[test]
fn accepted_disbursement_lifecycle_rejects_duplicate_historical_facts() {
    use DisbursementLifecycleFact::{Creation, Submission, Terminal};
    let (ledger, _) = original_ledger();
    let governance = original_governance_fixture();
    let creation = encode_disbursement_creation(&original_disbursement_creation(
        "create-1",
        "disb-1",
        "beneficiary-a",
        1800,
    ))
    .unwrap();
    let spend = encode_governance_fund_spend(&spend()).unwrap();
    let submission =
        encode_disbursement_submission(&submitted_event("submit-1", "request-1", 1900)).unwrap();
    let terminal = encode_disbursement_terminal(&terminal_event("terminal-1", 2000)).unwrap();

    let created = Creation {
        creation: creation.as_slice(),
        spend: spend.as_slice(),
    };
    let submitted = Submission(submission.as_slice());
    let settled = Terminal(terminal.as_slice());

    for (label, stream) in [
        ("creation", vec![created, created]),
        ("submission", vec![created, submitted, submitted]),
        ("terminal", vec![created, submitted, settled, settled]),
    ] {
        assert!(
            replay_disbursement_lifecycle(stream, &community(), &governance, &ledger).is_err(),
            "an accepted source stream cannot repeat a previously accepted {label} fact"
        );
    }

    assert!(
        replay_disbursement_lifecycle(
            [created, submitted, settled],
            &community(),
            &governance,
            &ledger
        )
        .is_ok()
    );
}

use cofi_storage::financial_history::{
    AcceptedFinancialFact as FinancialFact, replay_accepted_financial_history,
};

#[test]
fn shared_financial_chronology_rebuilds_original_governance_and_disbursement() {
    let (ledger, _) = original_ledger();
    let (policy, proposal, votes) = sources();
    let spend = encode_governance_fund_spend(&spend()).unwrap();
    let creation = encode_disbursement_creation(&original_disbursement_creation(
        "create-1",
        "disb-1",
        "beneficiary-a",
        1800,
    ))
    .unwrap();
    let submission =
        encode_disbursement_submission(&submitted_event("submit-1", "request-1", 1900)).unwrap();
    let terminal = encode_disbursement_terminal(&terminal_event("terminal-1", 2000)).unwrap();
    use FinancialFact::{Approval, Creation, FundSpend, Policy, Proposal, Submission, Terminal};
    let stream = [
        Policy(policy.as_slice()),
        Proposal(proposal.as_slice()),
        Approval(votes[0].as_slice()),
        Approval(votes[1].as_slice()),
        FundSpend(spend.as_slice()),
        Creation {
            creation: creation.as_slice(),
            spend_source_event_id: "spend-event-1",
        },
        Submission(submission.as_slice()),
        Terminal(terminal.as_slice()),
    ];
    let result = replay_accepted_financial_history(stream, &community(), &ledger).unwrap();
    assert_eq!(result.governance().proposal_count(), 1);
    assert_eq!(result.governance().approval_count(), 2);
    assert_eq!(result.verified_spends().len(), 1);
    let id = DisbursementId::new("disb-1").unwrap();
    assert_eq!(
        result.disbursements().disbursement(&id).unwrap().status(),
        DisbursementStatus::Settled
    );
    assert_eq!(ledger.entry_count(), 2);
}

#[test]
fn shared_financial_chronology_rejects_preaccepted_spend_and_duplicate_or_missing_sources() {
    let (ledger, _) = original_ledger();
    let (policy, proposal, votes) = sources();
    let spend = encode_governance_fund_spend(&spend()).unwrap();
    let creation = encode_disbursement_creation(&original_disbursement_creation(
        "create-1",
        "disb-1",
        "beneficiary-a",
        1800,
    ))
    .unwrap();
    let submission =
        encode_disbursement_submission(&submitted_event("submit-1", "request-1", 1900)).unwrap();
    let terminal = encode_disbursement_terminal(&terminal_event("terminal-1", 2000)).unwrap();
    use FinancialFact::{Approval, Creation, FundSpend, Policy, Proposal, Submission, Terminal};
    let created = Creation {
        creation: creation.as_slice(),
        spend_source_event_id: "spend-event-1",
    };
    let history = vec![
        Policy(policy.as_slice()),
        Proposal(proposal.as_slice()),
        Approval(votes[0].as_slice()),
        Approval(votes[1].as_slice()),
    ];
    let cases = [
        vec![created, FundSpend(spend.as_slice())],
        vec![FundSpend(spend.as_slice()), created, created],
        vec![
            FundSpend(spend.as_slice()),
            FundSpend(spend.as_slice()),
            created,
        ],
        vec![
            FundSpend(spend.as_slice()),
            created,
            Terminal(terminal.as_slice()),
        ],
        vec![
            FundSpend(spend.as_slice()),
            created,
            Submission(submission.as_slice()),
            Submission(submission.as_slice()),
        ],
        vec![
            FundSpend(spend.as_slice()),
            created,
            Submission(submission.as_slice()),
            Terminal(terminal.as_slice()),
            Terminal(terminal.as_slice()),
        ],
        vec![Creation {
            creation: creation.as_slice(),
            spend_source_event_id: "missing-spend",
        }],
        vec![
            FundSpend(spend.as_slice()),
            Creation {
                creation: creation.as_slice(),
                spend_source_event_id: "missing-spend",
            },
        ],
    ];
    for (i, tail) in cases.into_iter().enumerate() {
        let mut events = history.clone();
        events.extend(tail);
        assert!(
            replay_accepted_financial_history(events, &community(), &ledger).is_err(),
            "must reject impossible original financial chronology case {i}"
        );
    }
    assert_eq!(ledger.entry_count(), 2);
}

use cofi_storage::source_checkpoint::{
    DeclaredSourceFact, DeclaredSourceScope, SupportedSourceKind, compute_untrusted_range,
    inspect_untrusted_source_checkpoint,
};

#[test]
fn source_checkpoint_checks_range_but_never_authenticates_self_declared_history() {
    let (policy_bytes, proposal_bytes, votes) = sources();
    let scope = DeclaredSourceScope {
        authority_id: "external-source-a",
        organization_id: "org-a",
        environment_id: "test",
        previous_digest_hex: None,
    };
    let facts = [
        DeclaredSourceFact {
            sequence: "1",
            authority_id: "external-source-a",
            organization_id: "org-a",
            environment_id: "test",
            source_record_key: "policy:policy-a:1",
            kind: SupportedSourceKind::Policy,
            bytes: policy_bytes.as_slice(),
        },
        DeclaredSourceFact {
            sequence: "2",
            authority_id: "external-source-a",
            organization_id: "org-a",
            environment_id: "test",
            source_record_key: "proposal-event",
            kind: SupportedSourceKind::Proposal,
            bytes: proposal_bytes.as_slice(),
        },
        DeclaredSourceFact {
            sequence: "3",
            authority_id: "external-source-a",
            organization_id: "org-a",
            environment_id: "test",
            source_record_key: "v-1",
            kind: SupportedSourceKind::Approval,
            bytes: votes[0].as_slice(),
        },
    ];
    let calculated = compute_untrusted_range(scope, &facts).unwrap();
    let declared = serde_json::json!({
        "schema_version": 1,
        "record_type": "source.checkpoint.declaration",
        "authority_id": "external-source-a",
        "organization_id": "org-a",
        "environment_id": "test",
        "first_sequence": "1",
        "last_sequence": "3",
        "accepted_event_count": "3",
        "previous_digest_hex": null,
        "ordered_digest_hex": calculated.ordered_digest_hex(),
    });
    let data = serde_json::to_vec(&declared).unwrap();
    let inspected = inspect_untrusted_source_checkpoint(&data, &facts).unwrap();
    assert_eq!(inspected.count(), 3);
    assert_eq!(inspected.first_sequence(), 1);
    assert_eq!(inspected.last_sequence(), 3);
    assert!(
        inspected
            .require_independent_source_authentication()
            .is_err()
    );

    // A coherent, fully recomputed, caller-created checkpoint remains
    // unauthenticated: hash consistency is not an independently pinned key.
    let forged = serde_json::to_vec(&declared).unwrap();
    assert!(
        inspect_untrusted_source_checkpoint(&forged, &facts)
            .unwrap()
            .require_independent_source_authentication()
            .is_err()
    );

    for (path, bad) in [
        ("/schema_version", serde_json::json!(2)),
        (
            "/record_type",
            serde_json::json!("source.checkpoint.trusted"),
        ),
        ("/authority_id", serde_json::json!("other")),
        ("/organization_id", serde_json::json!("other-tenant")),
        ("/environment_id", serde_json::json!("production")),
        ("/first_sequence", serde_json::json!("0")),
        ("/last_sequence", serde_json::json!("2")),
        ("/accepted_event_count", serde_json::json!("4")),
        ("/accepted_event_count", serde_json::json!("3.0")),
        ("/ordered_digest_hex", serde_json::json!("bad")),
        ("/previous_digest_hex", serde_json::json!("bad")),
    ] {
        let mut modified = declared.clone();
        *modified.pointer_mut(path).unwrap() = bad;
        assert!(
            inspect_untrusted_source_checkpoint(&serde_json::to_vec(&modified).unwrap(), &facts)
                .is_err(),
            "{path}"
        );
    }
    for bad in [
        {
            let mut a = facts;
            a[1].sequence = "3";
            a
        },
        {
            let mut a = facts;
            a[1].source_record_key = "v-1";
            a
        },
        {
            let mut a = facts;
            a[1].organization_id = "other";
            a
        },
        {
            let mut a = facts;
            a[2].source_record_key = "fake-vote";
            a
        },
        {
            let mut a = facts;
            a[2].sequence = "18446744073709551616";
            a
        },
    ] {
        assert!(inspect_untrusted_source_checkpoint(&data, &bad).is_err());
    }
    assert!(inspect_untrusted_source_checkpoint(&data, &facts[..2]).is_err());
    assert!(inspect_untrusted_source_checkpoint(&data, &[facts[1], facts[0], facts[2]]).is_err());
    let mut unknown = declared.clone();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(
        inspect_untrusted_source_checkpoint(&serde_json::to_vec(&unknown).unwrap(), &facts)
            .is_err()
    );
    assert!(inspect_untrusted_source_checkpoint(&vec![b' '; 4097], &facts).is_err());
}

#[test]
fn genesis_checkpoint_rejects_votes_without_matching_prior_proposal() {
    let (policy, proposal, votes) = sources();
    let mut foreign_vote: serde_json::Value = serde_json::from_slice(&votes[0]).unwrap();
    // The event identity is unchanged and the declared organization is org-a.
    // The original vote has NO organization field; its proposal ancestor must
    // bind it to the declared organization in a complete genesis range.
    foreign_vote["payload"]["proposal_id"] = serde_json::json!("foreign-proposal");
    let foreign_vote = serde_json::to_vec(&foreign_vote).unwrap();
    let scope = DeclaredSourceScope {
        authority_id: "source-a",
        organization_id: "org-a",
        environment_id: "test",
        previous_digest_hex: None,
    };
    let policy_fact = DeclaredSourceFact {
        sequence: "1",
        authority_id: "source-a",
        organization_id: "org-a",
        environment_id: "test",
        source_record_key: "policy:policy-a:1",
        kind: SupportedSourceKind::Policy,
        bytes: &policy,
    };
    let proposal_fact = DeclaredSourceFact {
        sequence: "2",
        source_record_key: "proposal-event",
        kind: SupportedSourceKind::Proposal,
        bytes: &proposal,
        ..policy_fact
    };
    let valid_vote = DeclaredSourceFact {
        sequence: "3",
        source_record_key: "v-1",
        kind: SupportedSourceKind::Approval,
        bytes: &votes[0],
        ..policy_fact
    };
    let foreign_vote_fact = DeclaredSourceFact {
        bytes: &foreign_vote,
        ..valid_vote
    };
    assert!(compute_untrusted_range(scope, &[policy_fact, proposal_fact, valid_vote]).is_ok());
    assert!(
        compute_untrusted_range(scope, &[policy_fact, proposal_fact, foreign_vote_fact]).is_err(),
        "genesis must not allow a vote to claim an unrelated proposal"
    );
    let vote_before_proposal = DeclaredSourceFact {
        sequence: "2",
        ..valid_vote
    };
    let late_proposal = DeclaredSourceFact {
        sequence: "3",
        ..proposal_fact
    };
    assert!(
        compute_untrusted_range(scope, &[policy_fact, vote_before_proposal, late_proposal])
            .is_err(),
        "a later proposal cannot supply already-accepted vote ancestry"
    );

    // Two different source event IDs must not re-register the same original
    // proposal identity in a declared complete genesis stream.
    let mut duplicate_proposal: serde_json::Value = serde_json::from_slice(&proposal).unwrap();
    duplicate_proposal["payload"]["source_event_id"] = serde_json::json!("proposal-event-2");
    let duplicate_proposal = serde_json::to_vec(&duplicate_proposal).unwrap();
    let duplicate = DeclaredSourceFact {
        sequence: "3",
        source_record_key: "proposal-event-2",
        bytes: &duplicate_proposal,
        ..proposal_fact
    };
    assert!(
        compute_untrusted_range(scope, &[policy_fact, proposal_fact, duplicate]).is_err(),
        "distinct proposal events must not mask a reused original proposal ID"
    );

    // A non-genesis range may depend on a prior range, but that previous
    // hash is caller-controlled; it does NOT prove the vote's tenant ancestry.
    let continuation = DeclaredSourceScope {
        previous_digest_hex: Some(
            "0000000000000000000000000000000000000000000000000000000000000000",
        ),
        ..scope
    };
    let single = DeclaredSourceFact {
        sequence: "4",
        ..foreign_vote_fact
    };
    let self_consistent = compute_untrusted_range(continuation, &[single]).unwrap();
    assert!(
        self_consistent
            .require_independent_source_authentication()
            .is_err()
    );
}

#[test]
fn genesis_checkpoint_rejects_proposals_without_matching_prior_policy() {
    let (policy, proposal, _) = sources();
    let scope = DeclaredSourceScope {
        authority_id: "source-a",
        organization_id: "org-a",
        environment_id: "test",
        previous_digest_hex: None,
    };
    let policy_fact = DeclaredSourceFact {
        sequence: "1",
        authority_id: "source-a",
        organization_id: "org-a",
        environment_id: "test",
        source_record_key: "policy:policy-a:1",
        kind: SupportedSourceKind::Policy,
        bytes: &policy,
    };
    let proposal_fact = DeclaredSourceFact {
        sequence: "2",
        source_record_key: "proposal-event",
        kind: SupportedSourceKind::Proposal,
        bytes: &proposal,
        ..policy_fact
    };
    assert!(compute_untrusted_range(scope, &[policy_fact, proposal_fact]).is_ok());
    let unsupported_first = DeclaredSourceFact {
        sequence: "1",
        ..proposal_fact
    };
    assert!(
        compute_untrusted_range(scope, &[unsupported_first]).is_err(),
        "a genesis proposal must have a previously accepted policy"
    );
    let later_policy = DeclaredSourceFact {
        sequence: "2",
        ..policy_fact
    };
    assert!(
        compute_untrusted_range(scope, &[unsupported_first, later_policy]).is_err(),
        "a later policy cannot authorize an already declared proposal"
    );
    let original: serde_json::Value = serde_json::from_slice(&proposal).unwrap();
    for (field, replacement) in [
        ("policy_id", serde_json::json!("foreign-policy")),
        ("policy_version", serde_json::json!(2)),
        ("community_id", serde_json::json!("other-community")),
        ("fund_id", serde_json::json!("other-fund")),
        ("currency", serde_json::json!("USD")),
        ("amount_minor", serde_json::json!("1001")),
    ] {
        let mut altered = original.clone();
        altered["payload"][field] = replacement;
        let altered = serde_json::to_vec(&altered).unwrap();
        let fact = DeclaredSourceFact {
            bytes: &altered,
            ..proposal_fact
        };
        assert!(
            compute_untrusted_range(scope, &[policy_fact, fact]).is_err(),
            "policy-proposal binding must fail closed for {field}"
        );
    }

    // The previous checkpoint hash on a continuation is self-declared and
    // cannot serve as independent proof of its earlier policy ancestors.
    let continuation = DeclaredSourceScope {
        previous_digest_hex: Some(
            "0000000000000000000000000000000000000000000000000000000000000000",
        ),
        ..scope
    };
    let fact = DeclaredSourceFact {
        sequence: "4",
        ..proposal_fact
    };
    let consistent_but_untrusted = compute_untrusted_range(continuation, &[fact]).unwrap();
    assert!(
        consistent_but_untrusted
            .require_independent_source_authentication()
            .is_err()
    );
}

#[test]
fn genesis_checkpoint_rejects_invalid_approval_lifecycle_and_identity() {
    let (policy, proposal, votes) = sources();
    let scope = DeclaredSourceScope {
        authority_id: "source-a",
        organization_id: "org-a",
        environment_id: "test",
        previous_digest_hex: None,
    };
    let first = DeclaredSourceFact {
        sequence: "1",
        authority_id: "source-a",
        organization_id: "org-a",
        environment_id: "test",
        source_record_key: "policy:policy-a:1",
        kind: SupportedSourceKind::Policy,
        bytes: &policy,
    };
    let second = DeclaredSourceFact {
        sequence: "2",
        source_record_key: "proposal-event",
        kind: SupportedSourceKind::Proposal,
        bytes: &proposal,
        ..first
    };
    let third = DeclaredSourceFact {
        sequence: "3",
        source_record_key: "v-1",
        kind: SupportedSourceKind::Approval,
        bytes: &votes[0],
        ..first
    };
    let fourth = DeclaredSourceFact {
        sequence: "4",
        source_record_key: "v-2",
        kind: SupportedSourceKind::Approval,
        bytes: &votes[1],
        ..first
    };
    assert!(compute_untrusted_range(scope, &[first, second, third, fourth]).is_ok());
    let original: serde_json::Value = serde_json::from_slice(&votes[0]).unwrap();
    for (field, replacement) in [
        ("approved_at_unix_ms", serde_json::json!("999")),
        ("approved_at_unix_ms", serde_json::json!("2001")),
    ] {
        let mut altered = original.clone();
        altered["payload"][field] = replacement;
        let bytes = serde_json::to_vec(&altered).unwrap();
        let event = DeclaredSourceFact {
            bytes: &bytes,
            ..third
        };
        assert!(
            compute_untrusted_range(scope, &[first, second, event]).is_err(),
            "invalid original approval time must not pass declared genesis"
        );
    }
    let original_second: serde_json::Value = serde_json::from_slice(&votes[1]).unwrap();
    for (field, replacement) in [
        ("id", serde_json::json!("vote-1")),
        ("approver_party_id", serde_json::json!("owner")),
    ] {
        let mut altered = original_second.clone();
        altered["payload"][field] = replacement;
        let bytes = serde_json::to_vec(&altered).unwrap();
        let event = DeclaredSourceFact {
            bytes: &bytes,
            ..fourth
        };
        assert!(
            compute_untrusted_range(scope, &[first, second, third, event]).is_err(),
            "duplicate original approval identity or same-party vote must fail: {field}"
        );
    }
    // Even a new event/id/party cannot vote after the original policy's
    // configured approval quorum is already reached.
    let mut after_quorum = original_second;
    after_quorum["payload"]["source_event_id"] = serde_json::json!("v-3");
    after_quorum["payload"]["id"] = serde_json::json!("vote-3");
    after_quorum["payload"]["approver_party_id"] = serde_json::json!("member");
    let encoded = serde_json::to_vec(&after_quorum).unwrap();
    let fifth = DeclaredSourceFact {
        sequence: "5",
        source_record_key: "v-3",
        bytes: &encoded,
        ..fourth
    };
    assert!(
        compute_untrusted_range(scope, &[first, second, third, fourth, fifth]).is_err(),
        "an additional vote after original quorum is not a valid first acceptance"
    );
    // Non-genesis continuation is still consistency-only: it cannot prove
    // earlier accepted proposal/party/quorum ancestry using a supplied hash.
    let continuation = DeclaredSourceScope {
        previous_digest_hex: Some(
            "0000000000000000000000000000000000000000000000000000000000000000",
        ),
        ..scope
    };
    let untrusted = DeclaredSourceFact {
        sequence: "6",
        ..fifth
    };
    assert!(
        compute_untrusted_range(continuation, &[untrusted])
            .unwrap()
            .require_independent_source_authentication()
            .is_err()
    );
}

#[test]
fn genesis_checkpoint_requires_original_approved_spend_ancestry() {
    let (policy, proposal, votes) = sources();
    let spend = encode_governance_fund_spend(&spend()).unwrap();
    let scope = DeclaredSourceScope {
        authority_id: "source-a",
        organization_id: "org-a",
        environment_id: "test",
        previous_digest_hex: None,
    };
    let first = DeclaredSourceFact {
        sequence: "1",
        authority_id: "source-a",
        organization_id: "org-a",
        environment_id: "test",
        source_record_key: "policy:policy-a:1",
        kind: SupportedSourceKind::Policy,
        bytes: &policy,
    };
    let second = DeclaredSourceFact {
        sequence: "2",
        source_record_key: "proposal-event",
        kind: SupportedSourceKind::Proposal,
        bytes: &proposal,
        ..first
    };
    let third = DeclaredSourceFact {
        sequence: "3",
        source_record_key: "v-1",
        kind: SupportedSourceKind::Approval,
        bytes: &votes[0],
        ..first
    };
    let fourth = DeclaredSourceFact {
        sequence: "4",
        source_record_key: "v-2",
        kind: SupportedSourceKind::Approval,
        bytes: &votes[1],
        ..first
    };
    let fifth = DeclaredSourceFact {
        sequence: "5",
        source_record_key: "spend-event-1",
        kind: SupportedSourceKind::FundSpend,
        bytes: &spend,
        ..first
    };
    assert!(compute_untrusted_range(scope, &[first, second, third, fourth, fifth]).is_ok());
    let premature = DeclaredSourceFact {
        sequence: "4",
        ..fifth
    };
    assert!(
        compute_untrusted_range(scope, &[first, second, third, premature]).is_err(),
        "genesis spend must follow original approval quorum"
    );
    let orphan = DeclaredSourceFact {
        sequence: "1",
        ..fifth
    };
    assert!(
        compute_untrusted_range(scope, &[orphan]).is_err(),
        "genesis spend must refer to a prior original proposal"
    );
    let base: serde_json::Value = serde_json::from_slice(&spend).unwrap();
    for (field, val) in [
        ("proposal_id", serde_json::json!("foreign-proposal")),
        ("community_id", serde_json::json!("other-community")),
        ("fund_id", serde_json::json!("other-fund")),
        ("currency", serde_json::json!("USD")),
        ("amount_minor", serde_json::json!("751")),
        ("purpose_reference", serde_json::json!("not-the-proposal")),
        ("executed_at_unix_ms", serde_json::json!("1499")),
    ] {
        let mut altered = base.clone();
        altered["payload"][field] = val;
        let record = serde_json::to_vec(&altered).unwrap();
        let event = DeclaredSourceFact {
            bytes: &record,
            ..fifth
        };
        assert!(
            compute_untrusted_range(scope, &[first, second, third, fourth, event]).is_err(),
            "original spend ancestry should reject {field}"
        );
    }
    let mut different_spend = base;
    different_spend["payload"]["source_event_id"] = serde_json::json!("spend-event-2");
    different_spend["payload"]["spend_id"] = serde_json::json!("spend-2");
    let record = serde_json::to_vec(&different_spend).unwrap();
    let second_spend = DeclaredSourceFact {
        sequence: "6",
        source_record_key: "spend-event-2",
        bytes: &record,
        ..fifth
    };
    assert!(
        compute_untrusted_range(scope, &[first, second, third, fourth, fifth, second_spend])
            .is_err(),
        "second distinct spend must not consume the same proposal twice"
    );
    let continuation = DeclaredSourceScope {
        previous_digest_hex: Some(
            "0000000000000000000000000000000000000000000000000000000000000000",
        ),
        ..scope
    };
    let later = DeclaredSourceFact {
        sequence: "7",
        ..fifth
    };
    assert!(
        compute_untrusted_range(continuation, &[later])
            .unwrap()
            .require_independent_source_authentication()
            .is_err()
    );
}

#[test]
fn segmented_genesis_requires_actual_links_and_global_ancestry() {
    use cofi_storage::source_checkpoint::{DeclaredSourceSegment, compute_untrusted_genesis_chain};
    let (policy, proposal, votes) = sources();
    let spend = encode_governance_fund_spend(&spend()).unwrap();
    let scope = DeclaredSourceScope {
        authority_id: "source-a",
        organization_id: "org-a",
        environment_id: "test",
        previous_digest_hex: None,
    };
    let first = DeclaredSourceFact {
        sequence: "1",
        authority_id: "source-a",
        organization_id: "org-a",
        environment_id: "test",
        source_record_key: "policy:policy-a:1",
        kind: SupportedSourceKind::Policy,
        bytes: &policy,
    };
    let second = DeclaredSourceFact {
        sequence: "2",
        source_record_key: "proposal-event",
        kind: SupportedSourceKind::Proposal,
        bytes: &proposal,
        ..first
    };
    let third = DeclaredSourceFact {
        sequence: "3",
        source_record_key: "v-1",
        kind: SupportedSourceKind::Approval,
        bytes: &votes[0],
        ..first
    };
    let fourth = DeclaredSourceFact {
        sequence: "4",
        source_record_key: "v-2",
        kind: SupportedSourceKind::Approval,
        bytes: &votes[1],
        ..first
    };
    let fifth = DeclaredSourceFact {
        sequence: "5",
        source_record_key: "spend-event-1",
        kind: SupportedSourceKind::FundSpend,
        bytes: &spend,
        ..first
    };
    let head = [first, second];
    let tail = [third, fourth, fifth];
    let partial_head = compute_untrusted_range(scope, &head).unwrap();
    let next_scope = DeclaredSourceScope {
        previous_digest_hex: Some(partial_head.ordered_digest_hex()),
        ..scope
    };
    // A later fragment alone cannot validate ancestors it does not contain.
    assert!(compute_untrusted_range(next_scope, &tail).is_ok());
    let fragments = [
        DeclaredSourceSegment {
            scope,
            facts: &head,
        },
        DeclaredSourceSegment {
            scope: next_scope,
            facts: &tail,
        },
    ];
    let combined = compute_untrusted_genesis_chain(&fragments).unwrap();
    assert_eq!(combined.count(), 5);
    assert_eq!(combined.first_sequence(), 1);
    assert_eq!(combined.last_sequence(), 5);
    assert_eq!(
        combined.ordered_digest_hex(),
        compute_untrusted_range(scope, &[first, second, third, fourth, fifth])
            .unwrap()
            .ordered_digest_hex(),
    );
    assert!(
        combined
            .require_independent_source_authentication()
            .is_err()
    );

    let fake_scope = DeclaredSourceScope {
        previous_digest_hex: Some(
            "0000000000000000000000000000000000000000000000000000000000000000",
        ),
        ..scope
    };
    // Each fragment is individually self-consistent, but the declared
    // predecessor digest is not linked to the previous fragment.
    assert!(compute_untrusted_range(fake_scope, &tail).is_ok());
    assert!(
        compute_untrusted_genesis_chain(&[
            fragments[0],
            DeclaredSourceSegment {
                scope: fake_scope,
                facts: &tail
            }
        ])
        .is_err()
    );
    let skipped = [
        DeclaredSourceFact {
            sequence: "4",
            ..third
        },
        DeclaredSourceFact {
            sequence: "5",
            ..fourth
        },
        DeclaredSourceFact {
            sequence: "6",
            ..fifth
        },
    ];
    assert!(compute_untrusted_range(next_scope, &skipped).is_ok());
    assert!(
        compute_untrusted_genesis_chain(&[
            fragments[0],
            DeclaredSourceSegment {
                scope: next_scope,
                facts: &skipped
            }
        ])
        .is_err()
    );
    let wrong_tenant = DeclaredSourceScope {
        authority_id: "another-authority",
        ..next_scope
    };
    assert!(compute_untrusted_range(wrong_tenant, &[third, fourth]).is_err());
    assert!(
        compute_untrusted_genesis_chain(&[
            fragments[0],
            DeclaredSourceSegment {
                scope: wrong_tenant,
                facts: &tail
            }
        ])
        .is_err()
    );
    // The spend follows just one approval across this chunk boundary, but
    // standalone later-fragment checking cannot know the earlier quorum.
    let premature = [
        third,
        DeclaredSourceFact {
            sequence: "4",
            ..fifth
        },
    ];
    assert!(compute_untrusted_range(next_scope, &premature).is_ok());
    assert!(
        compute_untrusted_genesis_chain(&[
            fragments[0],
            DeclaredSourceSegment {
                scope: next_scope,
                facts: &premature
            }
        ])
        .is_err()
    );
    // A continuation containing only an unscoped original approval can look
    // locally self-consistent under a changed claimed organization; the
    // complete chain must reject that tenant splice.
    let claimed_vote = DeclaredSourceFact {
        organization_id: "org-b",
        ..third
    };
    let claimed_scope = DeclaredSourceScope {
        organization_id: "org-b",
        ..next_scope
    };
    assert!(compute_untrusted_range(claimed_scope, &[claimed_vote]).is_ok());
    assert!(
        compute_untrusted_genesis_chain(&[
            fragments[0],
            DeclaredSourceSegment {
                scope: claimed_scope,
                facts: &[claimed_vote],
            }
        ])
        .is_err()
    );
    // The duplicate original source key looks fine in an isolated non-genesis
    // range but is rejected by the merged global first-time source index.
    let repeated_proposal = DeclaredSourceFact {
        sequence: "3",
        ..second
    };
    assert!(compute_untrusted_range(next_scope, &[repeated_proposal]).is_ok());
    assert!(
        compute_untrusted_genesis_chain(&[
            fragments[0],
            DeclaredSourceSegment {
                scope: next_scope,
                facts: &[repeated_proposal],
            }
        ])
        .is_err()
    );
    assert!(compute_untrusted_genesis_chain(&[fragments[0]; 65]).is_err());
    assert!(compute_untrusted_genesis_chain(&[]).is_err());
    assert!(
        compute_untrusted_genesis_chain(&[DeclaredSourceSegment { scope, facts: &[] }]).is_err()
    );
}

#[test]
fn checkpoint_cannot_relabel_original_financial_organization() {
    let (policy, proposal, _) = sources();
    let spend_bytes = encode_governance_fund_spend(&spend()).unwrap();
    use cofi_storage::source_checkpoint::{
        DeclaredSourceFact, DeclaredSourceScope, SupportedSourceKind, compute_untrusted_range,
    };
    let cases = [
        (
            SupportedSourceKind::Policy,
            "policy:policy-a:1",
            policy.as_slice(),
        ),
        (
            SupportedSourceKind::Proposal,
            "proposal-event",
            proposal.as_slice(),
        ),
        (
            SupportedSourceKind::FundSpend,
            "spend-event-1",
            spend_bytes.as_slice(),
        ),
    ];
    for (kind, source_record_key, bytes) in cases {
        // A standalone proposal or fund spend cannot form a valid *genesis*
        // range: both need earlier original governance ancestors. A declared
        // continuation remains unauthenticated, but still exposes attempts to
        // relabel the decoded original financial organization.
        let needs_ancestors = matches!(
            kind,
            SupportedSourceKind::Proposal | SupportedSourceKind::FundSpend
        );
        let valid = DeclaredSourceFact {
            sequence: if needs_ancestors { "4" } else { "1" },
            authority_id: "source",
            organization_id: "org-a",
            environment_id: "test",
            source_record_key,
            kind,
            bytes,
        };
        let valid_scope = DeclaredSourceScope {
            authority_id: "source",
            organization_id: "org-a",
            environment_id: "test",
            previous_digest_hex: needs_ancestors
                .then_some("0000000000000000000000000000000000000000000000000000000000000000"),
        };
        let verified_consistency = compute_untrusted_range(valid_scope, &[valid]).unwrap();
        assert!(
            verified_consistency
                .require_independent_source_authentication()
                .is_err()
        );
        let claimed = DeclaredSourceFact {
            organization_id: "org-b",
            ..valid
        };
        let claimed_scope = DeclaredSourceScope {
            organization_id: "org-b",
            ..valid_scope
        };
        assert!(
            compute_untrusted_range(claimed_scope, &[claimed]).is_err(),
            "original {kind:?} organization may not be relabeled"
        );
    }
}
