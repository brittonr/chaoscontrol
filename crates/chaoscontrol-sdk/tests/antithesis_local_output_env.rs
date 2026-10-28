use chaoscontrol_sdk::prelude::*;
use serde_json::{json, Value};

#[test]
fn antithesis_local_output_env_var_is_accepted() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-antithesis-local-output-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::remove_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT");
    std::env::set_var("ANTITHESIS_SDK_LOCAL_OUTPUT", &output);

    chaoscontrol_init();
    assert!(is_local_output());

    cc_assert_always!(
        true,
        "env fallback assertion",
        &json!({"source": "antithesis"})
    );
    send_event("  milestone  ", &json!({"step": 1}));
    send_event("   ", &json!({"step": 2}));

    let content = std::fs::read_to_string(&output).expect("local output");
    let records = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .collect::<Vec<_>>();

    let assertion = records
        .iter()
        .find_map(|record| record.get("antithesis_assert"))
        .expect("assertion record");
    assert_eq!(assertion["message"], "env fallback assertion");
    assert_eq!(assertion["details"]["source"], "antithesis");

    assert_eq!(
        records
            .iter()
            .find_map(|record| record.get("  milestone  "))
            .expect("verbatim event record")["step"],
        1
    );
    assert_eq!(
        records
            .iter()
            .find_map(|record| record.get("   "))
            .expect("verbatim empty-name event record")["step"],
        2
    );

    std::fs::remove_file(output).expect("remove local output");
}
