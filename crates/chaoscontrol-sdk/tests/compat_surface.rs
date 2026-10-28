use chaoscontrol_sdk::compat::*;
use chaoscontrol_sdk::prelude::AssertionKind;
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[test]
fn compat_module_exposes_the_antithesis_assertion_names() {
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-compat-surface-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);

    assert_eq!(
        chaoscontrol_sdk::LOCAL_OUTPUT,
        "CHAOSCONTROL_SDK_LOCAL_OUTPUT"
    );
    assert_eq!(
        chaoscontrol_sdk::ANTITHESIS_LOCAL_OUTPUT,
        "ANTITHESIS_SDK_LOCAL_OUTPUT"
    );
    let _: AntithesisRng = AntithesisRng;
    let _: AssertType = AssertionKind::Always.into();
    let _: assert::AssertType = assert::AssertType::Always;
    let tracking = assert::TrackingInfo::new();
    assert_eq!(
        tracking
            .pass_count
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    #[cfg(feature = "full")]
    let _ = assert::guidance::GuidanceType::Numeric;
    assert_eq!(
        AssertType::Reachability.to_kind(false),
        AssertionKind::Unreachable
    );

    antithesis_init();

    assert_always!(true, "compat always");
    assert_always_or_unreachable!(true, "compat aou");
    assert_sometimes!(true, "compat sometimes");
    assert_reachable!("compat reachable");
    assert_unreachable!("compat unreachable");
    assert_always_greater_than!(2, 1, "compat always greater than");
    assert_sometimes_less_than_or_equal_to!(1, 1, "compat sometimes less or equal");
    assert_always_some!({first: true, second: false}, "compat any bundle");
    assert_sometimes_all!({first: true, second: true}, "compat all bundle");

    let _ = random::random_choice(&[1, 2, 3]).expect("nonempty slice");
    let _ = random::random_choice_index(3);
    lifecycle::send_event("compat event", &json!({"ok": true}));

    let content = std::fs::read_to_string(&output).expect("local output");
    let messages = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .filter_map(|record| record.get("antithesis_assert").cloned())
        .map(|assertion| {
            assert_eq!(assertion["catalog_status"], "accepted");
            assertion["message"]
                .as_str()
                .expect("assertion message")
                .to_string()
        })
        .collect::<BTreeSet<_>>();

    let expected = [
        "compat always",
        "compat aou",
        "compat sometimes",
        "compat reachable",
        "compat unreachable",
        "compat always greater than",
        "compat sometimes less or equal",
        "compat any bundle",
        "compat all bundle",
    ];
    assert_eq!(messages.len(), expected.len());
    for message in expected {
        assert!(messages.contains(message), "missing {message}");
    }

    std::fs::remove_file(output).expect("remove local output");
}
