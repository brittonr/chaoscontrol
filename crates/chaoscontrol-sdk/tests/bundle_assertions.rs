use chaoscontrol_sdk::prelude::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[test]
fn bundle_assertions_emit_aggregate_condition_and_named_details() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-bundle-assertions-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    chaoscontrol_init();

    cc_assert_always_any!({first: true, second: false}, "any passes");
    cc_assert_always_any!({first: false, second: false}, "any fails");
    cc_assert_always_some!({first: true, second: false}, "always some bundle passes");
    cc_assert_always_some!({first: false, second: false}, "always some bundle fails");
    cc_assert_always_some!(Some(7), "always some option passes");
    cc_assert_always_some!(None::<u8>, "always some option fails");
    cc_assert_sometimes_all!({first: true, second: true}, "all passes");
    cc_assert_sometimes_all!({first: true, second: false}, "all fails");
    cc_assert_sometimes_all!(
        {first: true, second: true},
        "merged details",
        &json!({"custom": 7})
    );

    let content = std::fs::read_to_string(&output).expect("local output");
    let assertions = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .filter_map(|record| record.get("antithesis_assert").cloned())
        .map(|assertion| {
            (
                assertion["message"]
                    .as_str()
                    .expect("assertion message")
                    .to_string(),
                assertion,
            )
        })
        .collect::<BTreeMap<_, _>>();

    assert_eq!(assertions.len(), 9);

    let any_passes = &assertions["any passes"];
    assert_eq!(any_passes["assert_type"], "always");
    assert_eq!(any_passes["condition"], true);
    assert_eq!(any_passes["details"]["first"], true);
    assert_eq!(any_passes["details"]["second"], false);

    let any_fails = &assertions["any fails"];
    assert_eq!(any_fails["assert_type"], "always");
    assert_eq!(any_fails["condition"], false);

    let some_bundle_passes = &assertions["always some bundle passes"];
    assert_eq!(some_bundle_passes["assert_type"], "always");
    assert_eq!(some_bundle_passes["condition"], true);
    assert_eq!(some_bundle_passes["details"]["first"], true);
    assert_eq!(some_bundle_passes["details"]["second"], false);

    let some_bundle_fails = &assertions["always some bundle fails"];
    assert_eq!(some_bundle_fails["assert_type"], "always");
    assert_eq!(some_bundle_fails["condition"], false);

    let some_option_passes = &assertions["always some option passes"];
    assert_eq!(some_option_passes["condition"], true);
    assert_eq!(some_option_passes["details"], json!({}));

    let some_option_fails = &assertions["always some option fails"];
    assert_eq!(some_option_fails["condition"], false);
    assert_eq!(some_option_fails["details"]["value"], "None");

    let all_passes = &assertions["all passes"];
    assert_eq!(all_passes["assert_type"], "sometimes");
    assert_eq!(all_passes["condition"], true);

    let all_fails = &assertions["all fails"];
    assert_eq!(all_fails["assert_type"], "sometimes");
    assert_eq!(all_fails["condition"], false);
    assert_eq!(all_fails["details"]["first"], true);
    assert_eq!(all_fails["details"]["second"], false);

    let merged = &assertions["merged details"];
    assert_eq!(merged["condition"], true);
    assert_eq!(merged["details"]["custom"], 7);
    assert_eq!(merged["details"]["first"], true);
    assert_eq!(merged["details"]["second"], true);

    std::fs::remove_file(output).expect("remove local output");
}
