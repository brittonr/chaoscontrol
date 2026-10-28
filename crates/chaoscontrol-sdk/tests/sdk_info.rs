use chaoscontrol_sdk::prelude::*;
use serde_json::Value;

#[test]
fn local_output_starts_with_antithesis_sdk_metadata() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-sdk-info-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    cc_assert_always!(true, "sdk info assertion");
    chaoscontrol_init();

    let content = std::fs::read_to_string(&output).expect("local output");
    let records = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .collect::<Vec<_>>();

    let sdk = records
        .iter()
        .position(|record| record.get("antithesis_sdk").is_some())
        .map(|index| &records[index])
        .expect("sdk metadata record");
    assert_eq!(sdk["antithesis_sdk"]["language"]["name"], "Rust");
    let rust_version = sdk["antithesis_sdk"]["language"]["version"]
        .as_str()
        .expect("rust version");
    assert_ne!(rust_version, "unknown");
    assert!(rust_version
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit()));
    assert_eq!(
        sdk["antithesis_sdk"]["sdk_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(sdk["antithesis_sdk"]["protocol_version"], "1.0.0");

    let first_catalog = records
        .iter()
        .position(|record| record.get("chaoscontrol_assertion_catalog").is_some())
        .expect("catalog record");
    let sdk_index = records
        .iter()
        .position(|record| record.get("antithesis_sdk").is_some())
        .expect("sdk index");
    assert!(
        sdk_index < first_catalog,
        "sdk metadata must precede catalog"
    );

    std::fs::remove_file(output).expect("remove local output");
}
