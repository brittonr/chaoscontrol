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
        .find_map(|record| record.get("antithesis_assert").cloned())
        .expect("assertion record");

    assert_eq!(assertion["message"], "serialize details");
    assert_eq!(assertion["details"]["max_allowed"], 100);
    assert_eq!(assertion["details"]["actual"], 7);
    assert_eq!(assertion["details"]["source"], "demo");

    std::fs::remove_file(output).expect("remove local output");
}
