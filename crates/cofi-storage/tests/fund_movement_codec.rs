#![allow(clippy::unwrap_used)]

use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundAllocationAccounts, FundAllocationBridge,
    FundAllocationEvent, FundAllocationEventId, FundAllocationId, FundId, FundTransferBridge,
    FundTransferEvent, FundTransferEventId, FundTransferId, Organization, OrganizationId,
};
use cofi_ledger::{
    Account, AccountId, AccountKind, Currency, EntryMetadata, JournalEntry, JournalEntryId, Ledger,
    LedgerScopeId, Posting, Side,
};
use cofi_storage::fund_movement::{
    FundMovementFact, decode_fund_movement, encode_fund_movement, verify_fund_movement_history,
    verify_fund_movement_journal,
};

fn id(s: &str) -> AccountId {
    AccountId::new(s).unwrap()
}
fn scope() -> LedgerScopeId {
    LedgerScopeId::new("org-a").unwrap()
}
fn sar() -> Currency {
    Currency::new("SAR").unwrap()
}
fn fid(s: &str) -> FundId {
    FundId::new(s).unwrap()
}

fn fixture() -> (
    CommunityRegistry,
    Ledger,
    Ledger,
    FundMovementFact,
    FundMovementFact,
) {
    let mut ledger = Ledger::new();
    for (name, kind) in [
        ("bank", AccountKind::Asset),
        ("equity", AccountKind::Equity),
        ("fund-src-account", AccountKind::Asset),
        ("fund-dst-account", AccountKind::Asset),
    ] {
        ledger
            .register_account(Account::new(id(name), scope(), kind, sar()))
            .unwrap();
    }
    let mut registry = CommunityRegistry::new();
    registry
        .register_organization(Organization::new(OrganizationId::new("org-a").unwrap()))
        .unwrap();
    for (community, fund, account) in [
        ("community-a", "fund-a", "fund-src-account"),
        ("community-b", "fund-b", "fund-dst-account"),
    ] {
        registry
            .register_community(Community::new(
                CommunityId::new(community).unwrap(),
                OrganizationId::new("org-a").unwrap(),
            ))
            .unwrap();
        registry
            .register_fund(
                Fund::new(
                    fid(fund),
                    CommunityId::new(community).unwrap(),
                    id(account),
                    sar(),
                ),
                &ledger,
            )
            .unwrap();
    }
    let seed = JournalEntry::new(
        JournalEntryId::new("seed-1").unwrap(),
        vec![
            Posting::new(id("bank"), sar(), Side::Debit, 1000).unwrap(),
            Posting::new(id("equity"), sar(), Side::Credit, 1000).unwrap(),
        ],
        100,
        101,
        EntryMetadata::new(Some("seed-1".into()), Some("seed-1".into())).unwrap(),
    )
    .unwrap();
    ledger.commit(seed).unwrap();
    let allocation = FundAllocationEvent::new(
        FundAllocationEventId::new("allocated-event").unwrap(),
        scope(),
        fid("fund-a"),
        FundAllocationId::new("allocation-1").unwrap(),
        sar(),
        700,
        200,
        201,
    );
    let allocation_fact = FundMovementFact::Allocation {
        event: allocation.clone(),
        source_cash: id("bank"),
    };
    let original_before = ledger.clone();
    FundAllocationBridge::new()
        .apply(
            &registry,
            &allocation,
            &FundAllocationAccounts::new(id("bank")),
            &mut ledger,
        )
        .unwrap();
    let transfer = FundTransferEvent::new(
        FundTransferEventId::new("transfer-event").unwrap(),
        scope(),
        fid("fund-a"),
        fid("fund-b"),
        FundTransferId::new("transfer-1").unwrap(),
        sar(),
        300,
        300,
        301,
    );
    let transfer_fact = FundMovementFact::Transfer(transfer.clone());
    FundTransferBridge::new()
        .apply(&registry, &transfer, &mut ledger)
        .unwrap();
    (
        registry,
        ledger,
        original_before,
        allocation_fact,
        transfer_fact,
    )
}

#[test]
fn accepted_movement_facts_roundtrip_and_verify_exact_existing_journals() {
    let (registry, ledger, prior, allocation, transfer) = fixture();
    let before = ledger.entry_count();
    let items = [allocation.clone(), transfer.clone()];
    let encoded = items
        .iter()
        .map(encode_fund_movement)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for (original, blob) in items.iter().zip(&encoded) {
        assert_eq!(&decode_fund_movement(blob).unwrap(), original);
        assert!(verify_fund_movement_journal(original, &registry, &ledger).is_ok());
    }
    assert_eq!(
        verify_fund_movement_history(encoded.iter().map(Vec::as_slice), &registry, &ledger)
            .unwrap(),
        2
    );
    assert_eq!(
        verify_fund_movement_history(
            encoded.iter().cycle().take(4).map(Vec::as_slice),
            &registry,
            &ledger
        )
        .unwrap(),
        2
    );
    assert_eq!(ledger.entry_count(), before);
    // A missing target entry must NEVER be created by verification.
    assert!(verify_fund_movement_journal(&allocation, &registry, &prior).is_err());
    assert!(verify_fund_movement_journal(&transfer, &registry, &prior).is_err());
    assert_eq!(prior.entry_count(), 1);
}

