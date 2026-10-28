use chaoscontrol_sdk::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;

#[test]
fn fallback_schema_reports_antithesis_display_types_and_must_hit() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-fallback-schema-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    chaoscontrol_init();

    cc_assert_always!(true, "always assertion");
    cc_assert_always_or_unreachable!(true, "always or unreachable assertion");
    cc_assert_sometimes!(true, "sometimes assertion");
    cc_assert_reachable!("reachable assertion");
    cc_assert_unreachable!("unreachable assertion");

    let content = std::fs::read_to_string(&output).expect("local output");
    let assertions = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .filter_map(|record| record.get("antithesis_assert").cloned())
        .map(|assertion| {
            (
                assertion["message"]
                    .as_str()
                    .expect("assertion message")
                    .to_string(),
                assertion,
            )
        })
        .collect::<BTreeMap<_, _>>();

    assert_eq!(assertions.len(), 5);

    for (message, assert_type, display_type, must_hit) in [
        ("always assertion", "always", "Always", true),
        (
            "always or unreachable assertion",
            "always",
            "AlwaysOrUnreachable",
            false,
        ),
        ("sometimes assertion", "sometimes", "Sometimes", true),
        ("reachable assertion", "reachability", "Reachable", true),
        (
            "unreachable assertion",
            "reachability",
            "Unreachable",
            false,
        ),
    ] {
        let assertion = &assertions[message];
        assert_eq!(assertion["assert_type"], assert_type, "{message}");
        assert_eq!(assertion["display_type"], display_type, "{message}");
        assert_eq!(assertion["must_hit"], must_hit, "{message}");
    }

    std::fs::remove_file(output).expect("remove local output");
}
