use chaoscontrol_evidence::{check_sdk_assertion_quality_report, summarize_sdk_local_jsonl};
use serde_json::{json, Map, Value};

const EVIDENCE_CLASS: &str = "instrumentation-dry-run";
const MAX_SETUP_DETAIL_FIELDS: usize = 64;
const MAX_SETUP_DETAIL_KEY_BYTES: usize = 128;
const DUPLICATE_SETUP_COUNT: u64 = 2;

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
fn setup_details_require_a_bounded_object() {
    let valid_setup = setup(Map::new());
    let mut report = summarize_sdk_local_jsonl(&valid_setup, EVIDENCE_CLASS, None)
        .expect("empty setup metadata is valid");
    assert!(summarize_sdk_local_jsonl(
        &format!("{valid_setup}{valid_setup}"),
        EVIDENCE_CLASS,
        None,
    )
    .is_err());
    report["lifecycle_events"]["setup_complete"] = Value::from(DUPLICATE_SETUP_COUNT);
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
