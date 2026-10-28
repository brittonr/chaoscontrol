#![cfg(feature = "sparse_emission")]

use chaoscontrol_sdk::prelude::*;
use serde_json::{json, Value};

fn sampled_site(condition: bool) {
    cc_assert_sometimes!(condition, "sparse site");
}

fn raw_declaration() {
    assert_raw(
        false,
        "sparse raw".to_string(),
        &json!({}),
        "sparse".to_string(),
        "sparse::raw_site".to_string(),
        "src/sparse.rs".to_string(),
        1,
        1,
        false,
        true,
        AssertType::Sometimes,
        "Sometimes".to_string(),
        "sparse-raw".to_string(),
    );
}

fn raw_site(condition: bool) {
    assert_raw(
        condition,
        "sparse raw".to_string(),
        &json!({}),
        "sparse".to_string(),
        "sparse::raw_site".to_string(),
        "src/sparse.rs".to_string(),
        1,
        1,
        true,
        true,
        AssertType::Sometimes,
        "Sometimes".to_string(),
        "sparse-raw".to_string(),
    );
}

#[test]
fn sparse_emission_keeps_first_pass_and_first_fail_only() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-sparse-emission-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    raw_declaration();
    chaoscontrol_init();
    sampled_site(true);
    sampled_site(false);
    sampled_site(true);
    sampled_site(false);

    raw_site(true);
    raw_site(false);

    let content = std::fs::read_to_string(&output).expect("local output");
    let assertions = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .filter_map(|record| record.get("antithesis_assert").cloned())
        .filter(|assertion| assertion["hit"] == true)
        .collect::<Vec<_>>();

    let sampled = assertions
        .iter()
        .filter(|assertion| assertion["message"] == "sparse site")
        .collect::<Vec<_>>();
    assert_eq!(sampled.len(), 2);
    assert!(sampled
        .iter()
        .any(|assertion| assertion["condition"] == true));
    assert!(sampled
        .iter()
        .any(|assertion| assertion["condition"] == false));

    let raw = assertions
        .iter()
        .filter(|assertion| assertion["message"] == "sparse raw")
        .count();
    assert_eq!(raw, 2, "raw assertions are not sampled");

    let declarations = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .filter_map(|record| record.get("antithesis_assert").cloned())
        .filter(|assertion| assertion["hit"] == false)
        .filter(|assertion| {
            matches!(
                assertion["message"].as_str(),
                Some("sparse site" | "sparse raw")
            )
        })
        .count();
    assert_eq!(declarations, 2, "declarations are never sampled");

    std::fs::remove_file(output).expect("remove local output");
}
