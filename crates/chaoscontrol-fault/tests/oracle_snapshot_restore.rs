mod oracle_snapshot_common;
mod oracle_snapshot_restore_support;

use chaoscontrol_fault::engine::{validate_engine_snapshot, EngineConfig, FaultEngine};
use chaoscontrol_fault::oracle::{PropertyOracle, Verdict};
use chaoscontrol_fault::oracle_validation::{
    validate_oracle_snapshot, validate_restorable_oracle_snapshot, OracleValidationError,
};
use chaoscontrol_protocol::assertion_catalog::{token_for_descriptors, CatalogValidationStatus};
use chaoscontrol_protocol::assertion_identity::ASSERTION_FINGERPRINT_BYTES;
use oracle_snapshot_common::{descriptor, first_map_value_mut, forged_snapshot, strict_oracle};
use oracle_snapshot_restore_support::{
    active_failure_snapshot, fatal_diagnostic_snapshot, legacy_diagnostic_snapshot,
};
use serde_json::json;

#[test]
fn forged_current_run_sets_and_message_are_rejected_without_mutation() {
    let fingerprint = descriptor().fingerprint().expect("fingerprint").to_hex();
    let unknown_hit = forged_snapshot(|value| {
        let run_id = value["total_runs"].clone();
        value["current_run"] = json!({
            "run_id": run_id,
            "strict_hit_ids": ["00".repeat(ASSERTION_FINGERPRINT_BYTES)],
            "strict_satisfied_ids": [],
            "setup_complete": false,
            "immediate_failure": null
        });
    });
    let non_subset = forged_snapshot(|value| {
        let run_id = value["total_runs"].clone();
        value["current_run"] = json!({
            "run_id": run_id,
            "strict_hit_ids": [],
            "strict_satisfied_ids": [fingerprint],
            "setup_complete": false,
            "immediate_failure": null
        });
    });
    let mut wrong_message = serde_json::to_value(active_failure_snapshot()).expect("snapshot JSON");
    wrong_message["current_run"]["immediate_failure"][1] = json!("forged message");
    let wrong_message = serde_json::from_value(wrong_message).expect("forged current run");

    let mut oracle = strict_oracle();
    let before = oracle.report();
    for snapshot in [&unknown_hit, &non_subset, &wrong_message] {
        assert!(validate_restorable_oracle_snapshot(snapshot).is_err());
        assert!(oracle.restore(snapshot).is_err());
        assert_eq!(oracle.report(), before);
    }
}

#[test]
fn rejected_restore_does_not_mutate_oracle() {
    let mut oracle = strict_oracle();
    let before = serde_json::to_value(oracle.report()).expect("report before");
    let forged = forged_snapshot(|value| {
        value["catalog_status"] = json!("pending");
    });

    assert!(oracle.restore(&forged).is_err());
    let after = serde_json::to_value(oracle.report()).expect("report after");
    assert_eq!(after, before);
    assert!(oracle.report().collision_safe_evidence);
}

#[test]
fn diagnostic_legacy_and_fatal_snapshots_are_not_restorable() {
    let legacy = legacy_diagnostic_snapshot();
    let fatal = fatal_diagnostic_snapshot();
    validate_oracle_snapshot(&legacy).expect("bounded legacy diagnostic");
    validate_oracle_snapshot(&fatal).expect("bounded fatal diagnostic");
    assert_eq!(
        validate_restorable_oracle_snapshot(&legacy),
        Err(OracleValidationError::Status)
    );
    assert_eq!(
        validate_restorable_oracle_snapshot(&fatal),
        Err(OracleValidationError::Status)
    );

    let mut oracle = strict_oracle();
    let before = oracle.report();
    for snapshot in [&legacy, &fatal] {
        assert!(oracle.restore(snapshot).is_err());
        assert_eq!(oracle.report(), before);
    }
}

#[test]
fn engine_restore_rejects_diagnostic_snapshot_before_mutation() {
    let mut engine = FaultEngine::new(EngineConfig::default());
    let before = engine.oracle().report();
    let mut value = serde_json::to_value(engine.snapshot()).expect("engine snapshot JSON");
    value["oracle"] =
        serde_json::to_value(legacy_diagnostic_snapshot()).expect("legacy oracle JSON");
    let snapshot = serde_json::from_value(value).expect("diagnostic engine snapshot");

    assert_eq!(
        validate_engine_snapshot(&snapshot),
        Err(OracleValidationError::Status)
    );
    assert!(engine.restore(&snapshot).is_err());
    assert_eq!(engine.oracle().report(), before);
}

#[test]
fn event_counter_overflow_is_non_partial_and_poisoned() {
    let snapshot = forged_snapshot(|value| {
        let record = first_map_value_mut(value, "structured_assertions");
        record["hit_count"] = json!(u64::MAX);
        record["true_count"] = json!(u64::MAX);
    });
    validate_oracle_snapshot(&snapshot).expect("bounded maximum snapshot");
    let mut oracle = PropertyOracle::new();
    oracle.restore(&snapshot).expect("restore maximum snapshot");
    let descriptor = descriptor();
    let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("token");
    let event = chaoscontrol_protocol::assertion_catalog::BoundAssertionEvent {
        catalog_token: token,
        fingerprint: descriptor.fingerprint().expect("fingerprint"),
        kind: descriptor.kind,
    };
    let before = oracle.report().structured_assertions[&event.fingerprint].hit_count;

    assert!(oracle.record_bound_event(&event, true, None).is_err());
    let report = oracle.report();
    assert_eq!(
        report.structured_assertions[&event.fingerprint].hit_count,
        before
    );
    assert_eq!(
        report.catalog_status,
        CatalogValidationStatus::FatalConflict
    );
    assert!(!report.collision_safe_evidence);
    assert_eq!(
        report.structured_assertions[&event.fingerprint].verdict(),
        Verdict::Passed
    );
}

#[test]
fn total_run_overflow_is_non_partial_and_poisoned() {
    let snapshot = forged_snapshot(|value| {
        value["total_runs"] = json!(u32::MAX);
    });
    let mut oracle = PropertyOracle::new();
    oracle
        .restore(&snapshot)
        .expect("restore maximum run snapshot");
    oracle.begin_run();
    oracle.end_run();

    let report = oracle.report();
    assert_eq!(report.total_runs, u32::MAX);
    assert_eq!(
        report.catalog_status,
        CatalogValidationStatus::FatalConflict
    );
    assert!(!report.collision_safe_evidence);
}
