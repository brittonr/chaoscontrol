#![cfg(feature = "full")]

use chaoscontrol_sdk::assert::{emit_registered_assertion, location_id, AssertionKind};
use chaoscontrol_sdk::prelude::*;
use serde_json::Value;
use std::cell::Cell;

struct CountedDetails<'a> {
    condition: bool,
    serializations: &'a Cell<usize>,
}

impl serde::Serialize for CountedDetails<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.serializations.set(self.serializations.get() + 1);
        serializer.serialize_bool(self.condition)
    }
}

// Fixed FNV alias collision, admitted because the namespaces are distinct.
const RIGHT_ALIAS: u32 = location_id("org.example.sparse-b:05d3992156bd4061");

fn left(condition: bool, serializations: &Cell<usize>) {
    cc_assert_sometimes_stable!(
        "org.example.sparse-a",
        "e3870d15ed661eaa",
        "guest",
        "operation",
        condition,
        "sampling left",
        &CountedDetails {
            condition,
            serializations
        }
    );
}

fn right(condition: bool, serializations: &Cell<usize>) {
    cc_assert_sometimes_stable!(
        "org.example.sparse-b",
        "05d3992156bd4061",
        "guest",
        "operation",
        condition,
        "sampling right",
        &CountedDetails {
            condition,
            serializations
        }
    );
}

// r[verify chaoscontrol.sparse_sampling.identity]
// r[verify chaoscontrol.sparse_sampling.outcomes]
// r[verify chaoscontrol.stable_assertion_details.serialize]
#[test]
fn alias_collisions_do_not_share_outcome_samples() {
    assert_eq!(
        location_id("org.example.sparse-a:e3870d15ed661eaa"),
        RIGHT_ALIAS
    );
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-sampling-identity-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var(chaoscontrol_sdk::LOCAL_OUTPUT, &output);
    chaoscontrol_init();
    let unresolved_serializations = Cell::new(0);
    let left_serializations = Cell::new(0);
    let right_serializations = Cell::new(0);
    for condition in [true, false] {
        chaoscontrol_sdk::assert::assert_raw_with_key(
            AssertionKind::Sometimes,
            condition,
            "org.example.sparse-a",
            "e3870d15ed661eaa",
            "undeclared message",
            &CountedDetails {
                condition,
                serializations: &unresolved_serializations,
            },
        );
        emit_registered_assertion(
            AssertionKind::Sometimes,
            condition,
            RIGHT_ALIAS,
            "undeclared message",
            &CountedDetails {
                condition,
                serializations: &unresolved_serializations,
            },
        );
    }
    for condition in [true, false, true, false] {
        left(condition, &left_serializations);
        emit_registered_assertion(
            AssertionKind::Sometimes,
            condition,
            RIGHT_ALIAS,
            "sampling right",
            &CountedDetails {
                condition,
                serializations: &right_serializations,
            },
        );
        right(condition, &right_serializations);
    }
    assert_eq!(
        unresolved_serializations.get(),
        0,
        "unresolved sites must not serialize"
    );
    assert_eq!(
        left_serializations.get(),
        if cfg!(feature = "sparse_emission") {
            2
        } else {
            4
        }
    );
    assert_eq!(
        right_serializations.get(),
        if cfg!(feature = "sparse_emission") {
            2
        } else {
            8
        }
    );
    let content = std::fs::read_to_string(&output).expect("local output");
    let assertions: Vec<Value> = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON line"))
        .filter_map(|value| value.get("antithesis_assert").cloned())
        .collect();
    let mut fingerprints = std::collections::BTreeSet::new();
    for (message, evaluations_per_outcome) in [("sampling left", 2), ("sampling right", 4)] {
        let site: Vec<_> = assertions
            .iter()
            .filter(|value| value["message"] == message)
            .collect();
        let declarations: Vec<_> = site.iter().filter(|value| value["hit"] == false).collect();
        assert_eq!(declarations.len(), 1, "declarations are not sampled");
        let fingerprint = declarations[0]["assertion_fingerprint"]
            .as_str()
            .expect("fingerprint");
        fingerprints.insert(fingerprint.to_string());
        for condition in [true, false] {
            let hits: Vec<_> = site
                .iter()
                .filter(|value| value["hit"] == true && value["condition"] == condition)
                .collect();
            let expected = if cfg!(feature = "sparse_emission") {
                1
            } else {
                evaluations_per_outcome
            };
            assert_eq!(hits.len(), expected, "{message}/{condition} outcomes");
            for hit in hits {
                assert_eq!(hit["catalog_status"], "accepted");
                assert_eq!(hit["assertion_fingerprint"], fingerprint);
                assert_eq!(hit["details"], condition);
            }
        }
    }
    assert_eq!(fingerprints.len(), 2, "the admitted sites remain distinct");
    std::fs::remove_file(output).expect("remove output");
}
