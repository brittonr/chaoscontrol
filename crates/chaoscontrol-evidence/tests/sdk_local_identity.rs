use chaoscontrol_evidence::{check_sdk_assertion_quality_report, summarize_sdk_local_jsonl};
use chaoscontrol_protocol::assertion_catalog::token_for_descriptors;
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, AssertionLogicalKey,
    ASSERTION_IDENTITY_VERSION,
};
use serde_json::{json, Value};

const COMPATIBILITY_ID: u32 = 909;
const SOURCE_LINE: u32 = 33;
const SOURCE_COLUMN: u32 = 5;
const EVIDENCE_CLASS: &str = "instrumentation-dry-run";

fn descriptor(namespace: &str, guest: &str) -> AssertionDescriptor {
    AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: namespace.to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: "leader-unique".to_string(),
        },
        compatibility_id: Some(COMPATIBILITY_ID),
        kind: AssertionKind::Always,
        message: "leader remains unique".to_string(),
        source_file: "src/assertions.rs".to_string(),
        source_line: SOURCE_LINE,
        source_column: SOURCE_COLUMN,
        guest: guest.to_string(),
        category: "invariant".to_string(),
    }
}

fn catalog_jsonl(descriptors: &[AssertionDescriptor]) -> String {
    let token = token_for_descriptors(descriptors).unwrap_or(AssertionFingerprint::ZERO);
    let mut lines = vec![json!({
        "chaoscontrol_assertion_catalog": {
            "record": "begin",
            "catalog_version": ASSERTION_IDENTITY_VERSION,
            "expected_descriptors": descriptors.len(),
            "valid": true
        }
    })];
    for descriptor in descriptors {
        lines.push(json!({
            "chaoscontrol_assertion_catalog": {
                "record": "descriptor",
                "fingerprint": descriptor.fingerprint().expect("fingerprint"),
                "descriptor": descriptor,
                "canonical_descriptor": encode_hex(
                    &descriptor.canonical_bytes().expect("canonical descriptor")
                )
            }
        }));
    }
    lines.push(json!({
        "chaoscontrol_assertion_catalog": {
            "record": "complete",
            "catalog_version": ASSERTION_IDENTITY_VERSION,
            "descriptor_count": descriptors.len(),
            "catalog_token": token
        }
    }));
    render_lines(&lines)
}

fn event_line(
    descriptor: &AssertionDescriptor,
    message: &str,
    token_override: Option<Value>,
) -> String {
    let token = token_for_descriptors(core::slice::from_ref(descriptor)).expect("catalog token");
    let value = json!({
        "antithesis_assert": {
            "assert_type": "always",
            "condition": true,
            "hit": true,
            "must_hit": false,
            "id": format!("{COMPATIBILITY_ID:08x}"),
            "message": message,
            "display_type": "always",
            "details": {"guest": descriptor.guest, "category": descriptor.category},
            "identity_version": ASSERTION_IDENTITY_VERSION,
            "catalog_token": token_override.unwrap_or_else(|| json!(token)),
            "assertion_fingerprint": descriptor.fingerprint().expect("fingerprint"),
            "catalog_status": "accepted"
        }
    });
    format!("{}\n", serde_json::to_string(&value).expect("event JSON"))
}

fn setup_line() -> &'static str {
    "{\"antithesis_setup\":{\"status\":\"complete\",\"details\":{}}}\n"
}

fn render_lines(lines: &[Value]) -> String {
    let mut output = String::new();
    for line in lines {
        output.push_str(&serde_json::to_string(line).expect("JSON line"));
        output.push('\n');
    }
    output
}

fn encode_hex(bytes: &[u8]) -> String {
    use core::fmt::Write;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("write hex");
    }
    output
}

#[test]
fn strict_local_catalog_is_idempotent_and_aggregates_exact_events() {
    let value = descriptor("stable:raft", "raft");
    let mut content = catalog_jsonl(&[value.clone(), value.clone()]);
    content.push_str(setup_line());
    let event = event_line(&value, &value.message, None);
    content.push_str(&event);
    content.push_str(&event);
    let report =
        summarize_sdk_local_jsonl(&content, EVIDENCE_CLASS, None).expect("strict local report");
    assert_eq!(report["catalog_status"], "accepted");
    assert_eq!(report["collision_safe_evidence"], true);
    assert_eq!(report["registered_assertions"], 1);
    assert_eq!(report["assertion_coverage"][0]["observed_hits"], 2);
    assert!(
        check_sdk_assertion_quality_report(&report)
            .expect("quality gate")
            .passed
    );
}

