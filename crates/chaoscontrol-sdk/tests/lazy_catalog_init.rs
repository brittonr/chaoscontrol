use chaoscontrol_sdk::prelude::*;
use serde_json::Value;

#[test]
fn first_assertion_registers_the_catalog_without_explicit_init() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-lazy-catalog-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    // Deliberately omit chaoscontrol_init(): the first assertion must emit
    // the catalog before the assertion record, matching Antithesis.
    cc_assert_always!(true, "lazy assertion");

    let content = std::fs::read_to_string(&output).expect("local output");
    let records = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .collect::<Vec<_>>();

    let catalog = records
        .iter()
        .filter_map(|record| record.get("chaoscontrol_assertion_catalog"))
        .collect::<Vec<_>>();
    assert!(catalog.iter().any(|record| record["record"] == "complete"));
    assert!(catalog.iter().any(|record| {
        record["record"] == "descriptor" && record["descriptor"]["message"] == "lazy assertion"
    }));

    let assertion = records
        .iter()
        .find_map(|record| record.get("antithesis_assert"))
        .expect("assertion record");
    assert_eq!(assertion["message"], "lazy assertion");
    assert_eq!(assertion["catalog_status"], "accepted");

    std::fs::remove_file(output).expect("remove local output");
}
