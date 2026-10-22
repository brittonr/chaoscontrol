use super::{CampaignProfile, FaultScheduleProfile, RunProfile};
use crate::explorer::{FrontierPolicy, LEGACY_DIAGNOSTIC_POLICY_NAME};

const FRONTIER_POLICY_KEY: &str = "frontier_policy";
const UNKNOWN_FRONTIER_POLICIES: [&str; 6] = [
    "bogus",
    "Legacy",
    "campaign_candidate",
    "",
    "legacy",
    "campaign-candidate",
];
const CAMPAIGN_RUN: &str =
    include_str!("../../../../contracts/evidence/fixtures/valid/run-profile.campaign.valid.json");
const LEGACY_DIAGNOSTIC_RUN: &str = include_str!(
    "../../../../contracts/evidence/fixtures/valid/run-profile.legacy-diagnostic.valid.json"
);
const RANDOM_FAULTS_OPT_OUT_RUN: &str = include_str!(
    "../../../../contracts/evidence/fixtures/valid/run-profile.random-faults-opt-out.valid.json"
);
const RANDOM_FAULTS_KEY: &str = "random_faults";

const RUN: &str =
    include_str!("../../../../contracts/evidence/fixtures/valid/run-profile.valid.json");
const CAMPAIGN: &str =
    include_str!("../../../../contracts/evidence/fixtures/valid/campaign-profile.valid.json");
const SCHEDULE: &str =
    include_str!("../../../../contracts/evidence/fixtures/valid/fault-schedule-profile.valid.json");

#[test]
fn run_profile_maps_every_runtime_boundary_field() {
    let profile: RunProfile = serde_json::from_str(RUN).expect("run profile");
    let config = profile
        .try_into_explorer_config(47, Some("out".to_string()))
        .expect("run conversion");

    assert_eq!(config.seed, 47);
    assert_eq!(config.num_vms, 3);
    assert_eq!(config.vm_config.num_vcpus, 1);
    assert_eq!(config.vm_config.memory_size, 128 * 1024 * 1024);
    assert_eq!(config.branch_factor, 2);
    assert_eq!(config.output_dir.as_deref(), Some("out"));
    assert_eq!(config.coverage_gpa, 0);
}

#[test]
fn run_profile_rejects_unknown_fields_and_conflicting_coverage() {
    let forged = RUN.replacen("\"seed\": 42", "\"seed\": 42, \"elapsed\": 1", 1);
    assert!(serde_json::from_str::<RunProfile>(&forged).is_err());

    let mut value = serde_json::from_str::<serde_json::Value>(RUN).expect("run JSON");
    value["coverage"]["bitmap_gpa"] = serde_json::json!(4096);
    let profile: RunProfile = serde_json::from_value(value).expect("typed conflict");
    assert!(profile.validate().is_err());
}

#[test]
fn campaign_profile_maps_workers_mutation_metrics_and_output() {
    let profile: CampaignProfile = serde_json::from_str(CAMPAIGN).expect("campaign profile");
    let config = profile
        .try_into_campaign_config(None)
        .expect("campaign conversion");

    assert_eq!(config.seeds, vec![42, 43, 44]);
    assert_eq!(config.base_explorer_config.num_workers, 2);
    assert_eq!(config.base_explorer_config.havoc_mutations, [4, 16]);
    assert!(config.base_explorer_config.emit_metrics);
    assert_eq!(config.output_dir, "campaign-results/raft-v1");
}

