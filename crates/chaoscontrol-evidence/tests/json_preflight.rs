use chaoscontrol_evidence::{
    check_sdk_assertion_quality_path, summarize_sdk_local_jsonl, DEFAULT_SDK_LOCAL_EVIDENCE_CLASS,
};
use std::io::Write;
use tempfile::NamedTempFile;

const DEEP_LEVELS: usize = 65;
const TOKEN_ITEMS: usize = 5_000;
const REPORT_TOKENS_PER_ENTRY: usize = 96;
const REPORT_BASE_TOKENS: usize = 1_024;
const REPORT_TOKEN_ITEMS: usize = chaoscontrol_protocol::admission::MAX_ASSERTION_REPORT_ENTRIES
    * REPORT_TOKENS_PER_ENTRY
    + REPORT_BASE_TOKENS
    + 1;
const STRING_BYTES: usize = 13 * 1024;
const REPORT_STRING_BYTES: usize = 8 * 1024 * 1024 + 1;

fn expect_jsonl_error(input: &str, expected: &str) {
    let error = summarize_sdk_local_jsonl(input, DEFAULT_SDK_LOCAL_EVIDENCE_CLASS, None)
        .expect_err("JSONL preflight must reject input");
    assert!(error.message().contains(expected), "{}", error.message());
}

fn expect_report_error(input: &str, expected: &str) {
    let mut file = NamedTempFile::new().expect("create report fixture");
    file.write_all(input.as_bytes())
        .expect("write report fixture");
    let error =
        check_sdk_assertion_quality_path(file.path()).expect_err("report preflight must reject");
    assert!(error.message().contains(expected), "{}", error.message());
}

#[test]
fn preflight_accepts_a_small_valid_jsonl_stream_and_report() {
    let content = "{\"antithesis_setup\":{\"status\":\"complete\",\"details\":{}}}\n{\"antithesis_assert\":{\"assert_type\":\"always\",\"condition\":true,\"hit\":true,\"id\":\"1\",\"message\":\"valid\",\"details\":{\"category\":\"invariant\"}}}\n";
    let report = summarize_sdk_local_jsonl(content, DEFAULT_SDK_LOCAL_EVIDENCE_CLASS, None)
        .expect("valid report");
    let mut file = NamedTempFile::new().expect("create report fixture");
    serde_json::to_writer(&mut file, &report).expect("write report fixture");
    check_sdk_assertion_quality_path(file.path()).expect("valid report path");
}

#[test]
fn preflight_accepts_a_valid_maximum_cardinality_quality_report() {
    let entry_count = chaoscontrol_protocol::admission::MAX_ASSERTION_REPORT_ENTRIES;
    let entries = (0..entry_count)
        .map(|index| {
            serde_json::json!({
                "id": format!("{index:08x}"),
                "message": format!("assertion-{index}"),
                "assert_type": "always",
                "category": "invariant",
                "observed": true,
                "observed_hits": 1,
                "success_count": 1,
                "failure_count": 0,
                "adoption_tracks": []
            })
        })
        .collect::<Vec<_>>();
    let count = u64::try_from(entry_count).expect("entry count fits u64");
    let report = serde_json::json!({
        "adoption_tracks": {},
        "assertion_coverage": entries,
        "cataloged_assertions": count,
        "catalog_status": "legacy-ambiguous",
        "collision_safe_evidence": false,
        "evidence_class": "instrumentation-dry-run",
        "exercised_assertions": count,
        "failed_assertions": 0,
        "gaps": [],
        "instrumentation_sources": {},
        "lifecycle_events": {"setup_complete": 1},
        "observed_assertions": count,
        "random_choice_calls": 0,
        "reachable_without_hit": [],
        "registered_assertions": count,
        "replay_boundary": "local SDK JSONL proves instrumentation shape only; VM campaign and replay artifacts must be reviewed separately",
        "replay_evidence": false,
        "schema": "chaoscontrol.sdk.local_report.v2",
        "setup_complete": true,
        "sometimes_without_success": [],
        "uncategorized_assertions": 0,
        "unobserved_assertion_count": 0,
        "unobserved_assertions": []
    });
    let mut file = NamedTempFile::new().expect("create maximum report fixture");
    serde_json::to_writer(&mut file, &report).expect("write maximum report fixture");
    check_sdk_assertion_quality_path(file.path()).expect("maximum report passes preflight");
}

#[test]
fn jsonl_preflight_rejects_deep_token_heavy_and_string_heavy_lines() {
    let deep = format!("{}0{}", "[".repeat(DEEP_LEVELS), "]".repeat(DEEP_LEVELS));
    expect_jsonl_error(&deep, "nesting depth");

    let token_heavy = format!("[{}]", vec!["0"; TOKEN_ITEMS].join(","));
    expect_jsonl_error(&token_heavy, "structural token budget");

    let string_heavy = format!(r#"{{"value":"{}"}}"#, "x".repeat(STRING_BYTES));
    expect_jsonl_error(&string_heavy, "string byte budget");
}

#[test]
fn quality_report_preflight_rejects_structural_amplification() {
    let deep = format!("{}0{}", "[".repeat(DEEP_LEVELS), "]".repeat(DEEP_LEVELS));
    expect_report_error(&deep, "nesting depth");

    let token_heavy = format!("[{}]", vec!["0"; REPORT_TOKEN_ITEMS].join(","));
    expect_report_error(&token_heavy, "structural token budget");

    let string_heavy = format!(r#"{{"value":"{}"}}"#, "x".repeat(REPORT_STRING_BYTES));
    expect_report_error(&string_heavy, "string byte budget");
}
