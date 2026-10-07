#![allow(clippy::unwrap_used)]

use cofi_ledger::{
    Account, AccountId, AccountKind, CommitOutcome, Currency, EntryMetadata, JournalEntry,
    JournalEntryId, Ledger, LedgerScopeId, Posting, Side,
};
use cofi_storage::{
    CodecError, LedgerFact, decode_fact, encode_account, encode_entry, parse_i128_exact,
    parse_i64_exact, parse_u128_exact, replay_ledger,
};

fn account(id: &str, kind: AccountKind) -> Account {
    Account::new(
        AccountId::new(id).unwrap(),
        LedgerScopeId::new("org-sandbox").unwrap(),
        kind,
        Currency::new("SAR").unwrap(),
    )
}

fn entry(id: &str, key: &str, amount: i128) -> JournalEntry {
    JournalEntry::new(
        JournalEntryId::new(id).unwrap(),
        vec![
            Posting::new(
                AccountId::new("receivable").unwrap(),
                Currency::new("SAR").unwrap(),
                Side::Debit,
                amount,
            )
            .unwrap(),
            Posting::new(
                AccountId::new("revenue").unwrap(),
                Currency::new("SAR").unwrap(),
                Side::Credit,
                amount,
            )
            .unwrap(),
        ],
        -3,
        i64::MAX,
        EntryMetadata::new(Some("corr-001".to_owned()), Some(key.to_owned()))
            .unwrap()
            .with_business_key(Some("business-001".to_owned()))
            .unwrap(),
    )
    .unwrap()
}

fn accounts_and_entry() -> (Account, Account, JournalEntry) {
    (
        account("receivable", AccountKind::Asset),
        account("revenue", AccountKind::Revenue),
        entry("entry-001", "idem-001", i128::MAX),
    )
}

#[test]
fn account_and_journal_roundtrip_through_checked_domain_ctors() {
    let (receivable, revenue, original) = accounts_and_entry();

    match decode_fact(&encode_account(&receivable).unwrap()).unwrap() {
        LedgerFact::Account(decoded) => assert_eq!(decoded, receivable),
        LedgerFact::Entry(_) => panic!("account unexpectedly decoded as entry"),
    }
    match decode_fact(&encode_account(&revenue).unwrap()).unwrap() {
        LedgerFact::Account(decoded) => assert_eq!(decoded, revenue),
        LedgerFact::Entry(_) => panic!("account unexpectedly decoded as entry"),
    }
    match decode_fact(&encode_entry(&original).unwrap()).unwrap() {
        LedgerFact::Entry(decoded) => assert_eq!(decoded, original),
        LedgerFact::Account(_) => panic!("entry unexpectedly decoded as account"),
    }
    assert_eq!(original.postings()[0].account_id().as_str(), "receivable");
    assert_eq!(original.recorded_at_unix_ms(), i64::MAX);
    assert_eq!(original.metadata().business_key(), Some("business-001"));
}

#[test]
fn replay_rebuilds_indexes_balances_and_exact_identity() {
    let (receivable, revenue, original) = accounts_and_entry();
    let facts = vec![
        encode_account(&receivable).unwrap(),
        encode_account(&revenue).unwrap(),
        encode_entry(&original).unwrap(),
    ];

    let mut reference = Ledger::new();
    reference.register_account(receivable).unwrap();
    reference.register_account(revenue).unwrap();
    assert_eq!(reference.commit(original.clone()), Ok(CommitOutcome::Committed));

    let mut recovered = replay_ledger(facts.iter().map(Vec::as_slice)).unwrap();
    assert_eq!(recovered.entry_count(), reference.entry_count());
    assert_eq!(recovered.entry(original.id()), reference.entry(original.id()));
    for id in ["receivable", "revenue"] {
        let account_id = AccountId::new(id).unwrap();
        assert_eq!(recovered.account(&account_id), reference.account(&account_id));
        assert_eq!(recovered.balance(&account_id), reference.balance(&account_id));
    }
    assert_eq!(recovered.commit(original), Ok(CommitOutcome::Replayed));
}

