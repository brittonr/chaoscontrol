use chaoscontrol_evidence::{validate_assertion_summary, validate_assertion_summary_for_promotion};
use chaoscontrol_protocol::assertion_catalog::{catalog_token, AdmittedAssertion};
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_FINGERPRINT_BYTES,
    ASSERTION_IDENTITY_VERSION,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const COMPATIBILITY_ID: u32 = 7;
const SECOND_COMPATIBILITY_ID: u32 = 41;

fn descriptor(namespace: &str, key: &str, compatibility_id: u32) -> AssertionDescriptor {
    AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: namespace.to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: key.to_string(),
        },
        compatibility_id: Some(compatibility_id),
        kind: AssertionKind::Always,
        message: format!("message-{key}"),
        source_file: "src/main.rs".to_string(),
        source_line: 1,
        source_column: 1,
        guest: "guest".to_string(),
        category: "invariant".to_string(),
    }
}

fn catalog_token_for(descriptors: &[AssertionDescriptor]) -> String {
    let assertions = descriptors
        .iter()
        .map(|descriptor| {
            let fingerprint = descriptor.fingerprint().expect("fingerprint");
            (
                fingerprint,
                AdmittedAssertion {
                    descriptor: descriptor.clone(),
                    fingerprint,
                    canonical_bytes: descriptor.canonical_bytes().expect("canonical"),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    catalog_token(&assertions).to_hex()
}

fn encode_hex(bytes: &[u8]) -> String {
    chaoscontrol_protocol::assertion_identity::encode_lower_hex(bytes)
}

fn entry(descriptor: &AssertionDescriptor, token: &str) -> Value {
    json!({
        "id": descriptor.compatibility_id.unwrap_or_default(),
        "identity": {
            "descriptor": descriptor,
            "fingerprint": descriptor.fingerprint().expect("fingerprint"),
            "canonical_descriptor": encode_hex(
                &descriptor.canonical_bytes().expect("canonical descriptor")
            ),
            "catalog_tokens": [token],
        },
        "message": descriptor.message,
        "kind": "always",
        "guest": descriptor.guest,
        "category": descriptor.category,
        "verdict": "passed",
        "hit_count": 1,
        "true_count": 1,
        "false_count": 0,
    })
}

fn strict_summary(descriptors: &[AssertionDescriptor]) -> Value {
    let token = catalog_token_for(descriptors);
    json!({
        "schema": "chaoscontrol.assertion-summary.v2",
        "catalog_status": "accepted",
        "collision_safe_evidence": true,
        "assertions": descriptors
            .iter()
            .map(|descriptor| entry(descriptor, &token))
            .collect::<Vec<_>>()
    })
}

#[test]
fn accepts_collision_safe_summary_for_promotion() {
    let summary = strict_summary(&[descriptor(
        "org.example.guest",
        "stable-key",
        COMPATIBILITY_ID,
    )]);

    validate_assertion_summary(&summary).expect("compatibility validation");
    validate_assertion_summary_for_promotion(&summary).expect("promotion validation");
}

#[test]
fn accepts_null_compatibility_id_and_omits_absent_failure_details() {
    let mut without_alias = descriptor("org.example.guest", "without-alias", COMPATIBILITY_ID);
    without_alias.compatibility_id = None;
    let summary = strict_summary(std::slice::from_ref(&without_alias));

    assert_eq!(
        summary["assertions"][0]["identity"]["descriptor"]["compatibility_id"],
        Value::Null
    );
    assert_eq!(summary["assertions"][0]["id"], 0);
    assert!(summary["assertions"][0]
        .get("last_failure_details")
        .is_none());
    validate_assertion_summary_for_promotion(&summary).expect("null compatibility alias");
}

#[test]
fn rejects_present_null_identity() {
    let summary = json!([{
        "id": COMPATIBILITY_ID,
        "identity": null,
        "message": "legacy",
        "kind": "always",
        "guest": "legacy-guest",
        "category": "uncategorized",
        "verdict": "unexercised",
        "hit_count": 0,
        "true_count": 0,
        "false_count": 0
    }]);

    assert!(validate_assertion_summary(&summary).is_err());
}

#[test]
fn rejects_legacy_u32_identity_from_accepted_evidence() {
    let mut legacy = descriptor("legacy:guest", "ignored", COMPATIBILITY_ID);
    legacy.logical_key = AssertionLogicalKey::LegacyU32 {
        id: COMPATIBILITY_ID,
    };
    let summary = strict_summary(std::slice::from_ref(&legacy));

    let error = validate_assertion_summary(&summary).expect_err("legacy accepted summary");
    assert!(error.message().contains("LegacyIdentityForbidden"));
    assert!(validate_assertion_summary_for_promotion(&summary).is_err());
}

#[test]
fn rejects_mixed_legacy_and_structured_summary() {
    let descriptor = descriptor("org.example.guest", "stable-key", COMPATIBILITY_ID);
    let token = catalog_token_for(std::slice::from_ref(&descriptor));
    let mut items = vec![entry(&descriptor, &token)];
    items.push(json!({
        "id": SECOND_COMPATIBILITY_ID,
        "message": "legacy",
        "kind": "always",
        "guest": "legacy-guest",
        "category": "uncategorized",
        "verdict": "unexercised",
        "hit_count": 0,
        "true_count": 0,
        "false_count": 0
    }));

    let error = validate_assertion_summary(&Value::Array(items)).expect_err("mixed identities");
    assert!(error.message().contains("mixed legacy"));
}

#[test]
fn rejects_descriptor_fingerprint_mismatch() {
    let first = descriptor("org.example.guest", "first", COMPATIBILITY_ID);
    let second = descriptor("org.example.guest", "second", SECOND_COMPATIBILITY_ID);
    let mut summary = strict_summary(std::slice::from_ref(&first));
    summary["assertions"][0]["identity"]["fingerprint"] =
        json!(second.fingerprint().expect("second fingerprint"));

    let error = validate_assertion_summary(&summary).expect_err("fingerprint mismatch");
    assert!(error.message().contains("fingerprint mismatch"));
}

#[test]
fn rejects_report_metadata_conflict() {
    let descriptor = descriptor("org.example.guest", "stable-key", COMPATIBILITY_ID);
    let mut summary = strict_summary(std::slice::from_ref(&descriptor));
    summary["assertions"][0]["message"] = json!("conflicting message");

    let error = validate_assertion_summary(&summary).expect_err("metadata conflict");
    assert!(error.message().contains("metadata conflicts"));
}

#[test]
fn rejects_duplicate_fingerprint() {
    let descriptor = descriptor("org.example.guest", "stable-key", COMPATIBILITY_ID);
    let token = catalog_token_for(std::slice::from_ref(&descriptor));
    let summary = Value::Array(vec![entry(&descriptor, &token), entry(&descriptor, &token)]);

    let error = validate_assertion_summary(&summary).expect_err("duplicate fingerprint");
    assert!(error.message().contains("duplicate assertion fingerprint"));
}

#[test]
fn rejects_catalog_token_mismatch() {
    let descriptor = descriptor("org.example.guest", "stable-key", COMPATIBILITY_ID);
    let mut summary = strict_summary(std::slice::from_ref(&descriptor));
    summary["assertions"][0]["identity"]["catalog_tokens"] =
        json!(["00".repeat(ASSERTION_FINGERPRINT_BYTES)]);

    let error = validate_assertion_summary(&summary).expect_err("catalog token mismatch");
    assert!(error.message().contains("CatalogTokenMismatch"));
}

#[test]
fn accepts_equal_compatibility_ids_in_separate_namespaces() {
    let summary = strict_summary(&[
        descriptor("org.example.first", "stable-key", COMPATIBILITY_ID),
        descriptor("org.example.second", "stable-key", COMPATIBILITY_ID),
    ]);

    validate_assertion_summary_for_promotion(&summary).expect("namespace separation");
}

#[test]
fn rejects_compatibility_alias_conflict_in_one_namespace() {
    let summary = strict_summary(&[
        descriptor("org.example.same", "first", COMPATIBILITY_ID),
        descriptor("org.example.same", "second", COMPATIBILITY_ID),
    ]);

    let error = validate_assertion_summary(&summary).expect_err("compatibility alias conflict");
    assert!(error.message().contains("CompatibilityAliasConflict"));
}

#[test]
fn rejects_unknown_report_fields() {
    let descriptor = descriptor("org.example.guest", "stable-key", COMPATIBILITY_ID);
    let mut summary = strict_summary(std::slice::from_ref(&descriptor));
    summary["assertions"][0]["unexpected"] = json!(true);

    let error = validate_assertion_summary(&summary).expect_err("unknown field");
    assert!(error.message().contains("unknown field"));
}

#[test]
fn rejects_spoofed_verdict_kind_counts_and_unbounded_details() {
    let descriptor = descriptor("org.example.guest", "stable-key", COMPATIBILITY_ID);
    let mut verdict = strict_summary(std::slice::from_ref(&descriptor));
    verdict["assertions"][0]["verdict"] = json!("failed");
    assert!(validate_assertion_summary(&verdict).is_err());

    let invalid_reachable = json!([{
        "id": COMPATIBILITY_ID,
        "message": "reachable",
        "kind": "reachable",
        "guest": "guest",
        "category": "branch",
        "verdict": "passed",
        "hit_count": 1,
        "true_count": 0,
        "false_count": 1
    }]);
    assert!(validate_assertion_summary(&invalid_reachable).is_err());

    let mut details = strict_summary(std::slice::from_ref(&descriptor));
    details["assertions"][0]["last_failure_details"] = json!("x"
        .repeat(chaoscontrol_protocol::assertion_identity::MAX_ASSERTION_EVENT_DETAILS_BYTES + 1));
    assert!(validate_assertion_summary(&details).is_err());
}