#[test]
fn campaign_profile_rejects_unknown_fields_duplicate_seeds_and_scenario_substitution() {
    let mut unknown = serde_json::from_str::<serde_json::Value>(CAMPAIGN).expect("campaign JSON");
    unknown
        .as_object_mut()
        .expect("campaign object")
        .insert("elapsed".to_string(), serde_json::json!(1));
    assert!(serde_json::from_value::<CampaignProfile>(unknown).is_err());

    let mut value = serde_json::from_str::<serde_json::Value>(CAMPAIGN).expect("campaign JSON");
    value["seeds"] = serde_json::json!([42, 42]);
    let duplicate: CampaignProfile = serde_json::from_value(value).expect("typed duplicate");
    assert!(duplicate.validate().is_err());

    let mut value = serde_json::from_str::<serde_json::Value>(CAMPAIGN).expect("campaign JSON");
    value["scenario"] = serde_json::json!({
        "kind": "relative-artifact",
        "path": "scenarios/raft.json",
        "identity": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    });
    let profile: CampaignProfile = serde_json::from_value(value).expect("scenario profile");
    assert!(profile.try_into_campaign_config(None).is_err());
}

#[test]
fn finite_schedule_maps_closed_fault_descriptors() {
    let profile: FaultScheduleProfile = serde_json::from_str(SCHEDULE).expect("schedule profile");
    let schedule = profile.try_into_schedule().expect("schedule conversion");

    assert_eq!(schedule.total(), 2);
    assert_eq!(schedule.faults()[0].time_ns, 1_000_000);
    assert_eq!(schedule.faults()[1].time_ns, 2_000_000);
}

// r[verify chaoscontrol.campaign_policy.cutover.valid]
#[test]
fn run_profile_without_frontier_policy_projects_campaign_and_keeps_its_shape() {
    let profile: RunProfile = serde_json::from_str(RUN).expect("run profile");
    assert_eq!(
        profile.exploration.frontier_policy,
        FrontierPolicy::Campaign
    );
    let serialized = serde_json::to_value(&profile).expect("profile JSON");
    assert!(serialized["exploration"].get(FRONTIER_POLICY_KEY).is_none());

    let config = profile
        .try_into_explorer_config(47, None)
        .expect("run conversion");
    assert_eq!(config.frontier_policy, FrontierPolicy::Campaign);

    let explicit: RunProfile = serde_json::from_str(CAMPAIGN_RUN).expect("campaign profile");
    let config = explicit
        .try_into_explorer_config(47, None)
        .expect("run conversion");
    assert_eq!(config.frontier_policy, FrontierPolicy::Campaign);
}

// r[verify chaoscontrol.campaign_policy.rollback.valid]
#[test]
fn run_profile_projects_an_explicit_legacy_diagnostic_policy() {
    let profile: RunProfile =
        serde_json::from_str(LEGACY_DIAGNOSTIC_RUN).expect("legacy-diagnostic profile");
    let serialized = serde_json::to_value(&profile).expect("profile JSON");
    assert_eq!(
        serialized["exploration"][FRONTIER_POLICY_KEY],
        serde_json::json!(LEGACY_DIAGNOSTIC_POLICY_NAME)
    );

    let config = profile
        .try_into_explorer_config(47, None)
        .expect("run conversion");
    assert_eq!(config.frontier_policy, FrontierPolicy::LegacyDiagnostic);
}

// r[verify chaoscontrol.campaign_policy.rollback.invalid]
#[test]
fn run_profile_rejects_unknown_and_retired_frontier_policies() {
    for unknown in UNKNOWN_FRONTIER_POLICIES {
        let mut value = serde_json::from_str::<serde_json::Value>(RUN).expect("run JSON");
        value["exploration"][FRONTIER_POLICY_KEY] = serde_json::json!(unknown);
        assert!(
            serde_json::from_value::<RunProfile>(value).is_err(),
            "policy {unknown:?} must be rejected"
        );
    }
}

