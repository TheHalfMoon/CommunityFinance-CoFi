#![allow(clippy::unwrap_used)]
include!("support/payment_stream_fixture.rs");

use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundAllocationAccounts, FundAllocationBridge,
    FundAllocationEvent, FundAllocationEventId, FundAllocationId, FundAllocationOutcome, FundId,
    FundTransferBridge, FundTransferEvent, FundTransferEventId, FundTransferId,
    FundTransferOutcome, Organization, OrganizationId,
};
use cofi_storage::causal_fund_stream::{CausalFundFlow, rebuild_causal_fund_stream};
use cofi_storage::fund_movement::{FundMovementFact, encode_fund_movement};

struct SourceBundle {
    receipts: Vec<Vec<u8>>,
    billing: Vec<BillingLedgerAccounts>,
    p26: Vec<Vec<u8>>,
    p27: Vec<Vec<u8>>,
    community: CommunityRegistry,
    genesis: Ledger,
    after_p25: Ledger,
    after_p26: Ledger,
    after_p27: Ledger,
}
fn full_two_tenant_fund_reference() -> SourceBundle {
    let (receipts, billing, mut genesis, _) = input();
    for (tenant, suffix) in [("org-1", ""), ("org-2", "-2")] {
        let scope = LedgerScopeId::new(tenant).unwrap();
        for account in [
            format!("fund-asset-a{suffix}"),
            format!("fund-asset-b{suffix}"),
        ] {
            genesis
                .register_account(Account::new(
                    AccountId::new(account).unwrap(),
                    scope.clone(),
                    AccountKind::Asset,
                    Currency::new("SAR").unwrap(),
                ))
                .unwrap();
        }
    }
    let mut community = CommunityRegistry::new();
    for (tenant, suffix) in [("org-1", ""), ("org-2", "-2")] {
        community
            .register_organization(Organization::new(OrganizationId::new(tenant).unwrap()))
            .unwrap();
        let community_id = CommunityId::new(format!("community{suffix}")).unwrap();
        community
            .register_community(Community::new(
                community_id.clone(),
                OrganizationId::new(tenant).unwrap(),
            ))
            .unwrap();
        for (fund, account) in [
            (format!("fund-a{suffix}"), format!("fund-asset-a{suffix}")),
            (format!("fund-b{suffix}"), format!("fund-asset-b{suffix}")),
        ] {
            community
                .register_fund(
                    Fund::new(
                        FundId::new(fund).unwrap(),
                        community_id.clone(),
                        AccountId::new(account).unwrap(),
                        Currency::new("SAR").unwrap(),
                    ),
                    &genesis,
                )
                .unwrap();
        }
    }
    let mut after_p25 = genesis.clone();
    let mut captures = AuthorizedCaptureRegistry::new();
    let mut payouts = AuthorizedPayoutRegistry::new();
    for (p25, bill) in receipts.iter().zip(&billing) {
        let evidence = decode_payout_evidence(p25).unwrap();
        let capture = evidence.capture().request();
        let invoice = capture
            .authorized_finalization()
            .finalized()
            .to_billing_event();
        BillingLedgerBridge::new()
            .apply(&invoice, bill, &mut after_p25)
            .unwrap();
        let accepted = captures
            .capture(
                capture.clone(),
                evidence.capture().accounts(),
                &mut after_p25,
            )
            .unwrap()
            .authorization()
            .clone();
        let ev = evidence.event();
        payouts
            .apply(
                AuthorizedPayoutRequest::new(
                    ev.source_event_id().clone(),
                    ev.payout_id().clone(),
                    ev.bank_transaction_reference().clone(),
                    accepted,
                    ev.processor_fee_minor(),
                    ev.net_amount_minor(),
                    ev.paid_at_unix_ms(),
                    ev.observed_at_unix_ms(),
                ),
                evidence.accounts(),
                &mut after_p25,
            )
            .unwrap();
    }
    let mut after_p26 = after_p25.clone();
    let mut p26 = Vec::new();
    let mut p27 = Vec::new();
    let mut transfers = Vec::new();
    for (index, receipt) in receipts.iter().enumerate() {
        let suffix = if index == 0 { "" } else { "-2" };
        let checked = decode_payout_evidence(receipt).unwrap();
        let ev = checked.event();
        let scope = ev.organization_scope().clone();
        let alloc = FundAllocationEvent::new(
            FundAllocationEventId::new(format!("allocation-event{suffix}")).unwrap(),
            scope.clone(),
            FundId::new(format!("fund-a{suffix}")).unwrap(),
            FundAllocationId::new(format!("allocation{suffix}")).unwrap(),
            Currency::new("SAR").unwrap(),
            ev.net_amount_minor(),
            7 * DAY,
            7 * DAY + 100,
        );
        let source_cash = checked.accounts().bank_cash().clone();
        p26.push(
            encode_fund_movement(&FundMovementFact::Allocation {
                event: alloc.clone(),
                source_cash: source_cash.clone(),
            })
            .unwrap(),
        );
        assert!(matches!(
            FundAllocationBridge::new()
                .apply(
                    &community,
                    &alloc,
                    &FundAllocationAccounts::new(source_cash),
                    &mut after_p26,
                )
                .unwrap(),
            FundAllocationOutcome::Committed { .. }
        ));
        let transfer = FundTransferEvent::new(
            FundTransferEventId::new(format!("transfer-event{suffix}")).unwrap(),
            scope,
            FundId::new(format!("fund-a{suffix}")).unwrap(),
            FundId::new(format!("fund-b{suffix}")).unwrap(),
            FundTransferId::new(format!("transfer{suffix}")).unwrap(),
            Currency::new("SAR").unwrap(),
            ev.net_amount_minor(),
            8 * DAY,
            8 * DAY + 100,
        );
        p27.push(encode_fund_movement(&FundMovementFact::Transfer(transfer.clone())).unwrap());
        transfers.push(transfer);
    }
    let mut after_p27 = after_p26.clone();
    for transfer in &transfers {
        assert!(matches!(
            FundTransferBridge::new()
                .apply(&community, transfer, &mut after_p27)
                .unwrap(),
            FundTransferOutcome::Committed { .. }
        ));
    }
    SourceBundle {
        receipts,
        billing,
        p26,
        p27,
        community,
        genesis,
        after_p25,
        after_p26,
        after_p27,
    }
}
fn inputs<'a>(b: &'a SourceBundle) -> [CausalFundFlow<'a>; 2] {
    [
        CausalFundFlow {
            sequence: 1,
            p25_receipt: &b.receipts[0],
            p26_source: &b.p26[0],
            p27_source: &b.p27[0],
            billing_accounts: &b.billing[0],
        },
        CausalFundFlow {
            sequence: 2,
            p25_receipt: &b.receipts[1],
            p26_source: &b.p26[1],
            p27_source: &b.p27[1],
            billing_accounts: &b.billing[1],
        },
    ]
}
#[test]
fn reconstructs_two_tenant_original_consumed_payout_and_fund_transfer_indexes() {
    let b = full_two_tenant_fund_reference();
    let flows = inputs(&b);
    let result = rebuild_causal_fund_stream(
        &flows,
        &b.community,
        &b.genesis,
        &b.after_p25,
        &b.after_p26,
        &b.after_p27,
    )
    .unwrap();
    assert_eq!(result.flow_count(), 2);
    assert_eq!(result.captures().authorization_count(), 2);
    assert_eq!(result.payouts().authorization_count(), 2);
    assert_eq!(result.allocations().authorization_count(), 2);
    assert_eq!(result.allocations().payout_allocation_count(), 2);
    assert_eq!(result.transfers().authorization_count(), 2);
    assert_eq!(result.transfers().allocation_transfer_count(), 2);
    assert_eq!(result.ledger().entry_count(), 10);
    assert_eq!(b.genesis.entry_count(), 0);
    assert_eq!(b.after_p25.entry_count(), 6);
    assert_eq!(b.after_p26.entry_count(), 8);
    assert_eq!(b.after_p27.entry_count(), 10);
}
#[test]
fn rejects_reused_consumed_authority_and_untrusted_stage_snapshot() {
    let b = full_two_tenant_fund_reference();
    let mut flows = inputs(&b);
    flows[1].p26_source = &b.p26[0];
    assert!(
        rebuild_causal_fund_stream(
            &flows,
            &b.community,
            &b.genesis,
            &b.after_p25,
            &b.after_p26,
            &b.after_p27
        )
        .is_err()
    );
    let mut flows = inputs(&b);
    flows[1].p27_source = &b.p27[0];
    assert!(
        rebuild_causal_fund_stream(
            &flows,
            &b.community,
            &b.genesis,
            &b.after_p25,
            &b.after_p26,
            &b.after_p27
        )
        .is_err()
    );
    let flows = inputs(&b);
    assert!(
        rebuild_causal_fund_stream(
            &flows,
            &b.community,
            &b.genesis,
            &b.after_p26,
            &b.after_p26,
            &b.after_p27
        )
        .is_err()
    );
    assert!(
        rebuild_causal_fund_stream(
            &flows,
            &b.community,
            &b.genesis,
            &b.after_p25,
            &b.after_p27,
            &b.after_p27
        )
        .is_err()
    );
}
#[test]
fn rejects_corrupt_cross_tenant_original_fund_source() {
    let b = full_two_tenant_fund_reference();
    for (path, value) in [
        (
            "/payload/source_event_id",
            serde_json::json!("changed-source"),
        ),
        ("/payload/organization_scope", serde_json::json!("org-1")),
        ("/payload/amount_minor", serde_json::json!("999")),
        ("/payload/source_cash", serde_json::json!("bank-cash")),
    ] {
        let mut modified: serde_json::Value = serde_json::from_slice(&b.p26[1]).unwrap();
        *modified.pointer_mut(path).unwrap() = value;
        let altered = serde_json::to_vec(&modified).unwrap();
        let mut flows = inputs(&b);
        flows[1].p26_source = &altered;
        assert!(
            rebuild_causal_fund_stream(
                &flows,
                &b.community,
                &b.genesis,
                &b.after_p25,
                &b.after_p26,
                &b.after_p27
            )
            .is_err(),
            "{path}"
        );
    }
}

