use chaoscontrol_evidence::{validate_assertion_summary, validate_assertion_summary_for_promotion};
use chaoscontrol_explore::assertion_summary::AssertionSummaryV2;
use chaoscontrol_explore::campaign::CampaignReport;
use chaoscontrol_explore::coverage::CoverageStats;
use chaoscontrol_explore::explorer::{
    AssertionDetail, AssertionIdentityDetail, AssertionStats, ExplorationReport,
};
use chaoscontrol_protocol::admission::{token_for_descriptors, CatalogValidationStatus};
use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
};

const COMPATIBILITY_ID: u32 = 71;
const SOURCE_LINE: u32 = 12;
const SOURCE_COLUMN: u32 = 3;

fn encode_hex(bytes: &[u8]) -> String {
    chaoscontrol_protocol::identity::encode_lower_hex(bytes)
}

fn strict_detail() -> AssertionDetail {
    let descriptor = AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "org.example.summary".to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: "stable-summary".to_string(),
        },
        compatibility_id: Some(COMPATIBILITY_ID),
        kind: AssertionKind::Always,
        message: "summary remains valid".to_string(),
        source_file: "src/main.rs".to_string(),
        source_line: SOURCE_LINE,
        source_column: SOURCE_COLUMN,
        guest: "guest".to_string(),
        category: "invariant".to_string(),
    };
    let fingerprint = descriptor.fingerprint().expect("fingerprint");
    let canonical = descriptor.canonical_bytes().expect("canonical descriptor");
    let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("token");
    AssertionDetail {
        process_instances: Default::default(),
        id: COMPATIBILITY_ID,
        identity: Some(AssertionIdentityDetail {
            descriptor,
            fingerprint,
            canonical_descriptor: encode_hex(&canonical),
            catalog_tokens: vec![token],
        }),
        message: "summary remains valid".to_string(),
        kind: "always".to_string(),
        must_hit: None,
        guest: "guest".to_string(),
        category: "invariant".to_string(),
        verdict: "passed".to_string(),
        hit_count: 1,
        true_count: 1,
        false_count: 0,
        last_failure_details: None,
    }
}

fn legacy_u32_detail() -> AssertionDetail {
    let mut detail = strict_detail();
    let identity = detail.identity.as_mut().expect("strict identity");
    identity.descriptor.namespace = "legacy:summary".to_string();
    identity.descriptor.logical_key = AssertionLogicalKey::LegacyU32 {
        id: COMPATIBILITY_ID,
    };
    identity.fingerprint = identity
        .descriptor
        .fingerprint()
        .expect("legacy fingerprint");
    identity.canonical_descriptor = encode_hex(
        &identity
            .descriptor
            .canonical_bytes()
            .expect("legacy canonical"),
    );
    identity.catalog_tokens = vec![identity.fingerprint];
    detail
}

fn campaign(
    details: Vec<AssertionDetail>,
    status: CatalogValidationStatus,
    collision_safe: bool,
    conflicts: Vec<String>,
) -> CampaignReport {
    let passed = details
        .iter()
        .filter(|detail| detail.verdict == "passed")
        .count();
    CampaignReport {
        seeds_run: vec![1],
        seeds_with_bugs: Vec::new(),
        total_rounds: 1,
        total_branches: 1,
        bugs: Vec::new(),
        per_seed: Vec::new(),
        assertion_stats: AssertionStats {
            catalog_size: details.len(),
            passed,
            failed: 0,
            unexercised: 0,
        },
        assertion_details: details,
        assertion_identity_conflicts: conflicts,
        assertion_catalog_status: status,
        collision_safe_assertion_evidence: collision_safe,
        wall_clock_seconds: 1.0,
        failed_seeds: Vec::new(),
        scenario_config: None,
        frontier_policy: chaoscontrol_explore::explorer::FrontierPolicy::default(),
        release_eligible: true,
        exhaustive_search_non_claim: chaoscontrol_explore::explorer::EXHAUSTIVE_SEARCH_NON_CLAIM
            .to_string(),
    }
}

