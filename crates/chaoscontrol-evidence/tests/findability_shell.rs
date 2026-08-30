use chaoscontrol_evidence::{
    bind_findability_artifact, check_findability_artifact_path, read_findability_artifact_path,
    write_findability_report_path, RoundSubtree,
};
use chaoscontrol_sim_core::findability::{BugInstance, FindabilityPolicy, FindabilityStatus};
use std::os::unix::fs::symlink;

const BUG_DIGEST: &str = "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SUBTREE_HORIZON: u64 = 10;
const BUG_TIME: u64 = 5;
const PRIOR_SHAPE: f64 = 1.0;
const PRIOR_RATE: f64 = 10.0;
const CONFIDENCE_TARGET: f64 = 0.95;
const MAXIMUM_PROJECTED_RUNS: u64 = 10_000;
const FITTED_SUBTREE_COUNT: usize = 3;

fn policy() -> FindabilityPolicy {
    FindabilityPolicy {
        prior_shape: PRIOR_SHAPE,
        prior_rate: PRIOR_RATE,
        confidence_target: CONFIDENCE_TARGET,
        maximum_projected_runs: MAXIMUM_PROJECTED_RUNS,
    }
}

fn subtree(id: &str, bugs: Vec<BugInstance>) -> RoundSubtree {
    RoundSubtree {
        subtree_id: id.to_string(),
        independence_group: format!("group-{id}"),
        observed_time: SUBTREE_HORIZON,
        bugs,
    }
}

fn artifact() -> chaoscontrol_evidence::FindabilityRoundArtifact {
    bind_findability_artifact(
        "generation-a",
        policy(),
        vec![
            subtree(
                "a",
                vec![BugInstance {
                    found_at: BUG_TIME,
                    bug_blake3: BUG_DIGEST.to_string(),
                }],
            ),
            subtree("b", Vec::new()),
            subtree("c", Vec::new()),
        ],
    )
    .expect("artifact binds")
}

#[test]
fn shell_reads_checks_and_publishes_findability_report() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let artifact_path = directory.path().join("findability.json");
    let report_path = directory.path().join("report.json");
    let artifact = artifact();
    std::fs::write(
        &artifact_path,
        serde_json::to_vec_pretty(&artifact).expect("artifact serializes"),
    )
    .expect("artifact writes");

    let loaded = read_findability_artifact_path(&artifact_path).expect("artifact reads");
    assert_eq!(loaded.artifact_blake3, artifact.artifact_blake3);
    let report = check_findability_artifact_path(&artifact_path).expect("artifact checks");
    assert_eq!(report.status, FindabilityStatus::Fitted);
    assert_eq!(report.subtree_count, FITTED_SUBTREE_COUNT);
    write_findability_report_path(&artifact_path, &report_path).expect("report writes");
    assert!(report_path.is_file());
    assert!(write_findability_report_path(&artifact_path, &report_path).is_err());
}

#[test]
fn shell_rejects_symlink_identity_and_policy_drift() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let artifact_path = directory.path().join("findability.json");
    let link_path = directory.path().join("findability-link.json");
    let mut bound_artifact = artifact();
    std::fs::write(
        &artifact_path,
        serde_json::to_vec_pretty(&bound_artifact).expect("artifact serializes"),
    )
    .expect("artifact writes");
    symlink(&artifact_path, &link_path).expect("artifact symlink");
    assert!(read_findability_artifact_path(&link_path).is_err());

    bound_artifact.artifact_blake3 =
        "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string();
    std::fs::write(
        &artifact_path,
        serde_json::to_vec_pretty(&bound_artifact).expect("drifted artifact serializes"),
    )
    .expect("drifted artifact writes");
    assert!(check_findability_artifact_path(&artifact_path).is_err());

    let mut invalid_policy = artifact();
    invalid_policy.policy.confidence_target = 1.0;
    assert!(chaoscontrol_evidence::validate_findability_artifact(&invalid_policy).is_err());
}
