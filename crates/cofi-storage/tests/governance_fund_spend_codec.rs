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