#[test]
fn strict_campaign_summary_round_trips_through_promotion_validator() {
    let report = campaign(
        vec![strict_detail()],
        CatalogValidationStatus::Accepted,
        true,
        Vec::new(),
    );
    let summary = AssertionSummaryV2::from_campaign(&report).expect("strict summary");
    let value = serde_json::to_value(&summary).expect("summary JSON");
    validate_assertion_summary_for_promotion(&value).expect("promotion validation");
}

// r[verify chaoscontrol.assertion_reports.must_hit]
// r[verify chaoscontrol.assertion_reports.must_hit_verdict]
#[test]
fn zero_hit_must_hit_summaries_preserve_oracle_verdicts() {
    use chaoscontrol_explore::assertion_report::strict_projection;
    use chaoscontrol_fault::oracle::PropertyOracle;
    use chaoscontrol_protocol::admission::CatalogBuilder;

    for kind in [
        AssertionKind::Always,
        AssertionKind::Sometimes,
        AssertionKind::Reachable,
        AssertionKind::Unreachable,
    ] {
        for must_hit in [None, Some(true), Some(false)] {
            let mut descriptor = strict_detail().identity.expect("identity").descriptor;
            descriptor.kind = kind;
            let fingerprint = descriptor.fingerprint().expect("fingerprint");
            let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("token");
            let mut builder = CatalogBuilder::begin(1).expect("catalog begins");
            builder.insert(descriptor).expect("descriptor inserts");
            let mut oracle = PropertyOracle::new();
            oracle
                .activate_catalog(builder.complete(token).expect("catalog"))
                .expect("activation");
            if let Some(value) = must_hit {
                oracle
                    .record_event(
                        chaoscontrol_protocol::MUST_HIT_METADATA_EVENT,
                        serde_json::json!({"fingerprint": fingerprint.to_hex(), "must_hit": value}),
                    )
                    .expect("metadata");
            }
            let projection = strict_projection(&oracle.report()).expect("projection");
            let report = campaign(
                projection.details,
                projection.catalog_status,
                true,
                Vec::new(),
            );
            let summary =
                AssertionSummaryV2::from_campaign(&report).expect("oracle verdict accepted");
            let mut value = serde_json::to_value(&summary).expect("summary JSON");
            validate_assertion_summary_for_promotion(&value).expect("evidence verdict accepted");
            let parsed: AssertionSummaryV2 =
                serde_json::from_value(value.clone()).expect("read back");
            let expected = if kind == AssertionKind::Unreachable || must_hit == Some(false) {
                "passed"
            } else if must_hit == Some(true) {
                "failed"
            } else {
                "unexercised"
            };
            assert_eq!(parsed.assertions()[0].must_hit, must_hit);
            assert_eq!(parsed.assertions()[0].verdict, expected);
            if must_hit.is_none() {
                assert!(value["assertions"][0].get("must_hit").is_none());
            }
            value["assertions"][0]["verdict"] = serde_json::json!(if expected == "passed" {
                "failed"
            } else {
                "passed"
            });
            assert!(serde_json::from_value::<AssertionSummaryV2>(value.clone()).is_err());
            assert!(validate_assertion_summary_for_promotion(&value).is_err());
        }
    }
}

#[test]
fn must_hit_cannot_hide_observed_failure_or_accept_malformed_metadata() {
    let mut detail = strict_detail();
    detail.true_count = 0;
    detail.false_count = 1;
    detail.verdict = "failed".to_string();
    for must_hit in [true, false] {
        detail.must_hit = Some(must_hit);
        let report = campaign(
            vec![detail.clone()],
            CatalogValidationStatus::Accepted,
            true,
            Vec::new(),
        );
        let summary = AssertionSummaryV2::from_campaign(&report).expect("failure summary");
        let mut value = serde_json::to_value(summary).expect("JSON");
        value["assertions"][0]["verdict"] = serde_json::json!("passed");
        assert!(serde_json::from_value::<AssertionSummaryV2>(value.clone()).is_err());
        for malformed in [
            serde_json::Value::Null,
            serde_json::json!("false"),
            serde_json::json!(0),
        ] {
            value["assertions"][0]["must_hit"] = malformed;
            value["assertions"][0]["verdict"] = serde_json::json!("failed");
            assert!(serde_json::from_value::<AssertionSummaryV2>(value.clone()).is_err());
        }
    }
}

