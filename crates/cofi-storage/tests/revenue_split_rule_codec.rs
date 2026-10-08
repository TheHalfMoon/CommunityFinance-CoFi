#![allow(clippy::unwrap_used)]
use cofi_community::{BasisPoints, FundId, RevenueSplitLeg, RevenueSplitRule, RevenueSplitRuleId};
use cofi_storage::revenue_split_rule::{decode_revenue_split_rule, encode_revenue_split_rule};

fn rule() -> RevenueSplitRule {
    RevenueSplitRule::new(
        RevenueSplitRuleId::new("rule-v7").unwrap(),
        7,
        vec![
            RevenueSplitLeg::new(
                FundId::new("fund-z").unwrap(),
                BasisPoints::new(2500).unwrap(),
            ),
            RevenueSplitLeg::new(
                FundId::new("fund-a").unwrap(),
                BasisPoints::new(7500).unwrap(),
            ),
        ],
    )
    .unwrap()
}
#[test]
fn immutable_rule_version_and_canonical_sorted_legs_roundtrip() {
    let reference = rule();
    assert_eq!(reference.legs()[0].destination_fund_id().as_str(), "fund-a");
    let encoded = encode_revenue_split_rule(&reference).unwrap();
    assert_eq!(decode_revenue_split_rule(&encoded).unwrap(), reference);
    let reserialized =
        encode_revenue_split_rule(&decode_revenue_split_rule(&encoded).unwrap()).unwrap();
    assert_eq!(reserialized, encoded);
}

#[test]
fn invalid_basis_points_rule_version_and_duplicate_destinations_fail_closed() {
    let original: serde_json::Value =
        serde_json::from_slice(&encode_revenue_split_rule(&rule()).unwrap()).unwrap();
    for (path, replacement) in [
        ("/payload/legs/0/basis_points", serde_json::json!(0)),
        ("/payload/legs/0/basis_points", serde_json::json!(10001)),
        ("/payload/legs/0/basis_points", serde_json::json!(2000)),
        ("/payload/legs/0/basis_points", serde_json::json!(2000.5)),
        ("/payload/version", serde_json::json!(0)),
        ("/payload/id", serde_json::json!(" ")),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(path).unwrap() = replacement;
        assert!(
            decode_revenue_split_rule(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{path}"
        );
    }
    let mut duplicate = original.clone();
    duplicate["payload"]["legs"][1]["destination_fund_id"] = serde_json::json!("fund-a");
    assert!(decode_revenue_split_rule(&serde_json::to_vec(&duplicate).unwrap()).is_err());
    let mut too_few = original;
    too_few["payload"]["legs"] =
        serde_json::json!([{"destination_fund_id":"fund-a","basis_points":10000}]);
    assert!(decode_revenue_split_rule(&serde_json::to_vec(&too_few).unwrap()).is_err());
}

#[test]
fn unknown_schema_and_extra_or_oversize_fields_fail_closed() {
    let source = encode_revenue_split_rule(&rule()).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&source).unwrap();
    for (path, replacement) in [
        ("/schema_version", serde_json::json!(2)),
        ("/record_type", serde_json::json!("unknown.rule")),
        ("/payload/legs/0/destination_fund_id", serde_json::json!("")),
    ] {
        let mut edited = original.clone();
        *edited.pointer_mut(path).unwrap() = replacement;
        assert!(decode_revenue_split_rule(&serde_json::to_vec(&edited).unwrap()).is_err());
    }
    let mut modified = original.clone();
    modified["payload"]["extra"] = serde_json::json!(true);
    assert!(decode_revenue_split_rule(&serde_json::to_vec(&modified).unwrap()).is_err());
    modified = original;
    modified["payload"]["legs"][0]["unexpected"] = serde_json::json!(true);
    assert!(decode_revenue_split_rule(&serde_json::to_vec(&modified).unwrap()).is_err());
    assert!(decode_revenue_split_rule(&vec![b' '; 1024 * 1024 + 1]).is_err());
    let text = String::from_utf8(source).unwrap();
    let duplicate = text.replacen(
        r#""schema_version":1"#,
        r#""schema_version":1,"schema_version":1"#,
        1,
    );
    assert_ne!(duplicate, text);
    assert!(decode_revenue_split_rule(duplicate.as_bytes()).is_err());
}