#[test]
fn changed_source_id_timestamps_amount_and_accounts_fail_against_original_journal() {
    let (registry, ledger, _, allocation, transfer) = fixture();
    for fact in [&allocation, &transfer] {
        let raw = encode_fund_movement(fact).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        for (path, replacement) in [
            ("/payload/source_event_id", serde_json::json!("forged")),
            ("/payload/amount_minor", serde_json::json!("299")),
            ("/payload/effective_at_unix_ms", serde_json::json!("3333")),
            ("/payload/observed_at_unix_ms", serde_json::json!("3334")),
            (
                "/payload/organization_scope",
                serde_json::json!("other-tenant"),
            ),
            ("/payload/currency", serde_json::json!("USD")),
        ] {
            let mut modified = value.clone();
            *modified.pointer_mut(path).unwrap() = replacement;
            let decoded = decode_fund_movement(&serde_json::to_vec(&modified).unwrap()).unwrap();
            assert!(
                verify_fund_movement_journal(&decoded, &registry, &ledger).is_err(),
                "{path}"
            );
        }
    }
    let raw = encode_fund_movement(&allocation).unwrap();
    let mut altered: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    altered["payload"]["source_cash"] = serde_json::json!("fund-dst-account");
    assert!(
        verify_fund_movement_journal(
            &decode_fund_movement(&serde_json::to_vec(&altered).unwrap()).unwrap(),
            &registry,
            &ledger,
        )
        .is_err()
    );
    let raw = encode_fund_movement(&transfer).unwrap();
    let mut altered: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    altered["payload"]["destination_fund_id"] = serde_json::json!("fund-a");
    assert!(
        verify_fund_movement_journal(
            &decode_fund_movement(&serde_json::to_vec(&altered).unwrap()).unwrap(),
            &registry,
            &ledger,
        )
        .is_err()
    );
}

#[test]
fn malformed_or_invalid_facts_fail_closed_without_ledger_mutation() {
    let (registry, ledger, _, allocation, transfer) = fixture();
    let original = encode_fund_movement(&allocation).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&original).unwrap();
    for (path, replacement) in [
        ("/schema_version", serde_json::json!(2)),
        ("/record_type", serde_json::json!("wrong")),
        ("/payload/amount_minor", serde_json::json!("0")),
        ("/payload/amount_minor", serde_json::json!("-1")),
        ("/payload/amount_minor", serde_json::json!("1.5")),
        ("/payload/source_event_id", serde_json::json!(" ")),
        ("/payload/fund_id", serde_json::json!("missing")),
        (
            "/payload/amount_minor",
            serde_json::json!("170141183460469231731687303715884105728"),
        ),
    ] {
        let mut data = value.clone();
        *data.pointer_mut(path).unwrap() = replacement;
        let b = serde_json::to_vec(&data).unwrap();
        if let Ok(decoded) = decode_fund_movement(&b) {
            assert!(
                verify_fund_movement_journal(&decoded, &registry, &ledger).is_err(),
                "{path}"
            );
        }
    }
    let mut extra = value;
    extra["payload"]["unknown"] = serde_json::json!("fake");
    assert!(decode_fund_movement(&serde_json::to_vec(&extra).unwrap()).is_err());
    assert!(decode_fund_movement(&vec![b' '; 1024 * 1024 + 1]).is_err());
    let duplicated = String::from_utf8(original).unwrap().replacen(
        r#""schema_version":1"#,
        r#""schema_version":1,"schema_version":1"#,
        1,
    );
    assert!(decode_fund_movement(duplicated.as_bytes()).is_err());

    let a = encode_fund_movement(&allocation).unwrap();
    let mut changed: serde_json::Value = serde_json::from_slice(&a).unwrap();
    changed["payload"]["amount_minor"] = serde_json::json!("699");
    let different = serde_json::to_vec(&changed).unwrap();
    assert!(
        verify_fund_movement_history([a.as_slice(), different.as_slice()], &registry, &ledger)
            .is_err()
    );
    let t = encode_fund_movement(&transfer).unwrap();
    assert_eq!(
        verify_fund_movement_history([a.as_slice(), t.as_slice()], &registry, &ledger).unwrap(),
        2
    );
}