#[test]
fn legacy_u32_identity_is_rejected_by_exploration_and_campaign_summaries() {
    let legacy = legacy_u32_detail();
    let campaign = campaign(
        vec![legacy.clone()],
        CatalogValidationStatus::Accepted,
        true,
        Vec::new(),
    );
    assert!(AssertionSummaryV2::from_campaign(&campaign).is_err());

    let exploration = ExplorationReport {
        failed_branches: 0,
        rounds: 1,
        total_branches: 1,
        total_edges: 0,
        bugs: Vec::new(),
        corpus_size: 0,
        coverage_stats: CoverageStats {
            total_edges: 0,
            total_runs: 1,
            edges_per_run_avg: 0.0,
        },
        network_stats: chaoscontrol_vmm::controller::NetworkStats::default(),
        assertion_stats: AssertionStats {
            catalog_size: 1,
            passed: 1,
            failed: 0,
            unexercised: 0,
        },
        assertion_details: vec![legacy],
        guidance: Vec::new(),
        assertion_catalog_status: CatalogValidationStatus::Accepted,
        collision_safe_assertion_evidence: true,
        assertion_identity_conflicts: Vec::new(),
        campaign_liveness_gaps: Vec::new(),
        distinct_failing_assertions: 0,
        round_history: Vec::new(),
        wall_clock_seconds: 0.0,
        branches_per_second: 0.0,
        edges_per_second: 0.0,
        scenario_config: None,
        scenario_summary: None,
        frontier_policy: chaoscontrol_explore::explorer::FrontierPolicy::default(),
        release_eligible: true,
        stop: None,
    };
    assert!(AssertionSummaryV2::from_exploration(&exploration).is_err());
}

#[test]
fn legacy_campaign_summary_is_readable_but_non_promoting() {
    let mut legacy = strict_detail();
    legacy.identity = None;
    let report = campaign(
        vec![legacy],
        CatalogValidationStatus::LegacyAmbiguous,
        false,
        Vec::new(),
    );
    let summary = AssertionSummaryV2::from_campaign(&report).expect("legacy summary");
    let value = serde_json::to_value(&summary).expect("summary JSON");
    validate_assertion_summary(&value).expect("legacy diagnostic validation");
    assert!(validate_assertion_summary_for_promotion(&value).is_err());
}

#[test]
fn fatal_source_cannot_promote_a_structurally_valid_summary() {
    let report = campaign(
        vec![strict_detail()],
        CatalogValidationStatus::FatalConflict,
        false,
        vec!["source conflict".to_string()],
    );
    let summary = AssertionSummaryV2::from_campaign(&report).expect("fatal summary");
    assert_eq!(
        summary.catalog_status(),
        CatalogValidationStatus::FatalConflict
    );
    assert!(!summary.collision_safe_evidence());
    let value = serde_json::to_value(&summary).expect("summary JSON");
    validate_assertion_summary(&value).expect("fatal strict diagnostic validation");
    assert!(validate_assertion_summary_for_promotion(&value).is_err());
}

#[test]
fn fatal_legacy_and_mixed_summaries_remain_diagnostic() {
    let mut legacy = strict_detail();
    legacy.id = COMPATIBILITY_ID + 1;
    legacy.identity = None;
    legacy.message = "legacy diagnostic".to_string();
    let fatal_legacy = campaign(
        vec![legacy.clone()],
        CatalogValidationStatus::FatalConflict,
        false,
        vec!["legacy conflict".to_string()],
    );
    let summary = AssertionSummaryV2::from_campaign(&fatal_legacy).expect("fatal legacy");
    let value = serde_json::to_value(summary).expect("fatal legacy JSON");
    validate_assertion_summary(&value).expect("fatal legacy diagnostic validation");

    let fatal_mixed = campaign(
        vec![strict_detail(), legacy],
        CatalogValidationStatus::FatalConflict,
        false,
        vec!["mixed conflict".to_string()],
    );
    let summary = AssertionSummaryV2::from_campaign(&fatal_mixed).expect("fatal mixed");
    let value = serde_json::to_value(summary).expect("fatal mixed JSON");
    validate_assertion_summary(&value).expect("fatal mixed diagnostic validation");
    assert!(validate_assertion_summary_for_promotion(&value).is_err());
}

