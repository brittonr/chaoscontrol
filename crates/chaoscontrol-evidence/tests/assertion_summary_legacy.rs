use chaoscontrol_evidence::{validate_assertion_summary, validate_assertion_summary_for_promotion};
use chaoscontrol_protocol::assertion_catalog::MAX_ASSERTION_REPORT_ENTRIES;
use serde_json::{json, Value};

const COMPATIBILITY_ID: u32 = 7;

fn legacy_assertion() -> Value {
    json!({
        "id": COMPATIBILITY_ID,
        "message": "legacy",
        "kind": "always",
        "guest": "legacy-guest",
        "category": "uncategorized",
        "verdict": "passed",
        "hit_count": 1,
        "true_count": 1,
        "false_count": 0
    })
}

#[test]
fn legacy_summary_is_readable_but_non_promoting() {
    let summary = Value::Array(vec![legacy_assertion()]);
    validate_assertion_summary(&summary).expect("legacy compatibility parsing");
    assert!(validate_assertion_summary_for_promotion(&summary).is_err());
}

#[test]
fn duplicate_legacy_ids_and_unbounded_metadata_are_rejected() {
    let legacy = legacy_assertion();
    assert!(validate_assertion_summary(&json!([legacy.clone(), legacy])).is_err());
    let too_long = json!([{
        "id": COMPATIBILITY_ID,
        "message": "x".repeat(
            chaoscontrol_protocol::assertion_identity::MAX_ASSERTION_MESSAGE_BYTES + 1
        ),
        "kind": "always",
        "guest": "guest",
        "category": "uncategorized",
        "verdict": "unexercised",
        "hit_count": 0,
        "true_count": 0,
        "false_count": 0
    }]);
    assert!(validate_assertion_summary(&too_long).is_err());
}

#[test]
fn legacy_ascii_control_is_rejected() {
    let mut legacy = legacy_assertion();
    legacy["message"] = json!("legacy\nforgery");

    assert!(validate_assertion_summary(&json!([legacy])).is_err());
}

#[test]
fn report_cardinality_overflow_is_rejected() {
    let items = std::iter::repeat_n(
        json!({
            "id": COMPATIBILITY_ID,
            "message": "legacy",
            "kind": "always",
            "guest": "legacy-guest",
            "category": "uncategorized",
            "verdict": "unexercised",
            "hit_count": 0,
            "true_count": 0,
            "false_count": 0
        }),
        MAX_ASSERTION_REPORT_ENTRIES + 1,
    )
    .collect();
    let error = validate_assertion_summary(&Value::Array(items)).expect_err("cardinality limit");
    assert!(error.message().contains("entry count exceeds"));
}