#[test]
fn local_catalog_metadata_conflicts_are_rejected() {
    let base = descriptor("stable:raft", "raft");
    let mut cases = Vec::new();
    let mut message = base.clone();
    message.message = "different message".to_string();
    cases.push(message);
    let mut source = base.clone();
    source.source_line = SOURCE_LINE + 1;
    cases.push(source);
    let mut guest = base.clone();
    guest.guest = "other".to_string();
    cases.push(guest);
    let mut category = base.clone();
    category.category = "recovery".to_string();
    cases.push(category);
    let mut kind = base.clone();
    kind.kind = AssertionKind::Sometimes;
    cases.push(kind);
    for conflict in cases {
        let content = catalog_jsonl(&[base.clone(), conflict]);
        assert!(summarize_sdk_local_jsonl(&content, EVIDENCE_CLASS, None).is_err());
    }
}

#[test]
fn local_events_reject_pre_catalog_unknown_token_and_message_spoofing() {
    let value = descriptor("stable:raft", "raft");
    assert!(summarize_sdk_local_jsonl(
        &event_line(&value, &value.message, None),
        EVIDENCE_CLASS,
        None
    )
    .is_err());

    let mut unknown = catalog_jsonl(core::slice::from_ref(&value));
    unknown.push_str(&event_line(
        &value,
        &value.message,
        Some(json!("00".repeat(32))),
    ));
    assert!(summarize_sdk_local_jsonl(&unknown, EVIDENCE_CLASS, None).is_err());

    let mut spoof = catalog_jsonl(core::slice::from_ref(&value));
    spoof.push_str(&event_line(&value, "spoofed message", None));
    assert!(summarize_sdk_local_jsonl(&spoof, EVIDENCE_CLASS, None).is_err());
}

#[test]
fn cross_namespace_compatibility_ids_remain_separate() {
    let raft = descriptor("stable:raft", "raft");
    let redb = descriptor("stable:redb", "redb");
    let token = token_for_descriptors(&[raft.clone(), redb.clone()]).expect("combined token");
    let mut content = catalog_jsonl(&[raft.clone(), redb.clone()]);
    content.push_str(&event_line(&raft, &raft.message, Some(json!(token))));
    content.push_str(&event_line(&redb, &redb.message, Some(json!(token))));
    let report = summarize_sdk_local_jsonl(&content, EVIDENCE_CLASS, None)
        .expect("namespace-separated report");
    assert_eq!(report["registered_assertions"], 2);
    assert_ne!(
        report["assertion_coverage"][0]["fingerprint"],
        report["assertion_coverage"][1]["fingerprint"]
    );
}

#[test]
fn legacy_duplicate_metadata_and_promotion_fail_closed() {
    let first = "{\"antithesis_assert\":{\"assert_type\":\"always\",\"condition\":true,\"hit\":true,\"id\":\"1\",\"message\":\"first\",\"details\":{}}}\n";
    let conflict = "{\"antithesis_assert\":{\"assert_type\":\"always\",\"condition\":true,\"hit\":true,\"id\":\"1\",\"message\":\"second\",\"details\":{}}}\n";
    assert!(
        summarize_sdk_local_jsonl(&format!("{first}{conflict}"), EVIDENCE_CLASS, None).is_err()
    );

    let report =
        summarize_sdk_local_jsonl(first, EVIDENCE_CLASS, None).expect("legacy diagnostic report");
    assert_eq!(report["catalog_status"], "legacy-ambiguous");
    assert_eq!(report["collision_safe_evidence"], false);
    assert!(
        !check_sdk_assertion_quality_report(&report)
            .expect("legacy quality gate")
            .passed
    );
}

#[test]
fn duplicate_and_unknown_catalog_fields_are_rejected() {
    let duplicate = "{\"chaoscontrol_assertion_catalog\":{\"record\":\"begin\",\"catalog_version\":1,\"catalog_version\":1,\"expected_descriptors\":0,\"valid\":true}}\n";
    let unknown = "{\"chaoscontrol_assertion_catalog\":{\"record\":\"begin\",\"catalog_version\":1,\"expected_descriptors\":0,\"valid\":true,\"extra\":false}}\n";
    assert!(summarize_sdk_local_jsonl(duplicate, EVIDENCE_CLASS, None).is_err());
    assert!(summarize_sdk_local_jsonl(unknown, EVIDENCE_CLASS, None).is_err());
}