// r[verify chaoscontrol.system_runs.fault_injection.scenario.exploration]
#[test]
fn run_profile_injects_random_faults_unless_it_opts_out_explicitly() {
    let profile: RunProfile = serde_json::from_str(RUN).expect("run profile");
    let serialized = serde_json::to_value(&profile).expect("profile JSON");
    assert!(serialized["exploration"].get(RANDOM_FAULTS_KEY).is_none());
    let config = profile
        .try_into_explorer_config(47, None)
        .expect("run conversion");
    assert!(config.random_faults);

    let opt_out: RunProfile =
        serde_json::from_str(RANDOM_FAULTS_OPT_OUT_RUN).expect("opt-out profile");
    let serialized = serde_json::to_value(&opt_out).expect("profile JSON");
    assert_eq!(
        serialized["exploration"][RANDOM_FAULTS_KEY],
        serde_json::json!(false)
    );
    let config = opt_out
        .try_into_explorer_config(47, None)
        .expect("run conversion");
    assert!(!config.random_faults);

    for malformed in [serde_json::json!("off"), serde_json::json!(0)] {
        let mut value = serde_json::from_str::<serde_json::Value>(RUN).expect("run JSON");
        value["exploration"][RANDOM_FAULTS_KEY] = malformed;
        assert!(serde_json::from_value::<RunProfile>(value).is_err());
    }
}

#[test]
fn campaign_profile_carries_the_run_frontier_policy_into_the_base_config() {
    let profile: CampaignProfile = serde_json::from_str(CAMPAIGN).expect("campaign profile");
    let config = profile
        .try_into_campaign_config(None)
        .expect("campaign conversion");
    assert_eq!(
        config.base_explorer_config.frontier_policy,
        FrontierPolicy::Campaign
    );

    let mut value = serde_json::from_str::<serde_json::Value>(CAMPAIGN).expect("campaign JSON");
    value["run"]["exploration"][FRONTIER_POLICY_KEY] =
        serde_json::json!(LEGACY_DIAGNOSTIC_POLICY_NAME);
    let profile: CampaignProfile = serde_json::from_value(value).expect("diagnostic campaign");
    let config = profile
        .try_into_campaign_config(None)
        .expect("campaign conversion");
    assert_eq!(
        config.base_explorer_config.frontier_policy,
        FrontierPolicy::LegacyDiagnostic
    );

    let mut value = serde_json::from_str::<serde_json::Value>(CAMPAIGN).expect("campaign JSON");
    value["run"]["exploration"][FRONTIER_POLICY_KEY] = serde_json::json!("bogus");
    assert!(serde_json::from_value::<CampaignProfile>(value).is_err());
}

#[test]
fn finite_schedule_rejects_unknown_fields_target_and_order_substitution() {
    let mut unknown = serde_json::from_str::<serde_json::Value>(SCHEDULE).expect("schedule JSON");
    unknown
        .as_object_mut()
        .expect("schedule object")
        .insert("elapsed".to_string(), serde_json::json!(1));
    assert!(serde_json::from_value::<FaultScheduleProfile>(unknown).is_err());

    let mut target = serde_json::from_str::<serde_json::Value>(SCHEDULE).expect("schedule JSON");
    target["faults"][1]["target"] = serde_json::json!(3);
    let target: FaultScheduleProfile = serde_json::from_value(target).expect("typed target");
    assert!(target.validate().is_err());

    let mut order = serde_json::from_str::<serde_json::Value>(SCHEDULE).expect("schedule JSON");
    order["faults"][1]["time_ns"] = serde_json::json!(1);
    let order: FaultScheduleProfile = serde_json::from_value(order).expect("typed order");
    assert!(order.validate().is_err());
}

#[test]
fn profile_loader_accepts_regular_files_and_rejects_symlinks() {
    let directory = tempfile::tempdir().expect("tempdir");
    let regular = directory.path().join("run.json");
    std::fs::write(&regular, RUN).expect("write profile");
    assert!(super::load_run_profile(&regular).is_ok());

    #[cfg(unix)]
    {
        let link = directory.path().join("run-link.json");
        std::os::unix::fs::symlink(&regular, &link).expect("symlink");
        assert!(super::load_run_profile(&link).is_err());
    }
}
