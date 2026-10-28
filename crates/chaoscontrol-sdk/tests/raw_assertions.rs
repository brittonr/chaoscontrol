use chaoscontrol_sdk::prelude::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[allow(clippy::too_many_arguments)]
fn raw(
    condition: bool,
    message: &str,
    class: &str,
    function: &str,
    hit: bool,
    must_hit: bool,
    id: &str,
) {
    assert_raw(
        condition,
        message.to_string(),
        &json!({"source": "raw"}),
        class.to_string(),
        function.to_string(),
        "src/raw.rs".to_string(),
        10,
        2,
        hit,
        must_hit,
        AssertType::Always,
        if must_hit {
            "Always".to_string()
        } else {
            "AlwaysOrUnreachable".to_string()
        },
        id.to_string(),
    );
}

#[test]
fn raw_assertions_declare_before_init_and_emit_afterward() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-raw-assertions-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    // Declarations must precede catalog emission.
    raw(
        false,
        "raw invariant",
        "raw::class",
        "raw::caller",
        false,
        false,
        "raw-invariant",
    );
    raw(
        false,
        "raw always",
        "raw::class",
        "raw::caller",
        false,
        true,
        "raw-always",
    );

    chaoscontrol_init();

    raw(
        true,
        "raw invariant",
        "raw::class",
        "raw::caller",
        true,
        false,
        "raw-invariant",
    );
    raw(
        true,
        "raw always",
        "raw::class",
        "raw::caller",
        true,
        true,
        "raw-always",
    );
    // A new site after catalog completion is rejected.
    raw(
        true,
        "late raw",
        "raw::class",
        "raw::caller",
        true,
        true,
        "late-raw",
    );

    let content = std::fs::read_to_string(&output).expect("local output");
    let records = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .collect::<Vec<_>>();

    let descriptors = records
        .iter()
        .filter_map(|record| record.get("chaoscontrol_assertion_catalog"))
        .filter(|record| record["record"] == "descriptor")
        .filter_map(|record| record["descriptor"].as_object())
        .filter(|descriptor| descriptor["namespace"] == "raw")
        .map(|descriptor| {
            (
                descriptor["message"].as_str().expect("message").to_string(),
                descriptor["source_file"]
                    .as_str()
                    .expect("file")
                    .to_string(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(descriptors["raw invariant"], "src/raw.rs");
    assert_eq!(descriptors["raw always"], "src/raw.rs");
    assert!(!descriptors.contains_key("late raw"));

    let assertions = records
        .iter()
        .filter_map(|record| record.get("antithesis_assert"))
        .map(|assertion| {
            (
                assertion["message"].as_str().expect("message").to_string(),
                assertion.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let invariant = &assertions["raw invariant"];
    assert_eq!(invariant["condition"], true);
    assert_eq!(invariant["must_hit"], false);
    assert_eq!(invariant["display_type"], "AlwaysOrUnreachable");
    assert_eq!(invariant["catalog_status"], "accepted");
    assert_eq!(invariant["location"]["file"], "src/raw.rs");

    let always = &assertions["raw always"];
    assert_eq!(always["condition"], true);
    assert_eq!(always["must_hit"], true);
    assert_eq!(always["display_type"], "Always");
    assert_eq!(always["catalog_status"], "accepted");

    assert!(!assertions.contains_key("late raw"));

    std::fs::remove_file(output).expect("remove local output");
}
