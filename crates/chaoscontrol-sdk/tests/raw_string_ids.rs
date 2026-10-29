#![cfg(feature = "full")]

use chaoscontrol_sdk::assert::{assert_raw, location_id, AssertType};
use serde_json::{json, Value};
const IDS: [&str; 2] = [
    "org.example.sparse-a:e3870d15ed661eaa",
    "org.example.sparse-b:05d3992156bd4061",
];
fn emit(id: &str, condition: bool, hit: bool) {
    assert_raw(
        condition,
        "same raw message".to_owned(),
        &json!({"site": id}),
        "adapter".to_owned(),
        "adapter::check".to_owned(),
        "src/adapter.rs".to_owned(),
        12,
        3,
        hit,
        true,
        AssertType::Always,
        "Always".to_owned(),
        id.to_owned(),
    );
}
// r[verify chaoscontrol.raw_string_ids.independent]
#[test]
fn colliding_raw_ids_keep_independent_identities_and_samples() {
    assert_eq!(location_id(IDS[0]), location_id(IDS[1]));
    let output =
        std::env::temp_dir().join(format!("cc-raw-string-ids-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);
    for id in IDS {
        emit(id, false, false);
    }
    chaoscontrol_sdk::chaoscontrol_init();
    for condition in [true, false, true, false] {
        for id in IDS {
            emit(id, condition, true);
        }
    }
    emit("late raw ID", false, true);
    let content = std::fs::read_to_string(&output).unwrap();
    let records = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let mut fingerprints = Vec::new();
    for id in IDS {
        let descriptor = records
            .iter()
            .filter_map(|r| r.get("chaoscontrol_assertion_catalog"))
            .find(|r| r["record"] == "descriptor" && r["descriptor"]["logical_key"]["key"] == id)
            .expect("both string IDs must be declared");
        fingerprints.push(descriptor["fingerprint"].clone());
        let assertions = records
            .iter()
            .filter_map(|r| r.get("antithesis_assert"))
            .filter(|r| r["id"] == id)
            .collect::<Vec<_>>();
        assert_eq!(assertions.iter().filter(|r| r["hit"] == false).count(), 1);
        let hits = assertions
            .iter()
            .filter(|r| r["hit"] == true)
            .collect::<Vec<_>>();
        let expected = if cfg!(feature = "sparse_emission") {
            vec![true, false]
        } else {
            vec![true, false, true, false]
        };
        assert_eq!(
            hits.iter()
                .map(|r| r["condition"].as_bool().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
        for assertion in assertions {
            assert_eq!(
                assertion["assertion_fingerprint"],
                descriptor["fingerprint"]
            );
            assert_eq!(assertion["catalog_status"], "accepted");
            if assertion["hit"] == true {
                assert_eq!(assertion["details"]["site"], id);
            }
        }
    }
    assert_ne!(fingerprints[0], fingerprints[1]);
    assert!(records
        .iter()
        .filter_map(|r| r.get("antithesis_assert"))
        .all(|r| r["id"] != "late raw ID"));
    std::fs::remove_file(output).unwrap();
}
