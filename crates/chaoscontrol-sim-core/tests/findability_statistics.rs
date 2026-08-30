use chaoscontrol_sim_core::findability::{
    assemble_observations, fit_findability, validate_report, BugInstance, FindabilityPolicy,
    FindabilityStatus, SubtreeObservation,
};

const SOURCE_DIGEST: &str =
    "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FIRST_BUG_DIGEST: &str =
    "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SECOND_BUG_DIGEST: &str =
    "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const SUBTREE_HORIZON: u64 = 10;
const EARLY_BUG_TIME: u64 = 5;
const EARLIER_DUPLICATE_TIME: u64 = 3;
const LATER_DUPLICATE_TIME: u64 = 7;
const KNOWN_SUBTREE_COUNT: usize = 4;
const KNOWN_BUG_COUNT: usize = 2;
const KNOWN_TOTAL_TIME: u64 = 35;
const EXPECTED_RATE: f64 = 2.0 / 35.0;
const EXPECTED_MEAN: f64 = 35.0 / 2.0;
const FLOAT_TOLERANCE: f64 = 1.0e-12;
const PRIOR_SHAPE: f64 = 1.0;
const PRIOR_RATE: f64 = 10.0;
const CONFIDENCE_TARGET: f64 = 0.95;
const MAXIMUM_PROJECTED_RUNS: u64 = 10_000;

fn policy() -> FindabilityPolicy {
    FindabilityPolicy {
        prior_shape: PRIOR_SHAPE,
        prior_rate: PRIOR_RATE,
        confidence_target: CONFIDENCE_TARGET,
        maximum_projected_runs: MAXIMUM_PROJECTED_RUNS,
    }
}

fn subtree(
    subtree_id: &str,
    independence_group: &str,
    bugs: Vec<BugInstance>,
) -> SubtreeObservation {
    SubtreeObservation {
        generation_id: "generation-a".to_string(),
        subtree_id: subtree_id.to_string(),
        independence_group: independence_group.to_string(),
        observed_time: SUBTREE_HORIZON,
        source_blake3: SOURCE_DIGEST.to_string(),
        bugs,
    }
}

fn bug(found_at: u64, identity: &str) -> BugInstance {
    BugInstance {
        found_at,
        bug_blake3: identity.to_string(),
    }
}

#[test]
fn fits_known_rate_mean_and_conservative_projection() {
    let observations = assemble_observations(&[
        subtree(
            "subtree-a",
            "group-a",
            vec![bug(EARLY_BUG_TIME, FIRST_BUG_DIGEST)],
        ),
        subtree("subtree-b", "group-b", Vec::new()),
        subtree(
            "subtree-c",
            "group-c",
            vec![bug(SUBTREE_HORIZON, SECOND_BUG_DIGEST)],
        ),
        subtree("subtree-d", "group-d", Vec::new()),
    ])
    .expect("observations assemble");
    let report = fit_findability(&observations, &policy()).expect("model fits");
    assert_eq!(report.status, FindabilityStatus::Fitted);
    assert_eq!(report.subtree_count, KNOWN_SUBTREE_COUNT);
    assert_eq!(report.first_bug_count, KNOWN_BUG_COUNT);
    assert_eq!(report.total_survival_time, KNOWN_TOTAL_TIME);
    let fit = report.exponential.as_ref().expect("exponential fit");
    assert!((fit.bug_rate - EXPECTED_RATE).abs() < FLOAT_TOLERANCE);
    assert!((fit.mean_time_to_bug - EXPECTED_MEAN).abs() < FLOAT_TOLERANCE);
    let lomax = report.lomax.as_ref().expect("Lomax projection");
    assert!(lomax.p_survival_next_run > 0.0);
    assert!(lomax.p_survival_next_run < 1.0);
    assert!(lomax.projected_additional_runs.is_some());
    validate_report(&report, &observations, &policy()).expect("report validates");
}

#[test]
fn duplicate_bug_records_count_only_the_first_instance() {
    let observations = assemble_observations(&[subtree(
        "subtree-a",
        "group-a",
        vec![
            bug(LATER_DUPLICATE_TIME, SECOND_BUG_DIGEST),
            bug(EARLIER_DUPLICATE_TIME, FIRST_BUG_DIGEST),
        ],
    )])
    .expect("observations assemble");
    assert_eq!(observations[0].first_bug_at, Some(EARLIER_DUPLICATE_TIME));
    assert_eq!(observations[0].discarded_bug_instances, 1);
}

#[test]
fn empty_and_single_observation_inputs_fail_or_remain_insufficient() {
    assert!(assemble_observations(&[]).is_err());
    let observations = assemble_observations(&[subtree(
        "subtree-a",
        "group-a",
        vec![bug(EARLY_BUG_TIME, FIRST_BUG_DIGEST)],
    )])
    .expect("single observation assembles");
    let report = fit_findability(&observations, &policy()).expect("single sample reports");
    assert_eq!(report.status, FindabilityStatus::InsufficientSamples);
    assert!(report.lomax.is_none());
}

#[test]
fn no_bug_generation_reports_unbounded_estimate() {
    let observations = assemble_observations(&[
        subtree("subtree-a", "group-a", Vec::new()),
        subtree("subtree-b", "group-b", Vec::new()),
    ])
    .expect("observations assemble");
    let report = fit_findability(&observations, &policy()).expect("no-bug report");
    assert_eq!(report.status, FindabilityStatus::NoBugObserved);
    assert!(report.exponential.is_none());
    assert!(report.lomax.is_none());
}

#[test]
fn baked_in_bug_flags_every_subtree_without_confidence() {
    let observations = assemble_observations(&[
        subtree(
            "subtree-a",
            "group-a",
            vec![bug(EARLY_BUG_TIME, FIRST_BUG_DIGEST)],
        ),
        subtree(
            "subtree-b",
            "group-b",
            vec![bug(EARLY_BUG_TIME, SECOND_BUG_DIGEST)],
        ),
    ])
    .expect("observations assemble");
    let report = fit_findability(&observations, &policy()).expect("baked-in report");
    assert_eq!(report.status, FindabilityStatus::IndependenceViolation);
    assert_eq!(report.independence.baked_in_subtrees.len(), KNOWN_BUG_COUNT);
    assert!(report.lomax.is_none());
}

#[test]
fn correlated_groups_and_identity_drift_fail_closed() {
    let observations = assemble_observations(&[
        subtree(
            "subtree-a",
            "shared-group",
            vec![bug(EARLY_BUG_TIME, FIRST_BUG_DIGEST)],
        ),
        subtree("subtree-b", "shared-group", Vec::new()),
    ])
    .expect("observations assemble");
    let mut report = fit_findability(&observations, &policy()).expect("correlated report");
    assert_eq!(report.status, FindabilityStatus::IndependenceViolation);
    assert_eq!(report.independence.correlated_groups, vec!["shared-group"]);
    report.observation_set_blake3 = SOURCE_DIGEST.to_string();
    assert!(validate_report(&report, &observations, &policy()).is_err());

    let mut invalid_policy = policy();
    invalid_policy.confidence_target = 1.0;
    assert_eq!(
        fit_findability(&observations, &invalid_policy)
            .expect_err("invalid confidence target")
            .class,
        "findability-policy"
    );
}
