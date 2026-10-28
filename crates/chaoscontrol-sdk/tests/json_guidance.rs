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
        "org.example.json",
        "guidance",
        "guest",
        "invariant",
        true,
        "json guidance site"
    );
    cc_guidance_json!(
        "org.example.json",
        "guidance",
        AssertionKind::Sometimes,
        "json guidance site",
        true,
        json!({"score": 3, "unit": "ms"})
    );

    let content = std::fs::read_to_string(&output).expect("local output");
    let guidance = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .filter_map(|record| record.get("antithesis_guidance").cloned())
        .find(|record| record["guidance_type"] == "json")
        .expect("json guidance record");

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
