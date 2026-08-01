use chaoscontrol_evidence::{check_sdk_assertion_quality_report, summarize_sdk_local_jsonl};
use chaoscontrol_protocol::assertion_catalog::{token_for_descriptors, ASSERTION_CATALOG_VERSION};
use chaoscontrol_protocol::assertion_identity::{
    encode_lower_hex, AssertionDescriptor, AssertionKind, AssertionLogicalKey,
    ASSERTION_IDENTITY_VERSION,
};
use serde_json::{json, Value};

const COMPATIBILITY_ID: u32 = 73;
const SOURCE_LINE: u32 = 19;
const SOURCE_COLUMN: u32 = 7;
const EVIDENCE_CLASS: &str = "instrumentation-dry-run";

fn strict_descriptor() -> AssertionDescriptor {
    AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "org.example.local".to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: "strict".to_string(),
        },
        compatibility_id: Some(COMPATIBILITY_ID),
        kind: AssertionKind::Always,
        message: "local identity".to_string(),
        source_file: "src/main.rs".to_string(),
        source_line: SOURCE_LINE,
        source_column: SOURCE_COLUMN,
        guest: "guest".to_string(),
        category: "invariant".to_string(),
    }
}

fn strict_catalog_jsonl(descriptor: &AssertionDescriptor) -> String {
    let token = token_for_descriptors(std::slice::from_ref(descriptor)).expect("strict token");
    [
        json!({
            "chaoscontrol_assertion_catalog": {
                "record": "begin",
                "catalog_version": ASSERTION_CATALOG_VERSION,
                "expected_descriptors": 1,
                "valid": true
            }
        }),
        json!({
            "chaoscontrol_assertion_catalog": {
                "record": "descriptor",
                "fingerprint": descriptor.fingerprint().expect("fingerprint"),
                "descriptor": descriptor,
                "canonical_descriptor": encode_lower_hex(
                    &descriptor.canonical_bytes().expect("canonical")
                )
            }
        }),
        json!({
            "chaoscontrol_assertion_catalog": {
                "record": "complete",
                "catalog_version": ASSERTION_CATALOG_VERSION,
                "descriptor_count": 1,
                "catalog_token": token
            }
        }),
    ]
    .into_iter()
    .map(|value| serde_json::to_string(&value).expect("catalog JSON"))
    .collect::<Vec<_>>()
    .join("\n")
        + "\n"
}

#[test]
fn local_quality_rejects_legacy_u32_as_strict_identity() {
    let strict = strict_descriptor();
    let content = strict_catalog_jsonl(&strict);
    let mut report =
        summarize_sdk_local_jsonl(&content, EVIDENCE_CLASS, None).expect("strict local report");
    let mut legacy = strict;
    legacy.namespace = "legacy:local".to_string();
    legacy.logical_key = AssertionLogicalKey::LegacyU32 {
        id: COMPATIBILITY_ID,
    };
    let fingerprint = legacy.fingerprint().expect("legacy fingerprint");
    let identity = &mut report["assertion_coverage"][0]["identity"];
    identity["descriptor"] = serde_json::to_value(&legacy).expect("legacy descriptor JSON");
    identity["fingerprint"] = json!(fingerprint);
    identity["canonical_descriptor"] = json!(encode_lower_hex(
        &legacy.canonical_bytes().expect("legacy canonical")
    ));
    identity["catalog_token"] = json!(fingerprint);

    let error = check_sdk_assertion_quality_report(&report).expect_err("legacy strict identity");
    assert!(error.message().contains("LegacyIdentityForbidden"));
}
