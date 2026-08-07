use chaoscontrol_explore::assertion_summary::AssertionSummaryV2;
use serde_json::{json, Value};

const COMPATIBILITY_ALIAS: u32 = 17;

fn legacy_summary() -> Value {
    json!({
        "schema": "chaoscontrol.assertion-summary.v2",
        "catalog_status": "legacy-ambiguous",
        "collision_safe_evidence": false,
        "assertions": [{
            "id": COMPATIBILITY_ALIAS,
            "message": "historical assertion",
            "kind": "always",
            "guest": "historical-guest",
            "category": "uncategorized",
            "verdict": "unexercised",
            "hit_count": 0,
            "true_count": 0,
            "false_count": 0
        }]
    })
}

#[test]
fn valid_legacy_summary_deserializes_without_promotion() {
    let summary: AssertionSummaryV2 =
        serde_json::from_value(legacy_summary()).expect("valid legacy summary");

    assert_eq!(summary.schema(), "chaoscontrol.assertion-summary.v2");
    assert!(!summary.collision_safe_evidence());
    assert_eq!(summary.assertions().len(), 1);
}

#[test]
fn forged_accepted_flag_is_rejected() {
    let mut value = legacy_summary();
    value["catalog_status"] = json!("accepted");
    value["collision_safe_evidence"] = json!(true);

    assert!(serde_json::from_value::<AssertionSummaryV2>(value).is_err());
}

#[test]
fn pending_summary_is_rejected_instead_of_normalized() {
    let mut value = legacy_summary();
    value["catalog_status"] = json!("pending");

    assert!(serde_json::from_value::<AssertionSummaryV2>(value).is_err());
}

#[test]
fn present_null_identity_is_rejected() {
    let mut value = legacy_summary();
    value["assertions"][0]["identity"] = Value::Null;

    assert!(serde_json::from_value::<AssertionSummaryV2>(value).is_err());
}

#[test]
fn legacy_ascii_control_is_rejected() {
    let mut value = legacy_summary();
    value["assertions"][0]["message"] = json!("legacy\nforgery");

    assert!(serde_json::from_value::<AssertionSummaryV2>(value).is_err());
}

#[test]
fn missing_guest_or_category_is_rejected() {
    for field in ["guest", "category"] {
        let mut value = legacy_summary();
        value["assertions"][0]
            .as_object_mut()
            .expect("assertion object")
            .remove(field);
        assert!(serde_json::from_value::<AssertionSummaryV2>(value).is_err());
    }
}

#[test]
fn unknown_summary_field_is_rejected() {
    let mut value = legacy_summary();
    value["unexpected"] = json!(true);

    assert!(serde_json::from_value::<AssertionSummaryV2>(value).is_err());
}