#[test]
fn rejects_changed_original_transfer_receipt_and_reference_cutoff() {
    let b = full_two_tenant_fund_reference();
    for (path, value) in [
        (
            "/payload/source_event_id",
            serde_json::json!("forged-transfer"),
        ),
        ("/payload/source_fund_id", serde_json::json!("fund-b-2")),
        ("/payload/destination_fund_id", serde_json::json!("fund-a")),
        ("/payload/amount_minor", serde_json::json!("999")),
        ("/payload/effective_at_unix_ms", serde_json::json!("1")),
        ("/payload/observed_at_unix_ms", serde_json::json!("1")),
    ] {
        let mut modified: serde_json::Value = serde_json::from_slice(&b.p27[1]).unwrap();
        *modified.pointer_mut(path).unwrap() = value;
        let receipt = serde_json::to_vec(&modified).unwrap();
        let mut flows = inputs(&b);
        flows[1].p27_source = &receipt;
        assert!(
            rebuild_causal_fund_stream(
                &flows,
                &b.community,
                &b.genesis,
                &b.after_p25,
                &b.after_p26,
                &b.after_p27
            )
            .is_err(),
            "{path}"
        );
    }
    let flows = inputs(&b);
    assert!(
        rebuild_causal_fund_stream(
            &flows,
            &b.community,
            &b.genesis,
            &b.after_p25,
            &b.after_p26,
            &b.after_p26
        )
        .is_err()
    );
}

