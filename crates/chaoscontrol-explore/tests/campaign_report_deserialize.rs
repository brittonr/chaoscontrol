use chaoscontrol_explore::campaign::{CampaignReport, SeedSummary};
use chaoscontrol_explore::explorer::AssertionStats;
use chaoscontrol_protocol::assertion_catalog::CatalogValidationStatus;
use serde_json::json;

fn report() -> CampaignReport {
    CampaignReport {
        seeds_run: Vec::new(),
        seeds_with_bugs: Vec::new(),
        total_rounds: 0,
        total_branches: 0,
        bugs: Vec::new(),
        per_seed: Vec::new(),
        assertion_details: Vec::new(),
        assertion_stats: AssertionStats {
            catalog_size: 0,
            passed: 0,
            failed: 0,
            unexercised: 0,
        },
        assertion_identity_conflicts: Vec::new(),
        assertion_catalog_status: CatalogValidationStatus::Pending,
        collision_safe_assertion_evidence: false,
        wall_clock_seconds: 0.0,
        failed_seeds: Vec::new(),
        scenario_config: None,
    }
}

#[test]
fn seed_summary_rejects_unknown_fields() {
    let value = json!({
        "seed": 1,
        "rounds": 1,
        "total_branches": 1,
        "total_edges": 1,
        "bugs_found": 0,
        "wall_clock_seconds": 0.0,
        "unexpected": true
    });

    assert!(serde_json::from_value::<SeedSummary>(value).is_err());
}

#[test]
fn campaign_report_rejects_unknown_fields() {
    let mut value = serde_json::to_value(report()).expect("campaign report JSON");
    value["unexpected"] = json!(true);

    assert!(serde_json::from_value::<CampaignReport>(value).is_err());
}