#[test]
fn pending_class_mismatch_and_empty_v2_summaries_are_rejected() {
    let empty = campaign(
        Vec::new(),
        CatalogValidationStatus::LegacyAmbiguous,
        false,
        Vec::new(),
    );
    assert!(AssertionSummaryV2::from_campaign(&empty).is_err());

    let report = campaign(
        vec![strict_detail()],
        CatalogValidationStatus::Accepted,
        true,
        Vec::new(),
    );
    let summary = AssertionSummaryV2::from_campaign(&report).expect("strict summary");
    let mut pending = serde_json::to_value(&summary).expect("summary JSON");
    pending["catalog_status"] = serde_json::json!("pending");
    assert!(validate_assertion_summary(&pending).is_err());

    let mut mismatch = serde_json::to_value(summary).expect("summary JSON");
    mismatch["catalog_status"] = serde_json::json!("legacy-ambiguous");
    mismatch["collision_safe_evidence"] = serde_json::json!(false);
    assert!(validate_assertion_summary(&mismatch).is_err());

    let demoted = campaign(
        vec![strict_detail()],
        CatalogValidationStatus::LegacyAmbiguous,
        false,
        Vec::new(),
    );
    let summary = AssertionSummaryV2::from_campaign(&demoted).expect("demoted strict summary");
    assert_eq!(
        summary.catalog_status(),
        CatalogValidationStatus::FatalConflict
    );
}

fn campaign_facts_summary() -> serde_json::Value {
    let report = campaign(
        vec![strict_detail()],
        CatalogValidationStatus::Accepted,
        true,
        Vec::new(),
    );
    let summary = AssertionSummaryV2::from_campaign(&report).expect("strict summary");
    let mut value = serde_json::to_value(summary).expect("summary JSON");
    // Campaign aggregates need not describe the current assertion row or its verdict.
    value["distinct_failing_assertions"] = serde_json::json!(4);
    value["liveness_gaps"] = serde_json::json!([
        {
            "fingerprint": "03".repeat(32),
            "kind": "sometimes",
            "message": "leader elected",
            "hit_count": 2
        },
        {
            "fingerprint": "04".repeat(32),
            "kind": "reachable",
            "message": "recovery reached",
            "hit_count": 0
        }
    ]);
    value
}

#[test]
fn campaign_facts_preserve_identity_authority_and_optional_field_compatibility() {
    let value = campaign_facts_summary();
    for omitted in [
        None,
        Some("liveness_gaps"),
        Some("distinct_failing_assertions"),
    ] {
        let mut value = value.clone();
        if let Some(field) = omitted {
            value.as_object_mut().expect("summary object").remove(field);
        }
        let parsed: AssertionSummaryV2 =
            serde_json::from_value(value.clone()).expect("producer accepts campaign facts");
        let round_trip = serde_json::to_value(parsed).expect("producer serializes facts");
        validate_assertion_summary(&round_trip).expect("consumer accepts campaign facts");
        validate_assertion_summary_for_promotion(&round_trip)
            .expect("campaign facts do not alter accepted identity authority");
    }
    let parsed: AssertionSummaryV2 = serde_json::from_value(value.clone()).expect("summary");
    assert_eq!(parsed.liveness_gaps().len(), 2);
    assert_eq!(parsed.distinct_failing_assertions(), 4);

    let mut fatal = value;
    fatal["catalog_status"] = serde_json::json!("fatal-conflict");
    fatal["collision_safe_evidence"] = serde_json::json!(false);
    assert!(serde_json::from_value::<AssertionSummaryV2>(fatal.clone()).is_ok());
    validate_assertion_summary(&fatal).expect("fatal campaign facts remain diagnostic");
    assert!(validate_assertion_summary_for_promotion(&fatal).is_err());

    let mut legacy = fatal;
    legacy["catalog_status"] = serde_json::json!("legacy-ambiguous");
    legacy["assertions"][0]
        .as_object_mut()
        .expect("assertion")
        .remove("identity");
    assert!(serde_json::from_value::<AssertionSummaryV2>(legacy.clone()).is_ok());
    validate_assertion_summary(&legacy).expect("legacy campaign facts remain diagnostic");
    assert!(validate_assertion_summary_for_promotion(&legacy).is_err());
}