#[test]
fn rejects_unreferenced_accounts_at_every_staged_fund_checkpoint() {
    let b = full_two_tenant_fund_reference();
    let flows = inputs(&b);
    let extra = Account::new(
        AccountId::new("unlisted-fund-checkpoint-account").unwrap(),
        LedgerScopeId::new("unknown-scope").unwrap(),
        AccountKind::Asset,
        Currency::new("SAR").unwrap(),
    );
    let mut altered_genesis = b.genesis.clone();
    altered_genesis.register_account(extra.clone()).unwrap();
    assert!(
        rebuild_causal_fund_stream(
            &flows,
            &b.community,
            &altered_genesis,
            &b.after_p25,
            &b.after_p26,
            &b.after_p27
        )
        .is_err()
    );

    let mut altered_p25 = b.after_p25.clone();
    altered_p25.register_account(extra.clone()).unwrap();
    assert!(
        rebuild_causal_fund_stream(
            &flows,
            &b.community,
            &b.genesis,
            &altered_p25,
            &b.after_p26,
            &b.after_p27
        )
        .is_err()
    );

    let mut altered_p26 = b.after_p26.clone();
    altered_p26.register_account(extra.clone()).unwrap();
    assert!(
        rebuild_causal_fund_stream(
            &flows,
            &b.community,
            &b.genesis,
            &b.after_p25,
            &altered_p26,
            &b.after_p27
        )
        .is_err()
    );

    let mut altered_p27 = b.after_p27.clone();
    altered_p27.register_account(extra).unwrap();
    assert!(
        rebuild_causal_fund_stream(
            &flows,
            &b.community,
            &b.genesis,
            &b.after_p25,
            &b.after_p26,
            &altered_p27
        )
        .is_err()
    );
}
