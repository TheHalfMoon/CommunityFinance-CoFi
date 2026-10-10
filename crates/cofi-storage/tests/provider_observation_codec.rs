#![allow(clippy::unwrap_used)]

use cofi_disbursements::{
    DisbursementEventId, DisbursementId, FailureCode, ProviderEventReference,
    ProviderRequestReference, ProviderSettlementReference,
};
use cofi_provider_contract::ProviderObservation;
use cofi_reconciliation::{ReconciliationCase, ReconciliationCaseId};
use cofi_storage::provider_observation::{
    check_untrusted_case_observation_pair, decode_provider_observation, encode_provider_observation,
};
use cofi_storage::reconciliation_case::encode_reconciliation_case;

fn d() -> DisbursementId {
    DisbursementId::new("d-1").unwrap()
}
fn r() -> ProviderRequestReference {
    ProviderRequestReference::new("req-1").unwrap()
}
fn e() -> ProviderEventReference {
    ProviderEventReference::new("event-1").unwrap()
}
fn case(at: i64) -> ReconciliationCase {
    ReconciliationCase::new(ReconciliationCaseId::new("case-1").unwrap(), d(), r(), at)
}
fn accepted(at: i64) -> ProviderObservation {
    ProviderObservation::accepted(d(), r(), e(), at)
}
fn settled(at: i64) -> ProviderObservation {
    ProviderObservation::settled(
        d(),
        r(),
        DisbursementEventId::new("terminal-1").unwrap(),
        e(),
        ProviderSettlementReference::new("settle-1").unwrap(),
        at,
    )
}
fn failed(at: i64) -> ProviderObservation {
    ProviderObservation::failed(
        d(),
        r(),
        DisbursementEventId::new("terminal-2").unwrap(),
        e(),
        FailureCode::new("declined").unwrap(),
        at,
    )
}

#[test]
fn checked_original_observation_roundtrip_all_three_kinds_and_i64_limits() {
    for kind in [accepted as fn(i64) -> ProviderObservation, settled, failed] {
        for at in [i64::MIN, -1, 0, 1, i64::MAX] {
            let original = kind(at);
            let bytes = encode_provider_observation(&original).unwrap();
            let checked = decode_provider_observation(&bytes).unwrap();
            assert_eq!(original, checked);
            assert_eq!(bytes, encode_provider_observation(&checked).unwrap());
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(json["schema_version"], 1);
            assert_eq!(json["record_type"], "provider.observation");
            assert_eq!(json["payload"]["disbursement_id"], "d-1");
            assert_eq!(json["payload"]["provider_request_reference"], "req-1");
            assert_eq!(json["payload"]["occurred_at_unix_ms"], at.to_string());
        }
    }
}

#[test]
fn strict_schema_disallows_kind_confusion_broken_ids_and_imprecise_timestamp() {
    let bytes = encode_provider_observation(&settled(100)).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (ptr, value) in [
        ("/schema_version", serde_json::json!(2)),
        ("/schema_version", serde_json::json!("1")),
        ("/record_type", serde_json::json!("reconciliation.case")),
        ("/payload/disbursement_id", serde_json::json!("")),
        (
            "/payload/provider_request_reference",
            serde_json::json!(" "),
        ),
        ("/payload/occurred_at_unix_ms", serde_json::json!(100)),
        ("/payload/occurred_at_unix_ms", serde_json::json!("+100")),
        ("/payload/occurred_at_unix_ms", serde_json::json!("0100")),
        ("/payload/occurred_at_unix_ms", serde_json::json!("-0")),
        ("/payload/occurred_at_unix_ms", serde_json::json!("1e2")),
        ("/payload/occurred_at_unix_ms", serde_json::json!("1.0")),
        (
            "/payload/occurred_at_unix_ms",
            serde_json::json!("9223372036854775808"),
        ),
        (
            "/payload/occurred_at_unix_ms",
            serde_json::json!("-9223372036854775809"),
        ),
        ("/payload/kind/kind", serde_json::json!("accepted")),
        ("/payload/kind/kind", serde_json::json!("unknown")),
        (
            "/payload/kind/provider_event_reference",
            serde_json::json!(""),
        ),
        ("/payload/kind/lifecycle_event_id", serde_json::json!("")),
        ("/payload/kind/settlement_reference", serde_json::json!("")),
    ] {
        let mut altered = original.clone();
        *altered.pointer_mut(ptr).unwrap() = value.clone();
        assert!(
            decode_provider_observation(&serde_json::to_vec(&altered).unwrap()).is_err(),
            "must reject {ptr}={value}"
        );
    }
    let mut extra = original.clone();
    extra["payload"]["kind"]["failure_code"] = serde_json::json!("unrequested");
    assert!(decode_provider_observation(&serde_json::to_vec(&extra).unwrap()).is_err());
    let mut extra = original.clone();
    extra["payload"]["provider_signature"] = serde_json::json!("unverified");
    assert!(decode_provider_observation(&serde_json::to_vec(&extra).unwrap()).is_err());
    let mut extra = original;
    extra["provider_receipt_authenticated"] = serde_json::json!(true);
    assert!(decode_provider_observation(&serde_json::to_vec(&extra).unwrap()).is_err());
    let duplicate = br#"{"schema_version":1,"schema_version":1,"record_type":"provider.observation","payload":{}}"#;
    assert!(decode_provider_observation(duplicate).is_err());
    assert!(decode_provider_observation(&vec![b' '; 1024 * 1024 + 1]).is_err());
}

#[test]
fn original_accepted_and_failed_variants_have_no_fake_terminal_settlement() {
    for original in [accepted(120), failed(125)] {
        let bytes = encode_provider_observation(&original).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            json["payload"]["kind"]
                .get("settlement_reference")
                .is_none()
        );
        assert!(json.get("accepted_provider_signature").is_none());
        assert!(json["payload"].get("reconciliation_outcome").is_none());
    }
    let accepted_bytes = encode_provider_observation(&accepted(120)).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&accepted_bytes).unwrap();
    json["payload"]["kind"]["lifecycle_event_id"] = serde_json::json!("forged-terminal");
    assert!(decode_provider_observation(&serde_json::to_vec(&json).unwrap()).is_err());
}

#[test]
fn untrusted_case_observation_binding_rejects_foreign_provider_or_future_observation() {
    let c = encode_reconciliation_case(&case(130)).unwrap();
    for original in [accepted(120), settled(120), failed(120)] {
        let bytes = encode_provider_observation(&original).unwrap();
        let (case_checked, observation_checked) =
            check_untrusted_case_observation_pair(&c, &bytes).unwrap();
        assert_eq!(case_checked, case(130));
        assert_eq!(observation_checked, original);
    }
    assert!(
        check_untrusted_case_observation_pair(
            &c,
            &encode_provider_observation(&accepted(131)).unwrap()
        )
        .is_err()
    );
    let foreign =
        ProviderObservation::accepted(DisbursementId::new("d-other").unwrap(), r(), e(), 120);
    assert!(
        check_untrusted_case_observation_pair(&c, &encode_provider_observation(&foreign).unwrap())
            .is_err()
    );
    let foreign = ProviderObservation::accepted(
        d(),
        ProviderRequestReference::new("req-other").unwrap(),
        e(),
        120,
    );
    assert!(
        check_untrusted_case_observation_pair(&c, &encode_provider_observation(&foreign).unwrap())
            .is_err()
    );
}