#[test]
fn replay_rejects_duplicate_changed_identity() {
    let (receivable, revenue, original) = accounts_and_entry();
    let other = entry("entry-002", "idem-001", 1);
    let facts = vec![
        encode_account(&receivable).unwrap(),
        encode_account(&revenue).unwrap(),
        encode_entry(&original).unwrap(),
        encode_entry(&other).unwrap(),
    ];
    assert!(matches!(
        replay_ledger(facts.iter().map(Vec::as_slice)),
        Err(CodecError::Replay(_))
    ));
}

#[test]
fn replay_rejects_missing_account_before_entry() {
    let (_, _, original) = accounts_and_entry();
    let encoded = encode_entry(&original).unwrap();
    assert!(matches!(
        replay_ledger([encoded.as_slice()]),
        Err(CodecError::Replay(_))
    ));
}

#[test]
fn exact_integer_boundaries_and_bad_representations() {
    assert_eq!(parse_u128_exact(&u128::MAX.to_string()), Ok(u128::MAX));
    assert_eq!(parse_i128_exact(&i128::MIN.to_string()), Ok(i128::MIN));
    assert_eq!(parse_i128_exact(&i128::MAX.to_string()), Ok(i128::MAX));
    assert_eq!(parse_i64_exact(&i64::MIN.to_string()), Ok(i64::MIN));
    for bad in ["", " 1", "1 ", "+1", "01", "-0", "1.0", "1e3", "NaN", "1_000"] {
        assert!(parse_i128_exact(bad).is_err(), "{bad:?}");
        assert!(parse_u128_exact(bad).is_err(), "{bad:?}");
    }
    assert!(parse_u128_exact("340282366920938463463374607431768211456").is_err());
    assert!(parse_i128_exact("170141183460469231731687303715884105728").is_err());
    assert!(parse_i64_exact("9223372036854775808").is_err());
}

#[test]
fn unknown_type_version_and_extra_fields_fail_closed() {
    let (account, _, _) = accounts_and_entry();
    let encoded = encode_account(&account).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();

    value["schema_version"] = serde_json::json!(2);
    assert!(matches!(
        decode_fact(&serde_json::to_vec(&value).unwrap()),
        Err(CodecError::UnsupportedVersion(2))
    ));

    value["schema_version"] = serde_json::json!(1);
    value["record_type"] = serde_json::json!("ledger.unknown");
    assert!(matches!(
        decode_fact(&serde_json::to_vec(&value).unwrap()),
        Err(CodecError::UnsupportedRecordKind(_))
    ));

    value["record_type"] = serde_json::json!("ledger.account");
    value["payload"]["unchecked_amount"] = serde_json::json!(1);
    assert!(matches!(
        decode_fact(&serde_json::to_vec(&value).unwrap()),
        Err(CodecError::InvalidPayload(_))
    ));
}

#[test]
fn invalid_domain_facts_and_fractional_json_numbers_fail_closed() {
    let (_, _, original) = accounts_and_entry();
    let mut value: serde_json::Value =
        serde_json::from_slice(&encode_entry(&original).unwrap()).unwrap();

    value["payload"]["postings"][0]["amount_minor"] = serde_json::json!("1.5");
    assert!(matches!(
        decode_fact(&serde_json::to_vec(&value).unwrap()),
        Err(CodecError::InvalidInteger(_))
    ));

    value["payload"]["postings"][0]["amount_minor"] = serde_json::json!(1.5);
    assert!(matches!(
        decode_fact(&serde_json::to_vec(&value).unwrap()),
        Err(CodecError::InvalidPayload(_))
    ));

    value["payload"]["postings"][0]["amount_minor"] = serde_json::json!("-1");
    assert!(matches!(
        decode_fact(&serde_json::to_vec(&value).unwrap()),
        Err(CodecError::InvalidDomain(_))
    ));
}
