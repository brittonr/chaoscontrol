#![cfg(feature = "full")]

use chaoscontrol_sdk::assert::{emit_registered_assertion, location_id, AssertionKind};
use chaoscontrol_sdk::prelude::*;
use serde_json::{json, Value};

const MESSAGE: &str = "colliding source locations";

fn left_site() {
    cc_assert_sometimes_stable!(
        "org.example.sparse-a",
        "e3870d15ed661eaa",
        "guest",
        "operation",
        true,
        MESSAGE,
        &json!({"site": "left"})
    );
}
fn right_site() {
    cc_assert_sometimes_stable!(
        "org.example.sparse-b",
        "05d3992156bd4061",
        "guest",
        "operation",
        false,
        MESSAGE,
        &json!({"site": "right"})
    );
}

// r[verify chaoscontrol.assertion_locations.identity]
// r[verify chaoscontrol.assertion_locations.preservation]
#[test]
fn colliding_sites_keep_their_own_declaration_and_hit_locations() {
    let alias = location_id("org.example.sparse-a:e3870d15ed661eaa");
    assert_eq!(alias, location_id("org.example.sparse-b:05d3992156bd4061"));
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-location-identity-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var(chaoscontrol_sdk::LOCAL_OUTPUT, &output);
    left_site();
    right_site();
    emit_registered_assertion(
        AssertionKind::Sometimes,
        true,
        alias,
        MESSAGE,
        &json!({"site": "ambiguous"}),
    );
    chaoscontrol_sdk::assert::assert_raw_with_key(
        AssertionKind::Sometimes,
        true,
        "org.example.missing",
        "missing",
        MESSAGE,
        &json!({"site": "undeclared"}),
    );
    let content = std::fs::read_to_string(&output).expect("local output");
    let records: Vec<Value> = content
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON record"))
        .collect();
    let assertions: Vec<_> = records
        .iter()
        .filter_map(|record| record.get("antithesis_assert"))
        .collect();
    assert_eq!(
        assertions
            .iter()
            .filter(|record| record["hit"] == true)
            .count(),
        2,
        "rejected calls cannot become hits"
    );
    let mut fingerprints = std::collections::BTreeSet::new();
    for (site, namespace, function, condition) in [
        ("left", "org.example.sparse-a", "left_site", true),
        ("right", "org.example.sparse-b", "right_site", false),
    ] {
        let hit = assertions
            .iter()
            .find(|record| record["hit"] == true && record["details"]["site"] == site)
            .expect("site hit");
        let declaration = assertions
            .iter()
            .find(|record| {
                record["hit"] == false
                    && record["assertion_fingerprint"] == hit["assertion_fingerprint"]
            })
            .expect("site declaration");
        let descriptor = records
            .iter()
            .filter_map(|record| record.get("chaoscontrol_assertion_catalog"))
            .filter(|record| record["record"] == "descriptor")
            .map(|record| &record["descriptor"])
            .find(|descriptor| descriptor["namespace"] == namespace)
            .expect("descriptor");
        for assertion in [*hit, *declaration] {
            assert_eq!(assertion["catalog_status"], "accepted");
            assert_eq!(assertion["location"]["class"], module_path!());
            assert_eq!(assertion["location"]["file"], file!());
            assert_eq!(
                assertion["location"]["function"],
                format!("{}::{function}", module_path!())
            );
            assert_eq!(
                assertion["location"]["begin_line"],
                descriptor["source_line"]
            );
            assert_eq!(
                assertion["location"]["begin_column"],
                descriptor["source_column"]
            );
            assert_eq!(assertion["must_hit"], true);
            assert_eq!(assertion["display_type"], "Sometimes");
        }
        assert_eq!(hit["condition"], condition);
        assert_eq!(hit["catalog_token"], declaration["catalog_token"]);
        fingerprints.insert(hit["assertion_fingerprint"].as_str().expect("fingerprint"));
    }
    assert_eq!(fingerprints.len(), 2);
    std::fs::remove_file(output).expect("remove local output");
}
