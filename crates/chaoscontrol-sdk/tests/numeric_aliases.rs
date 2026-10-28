use chaoscontrol_sdk::prelude::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[test]
fn antithesis_named_numeric_aliases_emit_the_expected_conditions() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-numeric-aliases-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    chaoscontrol_init();

    cc_assert_always_greater_than!(2, 1, "always greater than passes");
    cc_assert_always_greater_than!(1, 2, "always greater than fails");
    cc_assert_always_greater_than_or_equal_to!(2, 2, "always greater or equal passes");
    cc_assert_always_less_than!(1, 2, "always less than passes");
    cc_assert_always_less_than_or_equal_to!(2, 2, "always less or equal passes");
    cc_assert_sometimes_greater_than!(2, 1, "sometimes greater than passes");
    cc_assert_sometimes_greater_than_or_equal_to!(1, 2, "sometimes greater or equal fails");
    cc_assert_sometimes_less_than!(2, 1, "sometimes less than fails");
    cc_assert_sometimes_less_than_or_equal_to!(1, 1, "sometimes less or equal passes");
    cc_assert_always_greater_than!(2, 1, "alias forwards details", &json!({"custom": true}));

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

    assert_eq!(assertions.len(), 10);

    let expected = [
        ("always greater than passes", "always", true),
        ("always greater than fails", "always", false),
        ("always greater or equal passes", "always", true),
        ("always less than passes", "always", true),
        ("always less or equal passes", "always", true),
        ("sometimes greater than passes", "sometimes", true),
        ("sometimes greater or equal fails", "sometimes", false),
        ("sometimes less than fails", "sometimes", false),
        ("sometimes less or equal passes", "sometimes", true),
        ("alias forwards details", "always", true),
    ];
    for (message, assert_type, condition) in expected {
        let assertion = &assertions[message];
        assert_eq!(assertion["assert_type"], assert_type, "{message}");
        assert_eq!(assertion["condition"], condition, "{message}");
    }
    assert_eq!(
        assertions["always greater than fails"]["details"]["left"],
        1
    );
    assert_eq!(
        assertions["always greater than fails"]["details"]["right"],
        2
    );
    assert_eq!(
        assertions["always less or equal passes"]["details"]["left"],
        2
    );
    assert_eq!(
        assertions["always less or equal passes"]["details"]["right"],
        2
    );
    assert_eq!(
        assertions["alias forwards details"]["details"]["custom"],
        true
    );
    assert_eq!(assertions["alias forwards details"]["details"]["left"], 2);
    assert_eq!(assertions["alias forwards details"]["details"]["right"], 1);

    std::fs::remove_file(output).expect("remove local output");
}
