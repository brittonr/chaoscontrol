#![cfg(feature = "full")]

use chaoscontrol_sdk::assert::{location_id, AssertionKind};
use chaoscontrol_sdk::prelude::*;
use serde_json::{json, Value};

const LEFT: (&str, &str) = ("org.example.sparse-a", "e3870d15ed661eaa");
const RIGHT: (&str, &str) = ("org.example.sparse-b", "05d3992156bd4061");
const MESSAGE: &str = "colliding JSON guidance";

// r[verify chaoscontrol.stable_json_guidance.binding]
// r[verify chaoscontrol.stable_json_guidance.rejection]
#[test]
fn stable_json_guidance_retains_colliding_sites_and_rejects_mismatches() {
    assert_eq!(
        location_id("org.example.sparse-a:e3870d15ed661eaa"),
        location_id("org.example.sparse-b:05d3992156bd4061")
    );
    let output = std::env::temp_dir().join(format!(
        "chaoscontrol-json-guidance-identity-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    std::env::set_var(chaoscontrol_sdk::LOCAL_OUTPUT, &output);
    chaoscontrol_init();
    cc_assert_sometimes_stable!(
        "org.example.sparse-a",
        "e3870d15ed661eaa",
        "guest",
        "operation",
        true,
        MESSAGE,
        &json!({"site": "left"})
    );
    cc_assert_sometimes_stable!(
        "org.example.sparse-b",
        "05d3992156bd4061",
        "guest",
        "operation",
        true,
        MESSAGE,
        &json!({"site": "right"})
    );
    for ((namespace, key), site, maximize, score) in
        [(LEFT, "left", true, 7), (RIGHT, "right", false, -4)]
    {
        cc_guidance_json!(
            namespace,
            key,
            AssertionKind::Sometimes,
            MESSAGE,
            maximize,
            json!({"site": site, "score": score})
        );
    }
    for (namespace, key, kind, message) in [
        (
            "org.example.missing",
            LEFT.1,
            AssertionKind::Sometimes,
            MESSAGE,
        ),
        (LEFT.0, "missing", AssertionKind::Sometimes, MESSAGE),
        (LEFT.0, LEFT.1, AssertionKind::Always, MESSAGE),
        (LEFT.0, LEFT.1, AssertionKind::Sometimes, "wrong message"),
    ] {
        cc_guidance_json!(
            namespace,
            key,
            kind,
            message,
            true,
            json!({"site": "rejected"})
        );
    }
    let content = std::fs::read_to_string(&output).expect("local output");
    let records: Vec<Value> = content
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON record"))
        .collect();
    let guidance: Vec<_> = records
        .iter()
        .filter_map(|record| record.get("antithesis_guidance"))
        .collect();
    assert_eq!(
        guidance.len(),
        2,
        "both admitted sites emit and mismatches do not"
    );
    let mut fingerprints = std::collections::BTreeSet::new();
    for (site, maximize, score) in [("left", true, 7), ("right", false, -4)] {
        let assertion = records
            .iter()
            .filter_map(|record| record.get("antithesis_assert"))
            .find(|record| record["hit"] == true && record["details"]["site"] == site)
            .expect("accepted assertion hit");
        let observation = guidance
            .iter()
            .find(|record| record["guidance_data"]["site"] == site)
            .expect("guidance for site");
        assert_eq!(assertion["catalog_status"], "accepted");
        assert_eq!(observation["catalog_status"], "accepted");
        assert_eq!(
            observation["assertion_fingerprint"],
            assertion["assertion_fingerprint"]
        );
        assert_eq!(observation["catalog_token"], assertion["catalog_token"]);
        assert_eq!(
            observation["guidance_data"],
            json!({"site": site, "score": score})
        );
        assert_eq!(observation["maximize"], maximize);
        assert_eq!(observation["hit"], true);
        assert_eq!(observation["guidance_type"], "json");
        assert_eq!(observation["message"], MESSAGE);
        assert!(observation["location"]["function"]
            .as_str()
            .expect("caller function")
            .contains("stable_json_guidance_retains_colliding_sites_and_rejects_mismatches"));
        fingerprints.insert(
            assertion["assertion_fingerprint"]
                .as_str()
                .expect("fingerprint"),
        );
    }
    assert_eq!(fingerprints.len(), 2, "distinct accepted identities");
    std::fs::remove_file(output).expect("remove local output");
}
