#![cfg(feature = "full")]

use chaoscontrol_sdk::assert::{assert_raw, AssertType};
use serde_json::{json, Value};

fn raw(id: &str, condition: bool, hit: bool, must_hit: bool) {
    assert_raw(
        condition,
        id.to_string(),
        &json!({"observation": condition}),
        "adapter".to_string(),
        "adapter::reachability".to_string(),
        "src/adapter.rs".to_string(),
        21,
        3,
        hit,
        must_hit,
        AssertType::Reachability,
        if must_hit { "Reachable" } else { "Unreachable" }.to_string(),
        id.to_string(),
    );
}

// r[verify chaoscontrol.raw_reachability.obligation]
#[test]
fn raw_reachability_kind_is_stable_between_declaration_and_observation() {
    let output =
        std::env::temp_dir().join(format!("cc-raw-reachability-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);
    raw("required branch", false, false, true);
    raw("forbidden branch", false, false, false);
    chaoscontrol_sdk::chaoscontrol_init();
    raw("required branch", true, true, true);
    raw("forbidden branch", false, true, false);
    raw("late branch", true, true, true);

    let content = std::fs::read_to_string(&output).unwrap();
    let records = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    for (id, kind, display, must_hit, condition) in [
        ("required branch", "reachable", "Reachable", true, true),
        (
            "forbidden branch",
            "unreachable",
            "Unreachable",
            false,
            false,
        ),
    ] {
        let descriptor = records
            .iter()
            .filter_map(|record| record.get("chaoscontrol_assertion_catalog"))
            .find(|record| {
                record["record"] == "descriptor" && record["descriptor"]["message"] == id
            })
            .unwrap();
        assert_eq!(descriptor["descriptor"]["kind"], kind);
        let assertions = records
            .iter()
            .filter_map(|record| record.get("antithesis_assert"))
            .filter(|record| record["id"] == id)
            .collect::<Vec<_>>();
        let declaration = assertions
            .iter()
            .find(|record| record["hit"] == false)
            .unwrap();
        let hit = assertions
            .iter()
            .find(|record| record["hit"] == true)
            .expect("declared raw hit must not disappear");
        assert_eq!(declaration["condition"], false);
        assert_eq!(hit["condition"], condition);
        for assertion in [*declaration, *hit] {
            assert_eq!(assertion["assert_type"], "reachability");
            assert_eq!(assertion["display_type"], display);
            assert_eq!(assertion["must_hit"], must_hit);
            assert_eq!(
                assertion["assertion_fingerprint"],
                descriptor["fingerprint"]
            );
            assert_eq!(assertion["catalog_status"], "accepted");
            assert_eq!(assertion["location"]["file"], "src/adapter.rs");
            assert_eq!(assertion["location"]["function"], "adapter::reachability");
        }
        assert_eq!(hit["details"]["observation"], condition);
    }
    assert!(records
        .iter()
        .filter_map(|record| record.get("antithesis_assert"))
        .all(|record| record["id"] != "late branch"));
    std::fs::remove_file(output).unwrap();
}
