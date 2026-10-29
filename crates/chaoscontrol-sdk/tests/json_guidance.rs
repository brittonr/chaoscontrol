use chaoscontrol_sdk::prelude::*;
use serde_json::{json, Value};

#[test]
fn json_guidance_binds_to_a_stable_assertion_site() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-json-guidance-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    chaoscontrol_init();

    cc_assert_sometimes_stable!(
        "org.example.sparse-a",
        "e3870d15ed661eaa",
        "guest",
        "invariant",
        true,
        "json guidance site"
    );
    // This undeclared pair shares the accepted site's compact alias. It must
    // not be attributed to that site merely because kind/message also match.
    cc_guidance_json!(
        "org.example.sparse-b",
        "05d3992156bd4061",
        AssertionKind::Sometimes,
        "json guidance site",
        true,
        json!({"score": 999, "unit": "ms"})
    );
    cc_guidance_json!(
        "org.example.sparse-a",
        "e3870d15ed661eaa",
        AssertionKind::Sometimes,
        "json guidance site",
        true,
        json!({"score": 3, "unit": "ms"})
    );

    let content = std::fs::read_to_string(&output).expect("local output");
    let observations: Vec<_> = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .filter_map(|record| record.get("antithesis_guidance").cloned())
        .filter(|record| record["guidance_type"] == "json")
        .collect();
    assert_eq!(
        observations.len(),
        1,
        "undeclared alias collisions are rejected"
    );
    let guidance = &observations[0];

    assert_eq!(guidance["hit"], true);
    assert_eq!(guidance["maximize"], true);
    assert_eq!(guidance["guidance_data"]["score"], 3);
    assert_eq!(guidance["guidance_data"]["unit"], "ms");
    assert_eq!(guidance["catalog_status"], "accepted");
    assert!(guidance["catalog_token"].as_str().is_some());
    assert!(guidance["assertion_fingerprint"].as_str().is_some());
    assert!(guidance["location"]["function"]
        .as_str()
        .expect("function")
        .contains("json_guidance_binds_to_a_stable_assertion_site"));

    std::fs::remove_file(output).expect("remove local output");
}