#[test]
fn producer_and_consumer_reject_malformed_or_incoherent_campaign_facts() {
    use serde_json::{json, Value};

    let base = campaign_facts_summary();
    let mut cases = Vec::new();
    for field in ["distinct_failing_assertions", "liveness_gaps"] {
        for malformed in [Value::Null, json!("4"), json!(-1), json!(1.5), json!({})] {
            let mut value = base.clone();
            value[field] = malformed;
            cases.push(value);
        }
    }
    for (field, malformed) in [
        ("fingerprint", json!("not-a-fingerprint")),
        ("fingerprint", json!("AB".repeat(32))),
        ("fingerprint", Value::Null),
        ("kind", json!("always")),
        ("kind", json!("unreachable")),
        ("kind", json!("unknown")),
        ("kind", Value::Null),
        ("message", json!("")),
        ("message", json!("has\ncontrol")),
        (
            "message",
            json!("é".repeat(chaoscontrol_protocol::identity::MAX_ASSERTION_MESSAGE_BYTES / 2 + 1)),
        ),
        ("message", Value::Null),
        ("hit_count", json!(0)),
        ("hit_count", json!(-1)),
        ("hit_count", json!(1.5)),
        ("hit_count", json!("2")),
        ("hit_count", Value::Null),
        ("unexpected", json!(true)),
    ] {
        let mut value = base.clone();
        value["liveness_gaps"][0][field] = malformed;
        cases.push(value);
    }
    for field in ["fingerprint", "kind", "message", "hit_count"] {
        let mut value = base.clone();
        value["liveness_gaps"][0]
            .as_object_mut()
            .expect("gap")
            .remove(field);
        cases.push(value);
    }
    let mut reached = base.clone();
    reached["liveness_gaps"][1]["hit_count"] = json!(1);
    cases.push(reached);
    let mut duplicate = base.clone();
    duplicate["liveness_gaps"][1] = duplicate["liveness_gaps"][0].clone();
    cases.push(duplicate);
    let mut unknown = base.clone();
    unknown["unexpected"] = json!(true);
    cases.push(unknown);
    let mut spoofed = base.clone();
    spoofed["assertions"][0]["message"] = json!("spoofed identity metadata");
    cases.push(spoofed);

    for value in cases {
        assert!(
            serde_json::from_value::<AssertionSummaryV2>(value.clone()).is_err(),
            "producer: {value}"
        );
        assert!(
            validate_assertion_summary(&value).is_err(),
            "consumer: {value}"
        );
        assert!(
            validate_assertion_summary_for_promotion(&value).is_err(),
            "promotion: {value}"
        );
    }
}

#[test]
fn campaign_gap_metadata_accepts_exact_byte_and_counter_boundaries() {
    let mut value = campaign_facts_summary();
    value["liveness_gaps"][0]["message"] = serde_json::json!(
        "é".repeat(chaoscontrol_protocol::identity::MAX_ASSERTION_MESSAGE_BYTES / 2)
    );
    value["liveness_gaps"][0]["hit_count"] = serde_json::json!(u64::MAX);
    value["distinct_failing_assertions"] = serde_json::json!(usize::MAX);
    let parsed: AssertionSummaryV2 =
        serde_json::from_value(value.clone()).expect("exact producer boundaries");
    assert_eq!(parsed.distinct_failing_assertions(), usize::MAX);
    validate_assertion_summary_for_promotion(&value).expect("exact consumer boundaries");
}
