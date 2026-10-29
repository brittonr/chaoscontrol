use chaoscontrol_evidence::{validate_assertion_summary, validate_assertion_summary_for_promotion};
use chaoscontrol_protocol::admission::{catalog_token, AdmittedAssertion};
use chaoscontrol_protocol::identity::{
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
    chaoscontrol_protocol::identity::encode_lower_hex(bytes)
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
        "kind": match descriptor.kind {
            AssertionKind::Always => "always",
            AssertionKind::Sometimes => "sometimes",
            AssertionKind::Reachable => "reachable",
            AssertionKind::Unreachable => "unreachable",
        },
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

fn summary_with_observations(
    kind: AssertionKind,
    must_hit: Option<bool>,
    true_count: u64,
    false_count: u64,
    verdict: &str,
) -> Value {
    let mut descriptor = descriptor("org.example.guest", "must-hit", COMPATIBILITY_ID);
    descriptor.kind = kind;
    let mut summary = strict_summary(&[descriptor]);
    let assertion = &mut summary["assertions"][0];
    assertion["hit_count"] = json!(true_count + false_count);
    assertion["true_count"] = json!(true_count);
    assertion["false_count"] = json!(false_count);
    assertion["verdict"] = json!(verdict);
    if let Some(must_hit) = must_hit {
        assertion["must_hit"] = json!(must_hit);
    }
    summary
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
    let mut summary = strict_summary(std::slice::from_ref(&without_alias));
    summary["assertions"][0]["identity"]["descriptor"]["compatibility_id"] = Value::Null;

    assert!(summary["assertions"][0]["identity"]["descriptor"]
        .get("compatibility_id")
        .is_some());
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
    details["assertions"][0]["last_failure_details"] =
        json!("x".repeat(chaoscontrol_protocol::identity::MAX_ASSERTION_EVENT_DETAILS_BYTES + 1));
    assert!(validate_assertion_summary(&details).is_err());
}

// r[verify chaoscontrol.assertion_reports.must_hit_verdict]
#[test]
fn zero_hit_verdicts_follow_optional_must_hit_except_for_unreachable() {
    for kind in [
        AssertionKind::Always,
        AssertionKind::Sometimes,
        AssertionKind::Reachable,
        AssertionKind::Unreachable,
    ] {
        for (must_hit, expected) in [
            (None, "unexercised"),
            (Some(true), "failed"),
            (Some(false), "passed"),
        ] {
            let expected = if kind == AssertionKind::Unreachable {
                "passed"
            } else {
                expected
            };
            let mut summary = summary_with_observations(kind, must_hit, 0, 0, expected);
            validate_assertion_summary(&summary).expect("valid zero-hit verdict");
            validate_assertion_summary_for_promotion(&summary)
                .expect("valid must-hit metadata preserves promotion");
            for contradictory in ["passed", "failed", "unexercised"] {
                if contradictory == expected {
                    continue;
                }
                summary["assertions"][0]["verdict"] = json!(contradictory);
                assert!(
                    validate_assertion_summary(&summary).is_err(),
                    "{kind:?}/{must_hit:?} cannot report {contradictory} at zero hits"
                );
            }
        }
    }
}

// r[verify chaoscontrol.assertion_reports.must_hit_verdict]
#[test]
fn observed_verdicts_and_invalid_kind_counts_ignore_must_hit() {
    for (kind, true_count, false_count, expected) in [
        (AssertionKind::Always, 2, 0, Some("passed")),
        (AssertionKind::Always, 0, 2, Some("failed")),
        (AssertionKind::Always, 1, 1, Some("failed")),
        (AssertionKind::Sometimes, 2, 0, Some("passed")),
        (AssertionKind::Sometimes, 0, 2, Some("failed")),
        (AssertionKind::Sometimes, 1, 1, Some("passed")),
        (AssertionKind::Reachable, 2, 0, Some("passed")),
        (AssertionKind::Reachable, 0, 2, None),
        (AssertionKind::Reachable, 1, 1, None),
        (AssertionKind::Unreachable, 2, 0, None),
        (AssertionKind::Unreachable, 0, 2, Some("failed")),
        (AssertionKind::Unreachable, 1, 1, None),
    ] {
        for must_hit in [None, Some(true), Some(false)] {
            let mut summary =
                summary_with_observations(kind, must_hit, true_count, false_count, "passed");
            for verdict in ["passed", "failed", "unexercised"] {
                summary["assertions"][0]["verdict"] = json!(verdict);
                if expected == Some(verdict) {
                    validate_assertion_summary(&summary).expect("valid observed verdict");
                    validate_assertion_summary_for_promotion(&summary)
                        .expect("valid observed verdict remains promotable");
                } else {
                    assert!(
                        validate_assertion_summary(&summary).is_err(),
                        "{kind:?}/{must_hit:?} with {true_count} true and {false_count} false hits cannot report {verdict}"
                    );
                }
            }
        }
    }
}

// r[verify chaoscontrol.assertion_reports.must_hit_verdict]
#[test]
fn rejects_null_and_nonboolean_must_hit_metadata() {
    for kind in [
        AssertionKind::Always,
        AssertionKind::Sometimes,
        AssertionKind::Reachable,
        AssertionKind::Unreachable,
    ] {
        let verdict = if kind == AssertionKind::Unreachable {
            "passed"
        } else {
            "unexercised"
        };
        let base = summary_with_observations(kind, None, 0, 0, verdict);
        for invalid in [Value::Null, json!("true"), json!(1), json!([]), json!({})] {
            let mut summary = base.clone();
            summary["assertions"][0]["must_hit"] = invalid;
            assert!(
                validate_assertion_summary(&summary).is_err(),
                "{kind:?} must reject null or nonboolean metadata"
            );
        }
    }
}

// r[verify chaoscontrol.assertion_reports.must_hit_verdict]
#[test]
fn must_hit_does_not_relax_counter_integrity() {
    for must_hit in [None, Some(true), Some(false)] {
        for (hits, true_count, false_count) in [(0, 1, 0), (2, 0, 1), (0, u64::MAX, 1)] {
            let mut summary =
                summary_with_observations(AssertionKind::Always, must_hit, 0, 0, "passed");
            summary["assertions"][0]["hit_count"] = json!(hits);
            summary["assertions"][0]["true_count"] = json!(true_count);
            summary["assertions"][0]["false_count"] = json!(false_count);
            let error = validate_assertion_summary(&summary).expect_err("invalid counters");
            let expected_error = if true_count.checked_add(false_count).is_none() {
                "counter overflow"
            } else {
                "true and false counts do not equal hit count"
            };
            assert!(error.message().contains(expected_error));
        }
    }
}
