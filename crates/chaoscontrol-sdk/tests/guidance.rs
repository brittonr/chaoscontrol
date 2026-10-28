use chaoscontrol_sdk::prelude::*;
use serde_json::Value;

fn numeric_site(value: u64) {
    cc_assert_sometimes_gt!(value + 5, 5, "numeric guidance");
}

#[test]
fn guidance_records_match_the_antithesis_shape_and_watermark() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-guidance-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    chaoscontrol_init();

    cc_assert_sometimes_all!({first: true, second: false}, "boolean all guidance");
    cc_assert_always_any!({first: true, second: false}, "boolean any guidance");
    numeric_site(0);
    numeric_site(3);
    numeric_site(1);

    let content = std::fs::read_to_string(&output).expect("local output");
    let guidance = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .filter_map(|record| record.get("antithesis_guidance").cloned())
        .collect::<Vec<_>>();

    assert_eq!(guidance.len(), 4);

    let all = guidance
        .iter()
        .find(|record| record["message"] == "boolean all guidance")
        .expect("all guidance");
    assert_eq!(all["guidance_type"], "boolean");
    assert_eq!(all["id"], "boolean all guidance");
    assert_eq!(all["maximize"], true);
    assert_eq!(all["hit"], true);
    assert_eq!(all["guidance_data"]["first"], true);
    assert_eq!(all["guidance_data"]["second"], false);
    assert_eq!(all["catalog_status"], "accepted");
    assert!(all["catalog_token"].as_str().is_some());
    assert!(all["assertion_fingerprint"].as_str().is_some());
    assert_eq!(all["location"]["class"], module_path!());
    assert!(all["location"]["file"]
        .as_str()
        .expect("file")
        .ends_with("guidance.rs"));

    let any = guidance
        .iter()
        .find(|record| record["message"] == "boolean any guidance")
        .expect("any guidance");
    assert_eq!(any["guidance_type"], "boolean");
    assert_eq!(any["maximize"], false);

    let numeric = guidance
        .iter()
        .filter(|record| record["message"] == "numeric guidance")
        .collect::<Vec<_>>();
    assert_eq!(numeric.len(), 2, "watermark accepts only new extremes");
    for record in &numeric {
        assert_eq!(record["guidance_type"], "numeric");
        assert_eq!(record["maximize"], true);
        assert_eq!(record["catalog_status"], "accepted");
        assert!(record["catalog_token"].as_str().is_some());
        assert!(record["assertion_fingerprint"].as_str().is_some());
        assert_eq!(record["guidance_data"]["right"], 5);
    }
    assert_eq!(numeric[0]["guidance_data"]["left"], 5);
    assert_eq!(numeric[1]["guidance_data"]["left"], 8);

    std::fs::remove_file(output).expect("remove local output");
}
