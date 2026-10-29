#![cfg(feature = "full")]

use chaoscontrol_sdk::assert::{assert_raw, AssertType};
use serde_json::{json, Value};

#[derive(Clone, Copy)]
struct Site {
    id: &'static str,
    message: &'static str,
    class: &'static str,
    function: &'static str,
    file: &'static str,
    line: u32,
    column: u32,
    must_hit: bool,
    kind: AssertType,
    display: &'static str,
}

const ORIGINAL: Site = Site {
    id: "org.example.sparse-a:e3870d15ed661eaa",
    message: "raw identity invariant",
    class: "adapter",
    function: "adapter::check",
    file: "src/adapter.rs",
    line: 12,
    column: 3,
    must_hit: true,
    kind: AssertType::Always,
    display: "Always",
};

fn emit(site: Site, condition: bool, hit: bool, attempt: &str) {
    assert_raw(
        condition,
        site.message.to_owned(),
        &json!({"attempt": attempt}),
        site.class.to_owned(),
        site.function.to_owned(),
        site.file.to_owned(),
        site.line,
        site.column,
        hit,
        site.must_hit,
        site.kind,
        site.display.to_owned(),
        site.id.to_owned(),
    );
}

// r[verify chaoscontrol.raw_registration.exact_reuse]
#[test]
fn conflicting_raw_calls_cannot_steal_identity_or_outcome_samples() {
    let output =
        std::env::temp_dir().join(format!("cc-raw-registration-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&output);
    std::env::set_var("CHAOSCONTROL_SDK_LOCAL_OUTPUT", &output);
    let conflicts = [
        Site {
            message: "changed message",
            ..ORIGINAL
        },
        Site {
            class: "other",
            ..ORIGINAL
        },
        Site {
            function: "other::check",
            ..ORIGINAL
        },
        Site {
            file: "src/other.rs",
            ..ORIGINAL
        },
        Site {
            line: 13,
            ..ORIGINAL
        },
        Site {
            column: 4,
            ..ORIGINAL
        },
        Site {
            must_hit: false,
            ..ORIGINAL
        },
        Site {
            kind: AssertType::Sometimes,
            ..ORIGINAL
        },
        Site {
            display: "AlwaysOrUnreachable",
            ..ORIGINAL
        },
    ];
    emit(ORIGINAL, false, false, "declare");
    emit(ORIGINAL, false, false, "duplicate");
    for conflict in conflicts {
        emit(conflict, false, false, "conflicting declaration");
    }
    chaoscontrol_sdk::chaoscontrol_init();
    for conflict in conflicts {
        emit(conflict, false, true, "rejected");
    }
    emit(ORIGINAL, true, true, "original pass");
    emit(ORIGINAL, false, true, "original fail");
    emit(ORIGINAL, true, true, "repeated pass");
    emit(
        Site {
            id: "late",
            ..ORIGINAL
        },
        false,
        true,
        "late",
    );

    let content = std::fs::read_to_string(&output).unwrap();
    let records = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let descriptors = records
        .iter()
        .filter_map(|record| record.get("chaoscontrol_assertion_catalog"))
        .filter(|record| record["record"] == "descriptor")
        .collect::<Vec<_>>();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(
        descriptors[0]["descriptor"]["logical_key"]["key"],
        ORIGINAL.id
    );
    let assertions = records
        .iter()
        .filter_map(|record| record.get("antithesis_assert"))
        .collect::<Vec<_>>();
    let hits = assertions
        .iter()
        .filter(|record| record["hit"] == true)
        .copied()
        .collect::<Vec<_>>();
    let outcomes = hits
        .iter()
        .map(|record| {
            (
                record["condition"].as_bool().unwrap(),
                record["details"]["attempt"].as_str().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let expected = if cfg!(feature = "sparse_emission") {
        vec![(true, "original pass"), (false, "original fail")]
    } else {
        vec![
            (true, "original pass"),
            (false, "original fail"),
            (true, "repeated pass"),
        ]
    };
    assert_eq!(
        outcomes, expected,
        "conflicting calls must not become original observations or consume samples"
    );
    for assertion in assertions {
        assert_eq!(assertion["id"], ORIGINAL.id);
        assert_eq!(
            assertion["assertion_fingerprint"],
            descriptors[0]["fingerprint"]
        );
        assert_eq!(assertion["must_hit"], true);
        assert_eq!(assertion["display_type"], "Always");
        assert_eq!(assertion["location"]["class"], ORIGINAL.class);
        assert_eq!(assertion["location"]["function"], ORIGINAL.function);
        assert_eq!(assertion["location"]["file"], ORIGINAL.file);
        assert_eq!(assertion["location"]["begin_line"], ORIGINAL.line);
        assert_eq!(assertion["location"]["begin_column"], ORIGINAL.column);
    }
    std::fs::remove_file(output).unwrap();
}
