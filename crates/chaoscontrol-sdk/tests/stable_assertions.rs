use chaoscontrol_sdk::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;

const EXPECTED_ASSERTIONS: usize = 4;

#[test]
fn stable_macros_bind_all_assertion_kinds_to_catalog() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-stable-assertions-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    chaoscontrol_init();
    cc_assert_always_stable!(
        "org.example.stable",
        "always",
        "guest",
        "service-invariant",
        true,
        "always message"
    );
    cc_assert_sometimes_stable!(
        "org.example.stable",
        "sometimes",
        "guest",
        "operation",
        true,
        "sometimes message"
    );
    cc_assert_reachable_stable!(
        "org.example.stable",
        "reachable",
        "guest",
        "branch",
        "reachable message"
    );
    cc_assert_unreachable_stable!(
        "org.example.stable",
        "unreachable",
        "guest",
        "invariant",
        "unreachable message"
    );

    let content = std::fs::read_to_string(&output).expect("local output");
    let records = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .collect::<Vec<_>>();
    let descriptors = records
        .iter()
        .filter_map(|value| {
            value
                .get("chaoscontrol_assertion_catalog")?
                .get("descriptor")
        })
        .filter(|descriptor| descriptor["namespace"] == "org.example.stable")
        .map(|descriptor| {
            (
                descriptor["message"]
                    .as_str()
                    .expect("descriptor message")
                    .to_string(),
                (
                    descriptor["kind"]
                        .as_str()
                        .expect("descriptor kind")
                        .to_string(),
                    descriptor["logical_key"]["key"]
                        .as_str()
                        .expect("stable key")
                        .to_string(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(descriptors.len(), EXPECTED_ASSERTIONS);
    assert_eq!(descriptors["always message"].0, "always");
    assert_eq!(descriptors["sometimes message"].0, "sometimes");
    assert_eq!(descriptors["reachable message"].0, "reachable");
    assert_eq!(descriptors["unreachable message"].0, "unreachable");

    let bound_events = records
        .iter()
        .filter_map(|value| value.get("antithesis_assert"))
        .filter(|assertion| descriptors.contains_key(assertion["message"].as_str().unwrap_or("")))
        .collect::<Vec<_>>();
    assert_eq!(bound_events.len(), EXPECTED_ASSERTIONS);
    for event in bound_events {
        assert_eq!(event["catalog_status"], "accepted");
        assert!(event["catalog_token"].as_str().is_some());
        assert!(event["assertion_fingerprint"].as_str().is_some());
    }

    std::fs::remove_file(output).expect("remove local output");
}
