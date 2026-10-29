use chaoscontrol_sdk::prelude::*;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct Details<'a> {
    max_allowed: u64,
    actual: u64,
    source: &'a str,
}

#[test]
fn assertion_details_accept_arbitrary_serialize_types() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-serialize-details-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    chaoscontrol_init();

    let details = Details {
        max_allowed: 100,
        actual: 7,
        source: "demo",
    };
    cc_assert_always!(
        details.actual < details.max_allowed,
        "serialize details",
        &details
    );

    let content = std::fs::read_to_string(&output).expect("local output");
    let assertion = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .find_map(|record| {
            record
                .get("antithesis_assert")
                .filter(|assertion| assertion["hit"] == true)
                .cloned()
        })
        .expect("assertion record");

    assert_eq!(assertion["message"], "serialize details");
    assert_eq!(assertion["details"]["max_allowed"], 100);
    assert_eq!(assertion["details"]["actual"], 7);
    assert_eq!(assertion["details"]["source"], "demo");

    std::fs::remove_file(output).expect("remove local output");
}

const JSON_CHILD_MODE: &str = "CHAOSCONTROL_TEST_JSON_DETAILS";

// r[verify chaoscontrol.assertion_json_details.shapes]
// r[verify chaoscontrol.assertion_json_details.bounds]
#[test]
fn json_details_preserve_shapes_and_process_scoping() {
    for (index, (details, process, expected_hit)) in [
        (Value::Null, None, true),
        (serde_json::json!(true), None, true),
        (serde_json::json!(7), None, true),
        (serde_json::json!(-1.5), None, true),
        (serde_json::json!("ready λ"), None, true),
        (
            serde_json::json!([null, {"category": "nested"}]),
            None,
            true,
        ),
        (serde_json::json!({"term": 7}), None, true),
        (
            serde_json::json!({"term": 7}),
            Some("process:v1:fixture"),
            true,
        ),
        (Value::Null, Some("process:v1:fixture"), false),
        (serde_json::json!([]), Some("process:v1:fixture"), false),
        (serde_json::json!({}), Some("invalid process token"), false),
        (
            Value::String(
                "x".repeat(chaoscontrol_protocol::identity::MAX_ASSERTION_EVENT_DETAILS_BYTES),
            ),
            None,
            false,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let output = std::env::temp_dir().join(format!(
            "chaoscontrol-json-details-{}-{index}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&output);
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "json_details_child", "--ignored", "--nocapture"])
            .env(JSON_CHILD_MODE, details.to_string())
            .env(chaoscontrol_sdk::LOCAL_OUTPUT, &output)
            .env_remove("CHAOSCONTROL_PROCESS_ID");
        if let Some(process) = process {
            command.env("CHAOSCONTROL_PROCESS_ID", process);
        }
        let result = command.output().expect("SDK child process");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let records: Vec<Value> = std::fs::read_to_string(&output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let hits: Vec<_> = records
            .iter()
            .filter_map(|record| record.get("antithesis_assert"))
            .filter(|assertion| assertion["hit"] == true)
            .collect();
        assert_eq!(
            hits.len(),
            usize::from(expected_hit),
            "details={details}, process={process:?}"
        );
        if expected_hit {
            let mut expected = details;
            if let Some(process) = process {
                expected["chaoscontrol_process_identity"] = serde_json::json!(process);
            }
            assert_eq!(hits[0]["details"], expected);
            assert_eq!(hits[0]["condition"], true);
            assert_eq!(hits[0]["catalog_status"], "accepted");
        }
        std::fs::remove_file(output).unwrap();
    }
}

#[test]
#[ignore = "launched by json_details_preserve_shapes_and_process_scoping"]
fn json_details_child() {
    let details: Value = serde_json::from_str(&std::env::var(JSON_CHILD_MODE).unwrap()).unwrap();
    cc_assert_always!(true, "JSON assertion payload", &details);
}
