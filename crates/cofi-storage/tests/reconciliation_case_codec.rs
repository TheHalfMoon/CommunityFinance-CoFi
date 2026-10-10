#![allow(clippy::unwrap_used)]

use cofi_disbursements::{DisbursementId, ProviderRequestReference};
use cofi_reconciliation::{ReconciliationCase, ReconciliationCaseId};
use cofi_storage::reconciliation_case::{decode_reconciliation_case, encode_reconciliation_case};

fn original(timestamp: i64) -> ReconciliationCase {
    ReconciliationCase::new(
        ReconciliationCaseId::new("case-a").unwrap(),
        DisbursementId::new("disbursement-1").unwrap(),
        ProviderRequestReference::new("provider-request-1").unwrap(),
        timestamp,
    )
}

#[test]
fn roundtrip_exact_original_case_ids_provider_binding_and_i64_boundaries() {
    for timestamp in [i64::MIN, -1, 0, 1, i64::MAX] {
        let case = original(timestamp);
        let bytes = encode_reconciliation_case(&case).unwrap();
        let decoded = decode_reconciliation_case(&bytes).unwrap();
        assert_eq!(case, decoded);
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["record_type"], "reconciliation.case");
        assert_eq!(json["payload"]["id"], "case-a");
        assert_eq!(json["payload"]["disbursement_id"], "disbursement-1");
        assert_eq!(
            json["payload"]["provider_request_reference"],
            "provider-request-1"
        );
        assert_eq!(
            json["payload"]["reconciled_at_unix_ms"],
            timestamp.to_string()
        );
        assert_eq!(encode_reconciliation_case(&decoded).unwrap(), bytes);
    }
}

#[test]
fn rejects_wrong_version_kind_schema_and_extra_or_duplicate_fields() {
    let bytes = encode_reconciliation_case(&original(123)).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (ptr, val) in [
        ("/schema_version", serde_json::json!(2)),
        ("/schema_version", serde_json::json!("1")),
        ("/record_type", serde_json::json!("provider.observation")),
        ("/payload/id", serde_json::json!(" ")),
        ("/payload/disbursement_id", serde_json::json!("")),
        ("/payload/provider_request_reference", serde_json::json!("")),
        ("/payload/reconciled_at_unix_ms", serde_json::json!("+123")),
        ("/payload/reconciled_at_unix_ms", serde_json::json!("0123")),
        ("/payload/reconciled_at_unix_ms", serde_json::json!("1.0")),
        ("/payload/reconciled_at_unix_ms", serde_json::json!(123)),
        (
            "/payload/reconciled_at_unix_ms",
            serde_json::json!("9223372036854775808"),
        ),
        (
            "/payload/reconciled_at_unix_ms",
            serde_json::json!("-9223372036854775809"),
        ),
        ("/payload/reconciled_at_unix_ms", serde_json::json!("-0")),
    ] {
        let mut altered = json.clone();
        *altered.pointer_mut(ptr).unwrap() = val.clone();
        assert!(
            decode_reconciliation_case(&serde_json::to_vec(&altered).unwrap()).is_err(),
            "must reject {ptr}={val}"
        );
    }
    let mut extra = json.clone();
    extra["payload"]["provider_outcome"] = serde_json::json!("settled");
    assert!(decode_reconciliation_case(&serde_json::to_vec(&extra).unwrap()).is_err());
    let mut extra_outer = json;
    extra_outer["external_signature"] = serde_json::json!("self-signed");
    assert!(decode_reconciliation_case(&serde_json::to_vec(&extra_outer).unwrap()).is_err());
    let duplicate = br#"{"schema_version":1,"schema_version":1,"record_type":"reconciliation.case","payload":{"id":"case-a","disbursement_id":"disbursement-1","provider_request_reference":"provider-request-1","reconciled_at_unix_ms":"1"}}"#;
    assert!(decode_reconciliation_case(duplicate).is_err());
    let duplicate_field = br#"{"schema_version":1,"record_type":"reconciliation.case","payload":{"id":"case-a","id":"case-b","disbursement_id":"disbursement-1","provider_request_reference":"provider-request-1","reconciled_at_unix_ms":"1"}}"#;
    assert!(decode_reconciliation_case(duplicate_field).is_err());
    assert!(decode_reconciliation_case(&vec![b' '; 1024 * 1024 + 1]).is_err());
}

#[test]
fn immutable_case_identity_is_not_an_accepted_provider_observation_or_outcome() {
    let first = original(100);
    let checked = decode_reconciliation_case(&encode_reconciliation_case(&first).unwrap()).unwrap();
    assert_eq!(checked, first);
    let changed_time = original(200);
    assert_ne!(checked, changed_time);
    // The case snapshot is a request/context object. The record alone must
    // never claim an accepted provider observation, canonical disbursement
    // status transition, reconciliation outcome or authenticated source root.
    let json: serde_json::Value =
        serde_json::from_slice(&encode_reconciliation_case(&checked).unwrap()).unwrap();
    assert!(json.get("provider_observation").is_none());
    assert!(json["payload"].get("provider_observation").is_none());
    assert!(json["payload"].get("outcome").is_none());
}
