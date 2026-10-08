#![allow(clippy::unwrap_used)]
include!("support/payment_stream_fixture.rs");
use cofi_storage::causal_capture_payout_stream::{
    CausalPaymentFlow, rebuild_causal_capture_payout_stream,
};

#[test]
fn two_real_original_authorization_registries_rebuilt_across_two_tenants() {
    let (receipts, billing, genesis, reference) = input();
    let flows = [
        CausalPaymentFlow {
            sequence: 1,
            p25_receipt: &receipts[0],
            billing_accounts: &billing[0],
        },
        CausalPaymentFlow {
            sequence: 2,
            p25_receipt: &receipts[1],
            billing_accounts: &billing[1],
        },
    ];
    let restored = rebuild_causal_capture_payout_stream(&flows, &genesis, &reference).unwrap();
    assert_eq!(restored.captures().authorization_count(), 2);
    assert_eq!(restored.payouts().authorization_count(), 2);
    assert_eq!(restored.ledger().entry_count(), 6);
    assert_eq!(restored.flow_count(), 2);
    assert_eq!(genesis.entry_count(), 0);
}
#[test]
fn sequence_duplicates_and_incomplete_original_reference_fail_closed() {
    let (receipts, billing, genesis, reference) = input();
    for (a, b) in [(2, 3), (1, 1), (2, 1), (1, 3)] {
        let flows = [
            CausalPaymentFlow {
                sequence: a,
                p25_receipt: &receipts[0],
                billing_accounts: &billing[0],
            },
            CausalPaymentFlow {
                sequence: b,
                p25_receipt: &receipts[1],
                billing_accounts: &billing[1],
            },
        ];
        assert!(rebuild_causal_capture_payout_stream(&flows, &genesis, &reference).is_err());
    }
    let duplicate = [
        CausalPaymentFlow {
            sequence: 1,
            p25_receipt: &receipts[0],
            billing_accounts: &billing[0],
        },
        CausalPaymentFlow {
            sequence: 2,
            p25_receipt: &receipts[0],
            billing_accounts: &billing[0],
        },
    ];
    assert!(rebuild_causal_capture_payout_stream(&duplicate, &genesis, &reference).is_err());
    let incomplete = [CausalPaymentFlow {
        sequence: 1,
        p25_receipt: &receipts[0],
        billing_accounts: &billing[0],
    }];
    assert!(rebuild_causal_capture_payout_stream(&incomplete, &genesis, &reference).is_err());
    let all = [
        CausalPaymentFlow {
            sequence: 1,
            p25_receipt: &receipts[0],
            billing_accounts: &billing[0],
        },
        CausalPaymentFlow {
            sequence: 2,
            p25_receipt: &receipts[1],
            billing_accounts: &billing[1],
        },
    ];
    assert!(rebuild_causal_capture_payout_stream(&all, &reference, &reference).is_err());
}

#[test]
fn altered_capture_payout_source_and_cross_tenant_account_binding_fail_closed() {
    let (receipts, billing, genesis, reference) = input();
    for (path, value) in [
        ("/payload/processor_fee_minor", serde_json::json!("3")),
        ("/payload/paid_at_unix_ms", serde_json::json!("1")),
        (
            "/payload/p24_capture/payload/payment_id",
            serde_json::json!("forged"),
        ),
        ("/payload/organization_scope", serde_json::json!("org-1")),
        (
            "/payload/bank_cash_account_id",
            serde_json::json!("bank-cash"),
        ),
    ] {
        let mut altered: Value = serde_json::from_slice(&receipts[1]).unwrap();
        *altered.pointer_mut(path).unwrap() = value;
        let broken = serde_json::to_vec(&altered).unwrap();
        let flows = [
            CausalPaymentFlow {
                sequence: 1,
                p25_receipt: &receipts[0],
                billing_accounts: &billing[0],
            },
            CausalPaymentFlow {
                sequence: 2,
                p25_receipt: &broken,
                billing_accounts: &billing[1],
            },
        ];
        assert!(
            rebuild_causal_capture_payout_stream(&flows, &genesis, &reference).is_err(),
            "{path}"
        );
    }
    let flows = [
        CausalPaymentFlow {
            sequence: 1,
            p25_receipt: &receipts[0],
            billing_accounts: &billing[1],
        },
        CausalPaymentFlow {
            sequence: 2,
            p25_receipt: &receipts[1],
            billing_accounts: &billing[0],
        },
    ];
    assert!(rebuild_causal_capture_payout_stream(&flows, &genesis, &reference).is_err());
}

#[test]
fn rejects_unlisted_reference_account_even_with_matching_financial_journals() {
    let (receipts, billing, genesis, reference) = input();
    let mut augmented_reference = reference.clone();
    augmented_reference
        .register_account(Account::new(
            AccountId::new("unlisted-reference-account").unwrap(),
            LedgerScopeId::new("unknown-scope").unwrap(),
            AccountKind::Asset,
            Currency::new("SAR").unwrap(),
        ))
        .unwrap();
    let flows = [
        CausalPaymentFlow {
            sequence: 1,
            p25_receipt: &receipts[0],
            billing_accounts: &billing[0],
        },
        CausalPaymentFlow {
            sequence: 2,
            p25_receipt: &receipts[1],
            billing_accounts: &billing[1],
        },
    ];
    assert!(rebuild_causal_capture_payout_stream(&flows, &genesis, &augmented_reference).is_err());
}
