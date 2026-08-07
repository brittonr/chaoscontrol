use chaoscontrol_evidence::summarize_sdk_local_jsonl;
use chaoscontrol_protocol::identity::{
    ASSERTION_FINGERPRINT_HEX_BYTES, MAX_ASSERTION_CATEGORY_BYTES, MAX_ASSERTION_GUEST_BYTES,
    MAX_ASSERTION_MESSAGE_BYTES,
};

const EVIDENCE_CLASS: &str = "instrumentation-dry-run";
const NON_STRING_CATEGORY_JSON: &str = "7";

fn assertion(message: &str, assert_type: &str, guest: &str, category: &str) -> String {
    assertion_with_id("1", message, assert_type, guest, category)
}

fn assertion_with_id(
    id: &str,
    message: &str,
    assert_type: &str,
    guest: &str,
    category: &str,
) -> String {
    format!(
        "{{\"antithesis_assert\":{{\"assert_type\":\"{assert_type}\",\"condition\":true,\"hit\":true,\"id\":\"{id}\",\"message\":\"{message}\",\"details\":{{\"guest\":{guest},\"category\":{category}}}}}}}\n"
    )
}

#[test]
fn legacy_diagnostic_id_is_bounded_lowercase_hexadecimal() {
    for invalid_id in [
        String::new(),
        "a".repeat(ASSERTION_FINGERPRINT_HEX_BYTES + 1),
        "ABCDEF".to_string(),
        "0g".to_string(),
    ] {
        let event = assertion_with_id(&invalid_id, "base", "always", "\"guest\"", "\"invariant\"");
        assert!(summarize_sdk_local_jsonl(&event, EVIDENCE_CLASS, None).is_err());
    }
}

#[test]
fn legacy_identity_metadata_accepts_only_known_bounded_strings() {
    let valid = assertion("base", "always", "\"guest\"", "\"invariant\"");
    summarize_sdk_local_jsonl(&valid, EVIDENCE_CLASS, None).expect("valid legacy diagnostic");

    for invalid in [
        assertion("base", "unknown", "\"guest\"", "\"invariant\""),
        assertion("", "always", "\"guest\"", "\"invariant\""),
        assertion("base", "always", "\"\"", "\"invariant\""),
        assertion("base", "always", "\"guest\"", NON_STRING_CATEGORY_JSON),
        assertion(
            &"x".repeat(MAX_ASSERTION_MESSAGE_BYTES + 1),
            "always",
            "\"guest\"",
            "\"invariant\"",
        ),
        assertion(
            "base",
            "always",
            &format!("\"{}\"", "x".repeat(MAX_ASSERTION_GUEST_BYTES + 1)),
            "\"invariant\"",
        ),
        assertion(
            "base",
            "always",
            "\"guest\"",
            &format!("\"{}\"", "x".repeat(MAX_ASSERTION_CATEGORY_BYTES + 1)),
        ),
    ] {
        assert!(summarize_sdk_local_jsonl(&invalid, EVIDENCE_CLASS, None).is_err());
    }
}
