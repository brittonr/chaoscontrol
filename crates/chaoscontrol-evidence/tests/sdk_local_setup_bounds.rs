use chaoscontrol_evidence::{check_sdk_assertion_quality_report, summarize_sdk_local_jsonl};
use serde_json::{json, Map, Value};

const EVIDENCE_CLASS: &str = "instrumentation-dry-run";
const MAX_SETUP_DETAIL_FIELDS: usize = 64;
const MAX_SETUP_DETAIL_KEY_BYTES: usize = 128;
const OVERCOUNTED_SETUP_TRANSITIONS: u64 = 2;

fn setup(details: Map<String, Value>) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&json!({
            "antithesis_setup": {"status": "complete", "details": details}
        }))
        .expect("setup JSON")
    )
}

#[test]
fn setup_metadata_bounds_and_transition_count_remain_enforced() {
    let valid_setup = setup(Map::new());
    let mut report = summarize_sdk_local_jsonl(&valid_setup, EVIDENCE_CLASS, None)
        .expect("empty setup metadata is valid");
    report["lifecycle_events"]["setup_complete"] = Value::from(OVERCOUNTED_SETUP_TRANSITIONS);
    assert!(check_sdk_assertion_quality_report(&report).is_err());

    let mut too_many = Map::new();
    for index in 0..=MAX_SETUP_DETAIL_FIELDS {
        too_many.insert(format!("field-{index}"), Value::Bool(true));
    }
    assert!(summarize_sdk_local_jsonl(&setup(too_many), EVIDENCE_CLASS, None).is_err());

    let mut long_key = Map::new();
    long_key.insert(
        "x".repeat(MAX_SETUP_DETAIL_KEY_BYTES + 1),
        Value::Bool(true),
    );
    assert!(summarize_sdk_local_jsonl(&setup(long_key), EVIDENCE_CLASS, None).is_err());
}

#[test]
fn completed_setup_does_not_bypass_record_or_resource_validation() {
    const EXCESSIVE_DEPTH: usize = 65;
    const EXCESSIVE_STRING_BYTES: usize = 13 * 1024;
    let valid_prefix = setup(Map::new());
    let too_many = (0..=MAX_SETUP_DETAIL_FIELDS)
        .map(|index| (format!("field-{index}"), Value::Null))
        .collect();
    let long_key = [("x".repeat(MAX_SETUP_DETAIL_KEY_BYTES + 1), Value::Null)]
        .into_iter()
        .collect();
    for invalid in [
        "{\"antithesis_setup\":null}\n".to_string(),
        "{\"antithesis_setup\":{\"status\":\"pending\",\"details\":null}}\n".to_string(),
        "{\"antithesis_setup\":{\"status\":\"complete\"}}\n".to_string(),
        "{\"antithesis_setup\":{\"status\":\"complete\",\"details\":null,\"extra\":true}}\n"
            .to_string(),
        setup(too_many),
        setup(long_key),
        format!(
            "{{\"antithesis_setup\":{{\"status\":\"complete\",\"details\":{}0{}}}}}\n",
            "[".repeat(EXCESSIVE_DEPTH),
            "]".repeat(EXCESSIVE_DEPTH)
        ),
        format!(
            "{{\"antithesis_setup\":{{\"status\":\"complete\",\"details\":\"{}\"}}}}\n",
            "x".repeat(EXCESSIVE_STRING_BYTES)
        ),
    ] {
        assert!(summarize_sdk_local_jsonl(
            &format!("{valid_prefix}{invalid}"),
            EVIDENCE_CLASS,
            None
        )
        .is_err());
    }
}
