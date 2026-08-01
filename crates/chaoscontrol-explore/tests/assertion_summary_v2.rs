use chaoscontrol_evidence::{validate_assertion_summary, validate_assertion_summary_for_promotion};
use chaoscontrol_explore::assertion_summary::AssertionSummaryV2;
use chaoscontrol_explore::campaign::CampaignReport;
use chaoscontrol_explore::coverage::CoverageStats;
use chaoscontrol_explore::explorer::{
    AssertionDetail, AssertionIdentityDetail, AssertionStats, ExplorationReport,
};
use chaoscontrol_protocol::assertion_catalog::{token_for_descriptors, CatalogValidationStatus};
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
};

const COMPATIBILITY_ID: u32 = 71;
const SOURCE_LINE: u32 = 12;
const SOURCE_COLUMN: u32 = 3;

fn encode_hex(bytes: &[u8]) -> String {
    chaoscontrol_protocol::assertion_identity::encode_lower_hex(bytes)
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
        id: COMPATIBILITY_ID,
        identity: Some(AssertionIdentityDetail {
            descriptor,
            fingerprint,
            canonical_descriptor: encode_hex(&canonical),
            catalog_tokens: vec![token],
        }),
        message: "summary remains valid".to_string(),
        kind: "always".to_string(),
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
        assertion_catalog_status: CatalogValidationStatus::Accepted,
        collision_safe_assertion_evidence: true,
        assertion_identity_conflicts: Vec::new(),
        round_history: Vec::new(),
        wall_clock_seconds: 0.0,
        branches_per_second: 0.0,
        edges_per_second: 0.0,
        scenario_config: None,
        scenario_summary: None,
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
