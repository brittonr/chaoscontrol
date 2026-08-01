use chaoscontrol_evidence::{check_sdk_assertion_quality_report, summarize_sdk_local_jsonl};
use chaoscontrol_protocol::assertion_catalog::{token_for_descriptors, ASSERTION_CATALOG_VERSION};
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, AssertionLogicalKey,
    ASSERTION_IDENTITY_VERSION,
};
use serde_json::{json, Value};

const COMPATIBILITY_ID: u32 = 909;
const SOURCE_LINE: u32 = 33;
const SOURCE_COLUMN: u32 = 5;
const EVIDENCE_CLASS: &str = "instrumentation-dry-run";
const EXPECTED_MIXED_ASSERTION_SITES: u64 = 2;

fn descriptor(namespace: &str, kind: AssertionKind, message: &str) -> AssertionDescriptor {
    AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: namespace.to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: "verdict-key".to_string(),
        },
        compatibility_id: Some(COMPATIBILITY_ID),
        kind,
        message: message.to_string(),
        source_file: "src/assertions.rs".to_string(),
        source_line: SOURCE_LINE,
        source_column: SOURCE_COLUMN,
        guest: "raft".to_string(),
        category: "invariant".to_string(),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    chaoscontrol_protocol::assertion_identity::encode_lower_hex(bytes)
}

fn render(lines: &[Value]) -> String {
    let mut output = String::new();
    for line in lines {
        output.push_str(&serde_json::to_string(line).expect("JSON line"));
        output.push('\n');
    }
    output
}

fn catalog_jsonl(descriptor: &AssertionDescriptor) -> String {
    let fingerprint = descriptor.fingerprint().expect("fingerprint");
    let token = token_for_descriptors(std::slice::from_ref(descriptor)).expect("token");
    render(&[
        json!({"chaoscontrol_assertion_catalog": {
            "record": "begin", "catalog_version": ASSERTION_CATALOG_VERSION,
            "expected_descriptors": 1, "valid": true
        }}),
        json!({"chaoscontrol_assertion_catalog": {
            "record": "descriptor", "fingerprint": fingerprint,
            "descriptor": descriptor, "canonical_descriptor": encode_hex(
                &descriptor.canonical_bytes().expect("canonical descriptor")
            )
        }}),
        json!({"chaoscontrol_assertion_catalog": {
            "record": "complete", "catalog_version": ASSERTION_CATALOG_VERSION,
            "descriptor_count": 1, "catalog_token": token
        }}),
        json!({"antithesis_setup": {"status": "complete", "details": {}}}),
    ])
}

fn event_line(descriptor: &AssertionDescriptor, condition: bool) -> String {
    let fingerprint = descriptor.fingerprint().expect("fingerprint");
    let token = token_for_descriptors(std::slice::from_ref(descriptor)).expect("token");
    let (assert_type, must_hit) = match descriptor.kind {
        AssertionKind::Always => ("always", false),
        AssertionKind::Sometimes => ("sometimes", true),
        AssertionKind::Reachable | AssertionKind::Unreachable => ("reachability", true),
    };
    render(&[json!({"antithesis_assert": {
        "assert_type": assert_type, "condition": condition, "hit": true,
        "must_hit": must_hit,
        "id": descriptor.compatibility_id
            .map(|value| format!("{value:08x}"))
            .unwrap_or_else(|| fingerprint.to_hex()),
        "message": descriptor.message, "display_type": assert_type,
        "details": {"guest": descriptor.guest, "category": descriptor.category},
        "identity_version": ASSERTION_IDENTITY_VERSION, "catalog_token": token,
        "assertion_fingerprint": fingerprint, "catalog_status": "accepted"
    }})])
}

#[test]
fn sometimes_false_then_true_has_a_passing_assertion_verdict() {
    let value = descriptor(
        "stable:sometimes",
        AssertionKind::Sometimes,
        "eventually succeeds",
    );
    let mut content = catalog_jsonl(&value);
    content.push_str(&event_line(&value, false));
    content.push_str(&event_line(&value, true));
    let report = summarize_sdk_local_jsonl(&content, EVIDENCE_CLASS, None).expect("report");
    assert_eq!(report["failed_assertions"], 0);
    assert!(report["sometimes_without_success"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(check_sdk_assertion_quality_report(&report).unwrap().passed);
}

#[test]
fn strict_unreachable_round_trip_treats_no_hit_as_success_and_hit_as_failure() {
    let value = descriptor(
        "stable:unreachable",
        AssertionKind::Unreachable,
        "forbidden branch remains unreachable",
    );
    let no_hit = catalog_jsonl(&value);
    let report = summarize_sdk_local_jsonl(&no_hit, EVIDENCE_CLASS, None).expect("no-hit report");
    assert_eq!(report["unobserved_assertion_count"], 0);
    assert_eq!(report["failed_assertions"], 0);
    assert!(check_sdk_assertion_quality_report(&report).unwrap().passed);

    let mut hit = no_hit;
    hit.push_str(&event_line(&value, false));
    let report = summarize_sdk_local_jsonl(&hit, EVIDENCE_CLASS, None).expect("hit report");
    assert_eq!(report["failed_assertions"], 1);
    assert!(!check_sdk_assertion_quality_report(&report).unwrap().passed);
}

#[test]
fn fingerprint_hex_is_the_exact_event_id_without_a_compatibility_alias() {
    let mut value = descriptor("stable:no-alias", AssertionKind::Always, "strict");
    value.compatibility_id = None;
    let mut content = catalog_jsonl(&value);
    content.push_str(&event_line(&value, true));
    summarize_sdk_local_jsonl(&content, EVIDENCE_CLASS, None).expect("fallback ID report");

    let mut event: Value = serde_json::from_str(event_line(&value, true).trim()).expect("event");
    event["antithesis_assert"]["id"] = json!("arbitrary");
    let mut invalid = catalog_jsonl(&value);
    invalid.push_str(&render(&[event]));
    assert!(summarize_sdk_local_jsonl(&invalid, EVIDENCE_CLASS, None).is_err());
}

#[test]
fn legacy_fingerprint_alias_does_not_mutate_the_strict_site() {
    let value = descriptor("stable:mixed", AssertionKind::Always, "strict");
    let fingerprint = value.fingerprint().expect("fingerprint").to_hex();
    let mut content = catalog_jsonl(&value);
    content.push_str(&format!(
        "{{\"antithesis_assert\":{{\"assert_type\":\"always\",\"condition\":true,\"hit\":true,\"id\":\"{fingerprint}\",\"message\":\"legacy\",\"details\":{{\"category\":\"legacy\"}}}}}}\n"
    ));
    let report = summarize_sdk_local_jsonl(&content, EVIDENCE_CLASS, None).expect("mixed report");
    assert_eq!(report["catalog_status"], "legacy-ambiguous");
    assert_eq!(
        report["registered_assertions"],
        EXPECTED_MIXED_ASSERTION_SITES
    );
    let strict = report["assertion_coverage"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["identity"].is_object())
        .expect("strict site");
    assert_eq!(strict["observed_hits"], 0);
}
