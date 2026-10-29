#![cfg(feature = "full")]

use chaoscontrol_sdk::prelude::*;
use serde_json::{json, Value};
use std::cell::Cell;

// r[verify chaoscontrol.bundle_details.order]
// r[verify chaoscontrol.bundle_details.preservation]
#[test]
fn bundle_details_are_captured_before_conditions() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-bundle-detail-order-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var(chaoscontrol_sdk::LOCAL_OUTPUT, &output);
    macro_rules! exercise {
        ($bundle:ident, $message:literal, $first:literal, $second:literal) => {{
            let stage = Cell::new(0);
            let caller = json!({"custom": $message, "first": "caller", "second": 7});
            $bundle!(
                {
                    first: { assert_eq!(stage.get(), 1, "details must run first"); stage.set(2); $first },
                    second: { assert_eq!(stage.get(), 2); stage.set(3); $second }
                },
                $message,
                { assert_eq!(stage.get(), 0); stage.set(1); &caller }
            );
            assert_eq!(stage.get(), 3, "all expressions run exactly once in order");
            assert_eq!(caller, json!({"custom": $message, "first": "caller", "second": 7}), "borrowed caller data is unchanged");
        }};
    }
    exercise!(cc_assert_always_any, "ordered any", true, false);
    exercise!(cc_assert_always_some, "ordered some", false, false);
    exercise!(cc_assert_sometimes_all, "ordered all", true, false);
    // A named condition must not change which caller value supplies details.
    let first = json!({"custom": "shadowed details", "first": "caller", "second": 7});
    cc_assert_always_any!({first: true, second: false}, "shadowed details", first);
    assert_eq!(
        first,
        json!({"custom": "shadowed details", "first": "caller", "second": 7})
    );
    let content = std::fs::read_to_string(&output).expect("local output");
    let records: Vec<Value> = content
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON record"))
        .collect();
    for (message, condition, first, second) in [
        ("ordered any", true, true, false),
        ("ordered some", false, false, false),
        ("ordered all", false, true, false),
        ("shadowed details", true, true, false),
    ] {
        let hit = records
            .iter()
            .filter_map(|record| record.get("antithesis_assert"))
            .find(|record| record["hit"] == true && record["message"] == message)
            .expect("assertion hit");
        assert_eq!(hit["condition"], condition);
        assert_eq!(
            hit["details"],
            json!({"custom": message, "first": first, "second": second})
        );
        let guidance = records
            .iter()
            .filter_map(|record| record.get("antithesis_guidance"))
            .find(|record| record["hit"] == true && record["message"] == message)
            .expect("guidance hit");
        assert_eq!(
            guidance["guidance_data"],
            json!({"first": first, "second": second})
        );
        assert_eq!(
            guidance["assertion_fingerprint"],
            hit["assertion_fingerprint"]
        );
    }
    std::fs::remove_file(output).expect("remove local output");
}
