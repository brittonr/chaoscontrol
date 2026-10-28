use chaoscontrol_sdk::prelude::*;
use serde_json::Value;

#[test]
fn prelude_exposes_the_unprefixed_antithesis_names() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-prelude-alias-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    antithesis_init();
    assert_always!(true, "prelude always");
    assert_always_or_unreachable!(true, "prelude aou");
    assert_sometimes!(true, "prelude sometimes");
    assert_reachable!("prelude reachable");
    assert_unreachable!("prelude unreachable");
    assert_always_greater_than!(2, 1, "prelude numeric");
    assert_always_some!({first: true, second: false}, "prelude any");
    assert_sometimes_all!({first: true, second: true}, "prelude all");
    lifecycle::send_event("prelude module event", &serde_json::json!({"ok": true}));
    let _ = random::random_choice_index(3);

    let content = std::fs::read_to_string(&output).expect("local output");
    let messages = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .filter_map(|record| record.get("antithesis_assert").cloned())
        .filter(|assertion| assertion["hit"] == true)
        .filter_map(|assertion| assertion["message"].as_str().map(str::to_string))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(messages.len(), 8);
    assert!(messages.contains("prelude numeric"));
    assert!(messages.contains("prelude all"));

    std::fs::remove_file(output).expect("remove local output");
}
