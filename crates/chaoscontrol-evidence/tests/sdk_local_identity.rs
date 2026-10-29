use chaoscontrol_evidence::{check_sdk_assertion_quality_report, summarize_sdk_local_jsonl};
use chaoscontrol_protocol::admission::{token_for_descriptors, ASSERTION_CATALOG_VERSION};
use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, AssertionLogicalKey,
    ASSERTION_FINGERPRINT_BYTES, ASSERTION_IDENTITY_VERSION, MAX_ASSERTION_CANONICAL_BYTES,
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
    let token = match token_for_descriptors(descriptors) {
        Ok(token) => token,
        Err(_) => AssertionFingerprint::ZERO,
    };
    let mut lines = vec![json!({
        "chaoscontrol_assertion_catalog": {
            "record": "begin",
            "catalog_version": ASSERTION_CATALOG_VERSION,
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
            "catalog_version": ASSERTION_CATALOG_VERSION,
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
    event_line_with_condition(descriptor, message, token_override, true)
}

fn event_line_with_condition(
    descriptor: &AssertionDescriptor,
    message: &str,
    token_override: Option<Value>,
    condition: bool,
) -> String {
    let token = token_for_descriptors(core::slice::from_ref(descriptor)).expect("catalog token");
    let (assert_type, display_type, must_hit) = match descriptor.kind {
        AssertionKind::Always => ("always", "Always", true),
        AssertionKind::Sometimes => ("sometimes", "Sometimes", true),
        AssertionKind::Reachable => ("reachability", "Reachable", true),
        AssertionKind::Unreachable => ("reachability", "Unreachable", false),
    };
    let value = json!({
        "antithesis_assert": {
            "assert_type": assert_type,
            "condition": condition,
            "hit": true,
            "must_hit": must_hit,
            "id": descriptor.compatibility_id
                .map(|value| format!("{value:08x}"))
                .unwrap_or_else(|| descriptor.fingerprint().expect("fingerprint").to_hex()),
            "message": message,
            "display_type": display_type,
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

// r[verify chaoscontrol.optional_local_quality.blockers]
#[test]
fn optional_no_hit_properties_pass_without_inventing_observations() {
    let value = descriptor("stable:optional-quality", "raft");
    let base = format!(
        "{}{}",
        catalog_jsonl(std::slice::from_ref(&value)),
        setup_line()
    );
    let record = |hit: bool, must_hit: bool| {
        let mut event: Value = serde_json::from_str(&event_line_with_condition(
            &value,
            &value.message,
            None,
            false,
        ))
        .expect("assertion record");
        event["antithesis_assert"]["hit"] = json!(hit);
        event["antithesis_assert"]["must_hit"] = json!(must_hit);
        event["antithesis_assert"]["display_type"] = json!(if must_hit {
            "Always"
        } else {
            "AlwaysOrUnreachable"
        });
        render_lines(&[event])
    };
    let optional = format!("{base}{}", record(false, false));
    let report =
        summarize_sdk_local_jsonl(&optional, EVIDENCE_CLASS, None).expect("optional report");
    assert_eq!(report["registered_assertions"], 1);
    assert_eq!(report["exercised_assertions"], 0);
    assert_eq!(report["failed_assertions"], 0);
    assert_eq!(report["assertion_coverage"][0]["observed"], false);
    assert_eq!(report["assertion_coverage"][0]["observed_hits"], 0);
    assert_eq!(report["assertion_coverage"][0]["must_hit"], false);
    assert_eq!(report["unobserved_assertions"], json!([]));
    assert!(
        check_sdk_assertion_quality_report(&report)
            .expect("optional quality")
            .passed
    );

    let mut stale = report.clone();
    stale["unobserved_assertion_count"] = json!(1);
    stale["unobserved_assertions"] = json!([value.message]);
    assert!(check_sdk_assertion_quality_report(&stale).is_err());

    for input in [base.clone(), format!("{base}{}", record(false, true))] {
        let required = summarize_sdk_local_jsonl(&input, EVIDENCE_CLASS, None)
            .expect("required or unknown report");
        assert_eq!(required["unobserved_assertions"], json!([value.message]));
        assert!(
            !check_sdk_assertion_quality_report(&required)
                .expect("required quality")
                .passed
        );
        let mut forged = required;
        forged["unobserved_assertion_count"] = json!(0);
        forged["unobserved_assertions"] = json!([]);
        assert!(check_sdk_assertion_quality_report(&forged).is_err());
    }

    let violated = format!("{optional}{}", record(true, false));
    let report = summarize_sdk_local_jsonl(&violated, EVIDENCE_CLASS, None)
        .expect("observed optional violation");
    assert_eq!(report["failed_assertions"], 1);
    assert_eq!(report["assertion_coverage"][0]["failure_count"], 1);
    assert!(
        !check_sdk_assertion_quality_report(&report)
            .expect("violated optional quality")
            .passed
    );
}

// r[verify chaoscontrol.optional_local_quality.blockers]
#[test]
fn required_progress_sites_still_block_when_unobserved() {
    for kind in [AssertionKind::Sometimes, AssertionKind::Reachable] {
        let mut value = descriptor("stable:required-progress", "raft");
        value.kind = kind;
        let mut declaration: Value = serde_json::from_str(&event_line_with_condition(
            &value,
            &value.message,
            None,
            false,
        ))
        .expect("declaration");
        declaration["antithesis_assert"]["hit"] = json!(false);
        let content = format!(
            "{}{}{}",
            catalog_jsonl(std::slice::from_ref(&value)),
            setup_line(),
            render_lines(&[declaration])
        );
        let report = summarize_sdk_local_jsonl(&content, EVIDENCE_CLASS, None)
            .expect("required progress report");
        let progress_field = if kind == AssertionKind::Sometimes {
            "sometimes_without_success"
        } else {
            "reachable_without_hit"
        };
        assert_eq!(report[progress_field], json!([value.message]));
        assert_eq!(report["unobserved_assertions"], json!([value.message]));
        assert_eq!(report["failed_assertions"], 1);
        assert!(
            !check_sdk_assertion_quality_report(&report)
                .expect("required progress quality")
                .passed
        );
        let mut forged = report;
        forged[progress_field] = json!([]);
        assert!(check_sdk_assertion_quality_report(&forged).is_err());
    }
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
        Some(json!("00".repeat(ASSERTION_FINGERPRINT_BYTES))),
    ));
    assert!(summarize_sdk_local_jsonl(&unknown, EVIDENCE_CLASS, None).is_err());

    let mut spoof = catalog_jsonl(core::slice::from_ref(&value));
    spoof.push_str(&event_line(&value, "spoofed message", None));
    assert!(summarize_sdk_local_jsonl(&spoof, EVIDENCE_CLASS, None).is_err());

    let mut wrong_type = catalog_jsonl(core::slice::from_ref(&value));
    let mut event: Value =
        serde_json::from_str(event_line(&value, &value.message, None).trim()).expect("event value");
    event["antithesis_assert"]["details"]["guest"] = json!(7);
    wrong_type.push_str(&render_lines(&[event]));
    assert!(summarize_sdk_local_jsonl(&wrong_type, EVIDENCE_CLASS, None).is_err());
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
        report["assertion_coverage"][0]["identity"]["fingerprint"],
        report["assertion_coverage"][1]["identity"]["fingerprint"]
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
fn local_catalog_rejects_count_and_canonical_hex_bounds() {
    let value = descriptor("stable:raft", "raft");
    let content = catalog_jsonl(std::slice::from_ref(&value));
    let mut lines = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("catalog line"))
        .collect::<Vec<_>>();
    lines.last_mut().expect("complete record")["chaoscontrol_assertion_catalog"]
        ["descriptor_count"] = json!(0);
    assert!(summarize_sdk_local_jsonl(&render_lines(&lines), EVIDENCE_CLASS, None).is_err());

    lines = content
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("catalog line"))
        .collect();
    lines[1]["chaoscontrol_assertion_catalog"]["canonical_descriptor"] =
        json!("00".repeat(MAX_ASSERTION_CANONICAL_BYTES + 1));
    assert!(summarize_sdk_local_jsonl(&render_lines(&lines), EVIDENCE_CLASS, None).is_err());
}

#[test]
fn caller_flip_invalid_setup_and_duplicate_messages_fail_closed() {
    let legacy = "{\"antithesis_assert\":{\"assert_type\":\"always\",\"condition\":true,\"hit\":true,\"id\":\"1\",\"message\":\"legacy\",\"details\":{}}}\n";
    let mut report =
        summarize_sdk_local_jsonl(legacy, EVIDENCE_CLASS, None).expect("legacy report");
    report["collision_safe_evidence"] = json!(true);
    assert!(check_sdk_assertion_quality_report(&report).is_err());
    for setup in [
        "{\"antithesis_setup\":null}\n",
        "{\"antithesis_setup\":{\"status\":\"pending\",\"details\":{}}}\n",
        "{\"antithesis_setup\":{\"status\":\"complete\",\"details\":{},\"extra\":true}}\n",
    ] {
        assert!(summarize_sdk_local_jsonl(setup, EVIDENCE_CLASS, None).is_err());
    }

    let raft = descriptor("stable:raft", "raft");
    let redb = descriptor("stable:redb", "redb");
    let mut duplicate_messages = catalog_jsonl(&[raft, redb]);
    duplicate_messages.push_str(setup_line());
    let report = summarize_sdk_local_jsonl(&duplicate_messages, EVIDENCE_CLASS, None)
        .expect("duplicate messages remain distinct");
    assert_eq!(report["unobserved_assertions"].as_array().unwrap().len(), 2);
    assert!(
        !check_sdk_assertion_quality_report(&report)
            .expect("quality validation")
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

#[test]
fn setup_details_accept_all_json_types_without_changing_assertion_identity() {
    let assertion = descriptor("stable:setup-details", "raft");
    let catalog = catalog_jsonl(std::slice::from_ref(&assertion));
    for details in [
        Value::Null,
        json!(false),
        json!(true),
        json!(u64::MAX),
        json!(-1),
        json!(0.5),
        json!("ready \"雪\""),
        json!([]),
        json!([{"adoption_track": "nested-not-metadata"}, [null, false, 7]]),
        json!({}),
        json!({"adoption_track": "external-harness"}),
    ] {
        let setup = render_lines(&[json!({
            "antithesis_setup": {"status": "complete", "details": details}
        })]);
        let input = format!(
            "{catalog}{setup}{}",
            event_line(&assertion, &assertion.message, None)
        );
        let report = summarize_sdk_local_jsonl(&input, EVIDENCE_CLASS, None)
            .unwrap_or_else(|error| panic!("details {details}: {error}"));
        assert_eq!(report["setup_complete"], true);
        assert_eq!(report["lifecycle_events"]["setup_complete"], 1);
        assert_eq!(report["observed_assertions"], 1);
        assert_eq!(report["failed_assertions"], 0);
        assert_eq!(report["catalog_status"], "accepted");
        assert!(check_sdk_assertion_quality_report(&report).unwrap().passed);
        let expected_tracks = if details.is_object() && details.get("adoption_track").is_some() {
            json!({"external-harness": 1})
        } else {
            json!({})
        };
        assert_eq!(report["adoption_tracks"], expected_tracks);
    }
}

#[test]
fn json_setup_details_preserve_record_and_metadata_bounds() {
    const MAX_FIELDS: usize = 64;
    const MAX_KEY_BYTES: usize = 128;
    let mut boundary = (0..MAX_FIELDS - 1)
        .map(|index| (format!("field-{index}"), Value::Null))
        .collect::<serde_json::Map<_, _>>();
    boundary.insert("é".repeat(MAX_KEY_BYTES / "é".len()), Value::Null);
    let setup = |details| {
        render_lines(&[json!({
            "antithesis_setup": {"status": "complete", "details": details}
        })])
    };
    let valid = setup(Value::Object(boundary.clone()));
    let report = summarize_sdk_local_jsonl(&valid, EVIDENCE_CLASS, None)
        .expect("object metadata at both limits");
    assert_eq!(report["setup_complete"], true);

    boundary.insert("one-too-many".to_string(), Value::Null);
    for details in [
        Value::Object(boundary),
        json!({"": null}),
        Value::Object(
            [("x".repeat(MAX_KEY_BYTES + 1), Value::Null)]
                .into_iter()
                .collect(),
        ),
    ] {
        let error = summarize_sdk_local_jsonl(&setup(details), EVIDENCE_CLASS, None)
            .expect_err("object metadata bounds remain enforced");
        assert!(error.message().contains("metadata bounds"), "{error}");
    }
    for invalid in [
        "{\"antithesis_setup\":{\"status\":\"complete\"}}\n".to_string(),
        "{\"antithesis_setup\":{\"status\":null,\"details\":null}}\n".to_string(),
        "{\"antithesis_setup\":{\"status\":\"complete\",\"details\":false,\"extra\":true}}\n"
            .to_string(),
    ] {
        assert!(summarize_sdk_local_jsonl(&invalid, EVIDENCE_CLASS, None).is_err());
    }
}

#[test]
fn non_object_setup_details_still_obey_json_resource_limits() {
    const EXCESSIVE_DEPTH: usize = 65;
    const EXCESSIVE_STRING_BYTES: usize = 13 * 1024;
    let nested = format!(
        "{}0{}",
        "[".repeat(EXCESSIVE_DEPTH),
        "]".repeat(EXCESSIVE_DEPTH)
    );
    let long_string = serde_json::to_string(&"x".repeat(EXCESSIVE_STRING_BYTES)).unwrap();
    for details in [nested, long_string] {
        let input =
            format!("{{\"antithesis_setup\":{{\"status\":\"complete\",\"details\":{details}}}}}\n");
        assert!(summarize_sdk_local_jsonl(&input, EVIDENCE_CLASS, None).is_err());
    }
}

#[test]
fn repeated_setup_keeps_the_first_completion_and_its_metadata() {
    let assertion = descriptor("stable:first-setup", "raft");
    let catalog = catalog_jsonl(std::slice::from_ref(&assertion));
    for first in [
        json!({"adoption_track": "first"}),
        Value::Null,
        json!(true),
        json!(7),
        json!("ready"),
        json!([{"adoption_track": "nested"}]),
    ] {
        let records = [
            first.clone(),
            first.clone(),
            json!({"adoption_track": "replacement"}),
            Value::Null,
        ]
        .into_iter()
        .map(|details| {
            json!({
                "antithesis_setup": {"status": "complete", "details": details}
            })
        })
        .collect::<Vec<_>>();
        let input = format!(
            "{catalog}{}{}{}",
            render_lines(&records),
            event_line(&assertion, &assertion.message, None),
            render_lines(&[json!({"after_setup": {}})])
        );
        let report = summarize_sdk_local_jsonl(&input, EVIDENCE_CLASS, None)
            .expect("valid repeated setup report");
        assert_eq!(report["setup_complete"], true);
        assert_eq!(report["lifecycle_events"]["setup_complete"], 1);
        assert_eq!(report["lifecycle_events"]["after_setup"], 1);
        assert_eq!(report["observed_assertions"], 1);
        assert_eq!(report["failed_assertions"], 0);
        assert_eq!(
            report["adoption_tracks"],
            if first.is_object() {
                json!({"first": 1})
            } else {
                json!({})
            }
        );
        assert!(check_sdk_assertion_quality_report(&report).unwrap().passed);
    }
}
