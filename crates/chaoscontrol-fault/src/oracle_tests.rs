use super::*;
use chaoscontrol_protocol::admission::{
    token_for_descriptors, BoundAssertionEvent, CatalogBuilder,
};
use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
};
use serde_json::json;

const COMPATIBILITY_ALIAS: u32 = 41;
const SOURCE_LINE: u32 = 17;
const SOURCE_COLUMN: u32 = 5;
const EVENT_RUN: u32 = 7;
const EVENT_ATTEMPT: u32 = 2;
const MULTI_RUN_COUNT: u32 = 3;

fn descriptor(kind: AssertionKind, key: &str) -> AssertionDescriptor {
    AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "org.onixresearch.oracle-tests".to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: key.to_string(),
        },
        compatibility_id: Some(COMPATIBILITY_ALIAS),
        kind,
        message: format!("{key} assertion"),
        source_file: "src/oracle_tests.rs".to_string(),
        source_line: SOURCE_LINE,
        source_column: SOURCE_COLUMN,
        guest: "oracle-test-guest".to_string(),
        category: "invariant".to_string(),
    }
}

fn strict_oracle(
    kind: AssertionKind,
    key: &str,
) -> (PropertyOracle, BoundAssertionEvent, AssertionFingerprint) {
    let descriptor = descriptor(kind, key);
    let token = token_for_descriptors(core::slice::from_ref(&descriptor)).expect("catalog token");
    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    builder.insert(descriptor.clone()).expect("catalog insert");
    let catalog = builder.complete(token).expect("catalog complete");
    let fingerprint = descriptor.fingerprint().expect("descriptor fingerprint");
    let event = BoundAssertionEvent {
        catalog_token: token,
        fingerprint,
        kind,
    };
    let mut oracle = PropertyOracle::new();
    oracle.activate_catalog(catalog).expect("activate catalog");
    (oracle, event, fingerprint)
}

fn record<'a>(report: &'a OracleReport, fingerprint: &AssertionFingerprint) -> &'a AssertionRecord {
    report
        .structured_assertions
        .get(fingerprint)
        .expect("structured assertion record")
}

#[test]
fn oracle_event_details_round_trip_in_binary_and_json_forms() {
    let event = OracleEvent {
        run_id: EVENT_RUN,
        name: "setup_complete".to_string(),
        details: json!({"workload": "rust-workload", "attempt": EVENT_ATTEMPT}),
    };
    let bytes = serde_json::to_vec(&event).expect("serialize event bytes");
    let from_bytes: OracleEvent = serde_json::from_slice(&bytes).expect("deserialize event bytes");
    let value = serde_json::to_value(&event).expect("serialize event value");
    let from_value: OracleEvent = serde_json::from_value(value).expect("deserialize event value");

    assert_eq!(from_bytes, event);
    assert_eq!(from_value, event);
}

#[test]
fn accepted_catalog_creates_unexercised_structured_record_only() {
    let (oracle, _, fingerprint) = strict_oracle(AssertionKind::Always, "unexercised");
    let report = oracle.report();

    assert!(report.assertions.is_empty());
    assert_eq!(report.catalog_size, 1);
    assert_eq!(report.unexercised, 1);
    assert_eq!(
        record(&report, &fingerprint).verdict(),
        Verdict::Unexercised
    );
    assert!(report.collision_safe_evidence);
    assert!(report
        .record_for_compatibility_id(COMPATIBILITY_ALIAS)
        .expect("strict selector")
        .is_some());
}

#[test]
fn always_failure_retains_fingerprint_and_failure_details() {
    let (mut oracle, event, fingerprint) = strict_oracle(AssertionKind::Always, "always");
    oracle.begin_run();
    assert!(!oracle.has_immediate_failure());
    let passed = oracle
        .record_bound_event(&event, false, Some(br#"{"failure":true}"#))
        .expect("bound event");

    assert!(!passed);
    assert_eq!(
        oracle.immediate_failure(),
        Some((fingerprint, "always assertion"))
    );
    oracle.end_run();
    let report = oracle.report();
    let record = record(&report, &fingerprint);
    assert_eq!(record.verdict(), Verdict::Failed);
    assert_eq!(record.first_failure_run, Some(0));
    assert_eq!(
        record.last_failure_details.as_deref(),
        Some(br#"{"failure":true}"#.as_slice())
    );
}

#[test]
fn sometimes_tracks_satisfaction_across_runs() {
    let (mut oracle, event, fingerprint) = strict_oracle(AssertionKind::Sometimes, "sometimes");
    for run in 0..MULTI_RUN_COUNT {
        oracle.begin_run();
        oracle
            .record_bound_event(&event, run == 1, None)
            .expect("bound event");
        oracle.end_run();
    }

    let report = oracle.report();
    let record = record(&report, &fingerprint);
    assert_eq!(record.verdict(), Verdict::Passed);
    assert_eq!(record.runs_hit, MULTI_RUN_COUNT);
    assert_eq!(record.runs_satisfied, 1);
}

#[test]
fn reachable_and_unreachable_use_kind_semantics() {
    let (mut reachable, reachable_event, reachable_fingerprint) =
        strict_oracle(AssertionKind::Reachable, "reachable");
    reachable.begin_run();
    assert!(reachable
        .record_bound_event(&reachable_event, false, None)
        .expect("reachable event"));
    reachable.end_run();
    assert_eq!(
        record(&reachable.report(), &reachable_fingerprint).verdict(),
        Verdict::Passed
    );

    let (mut unreachable, unreachable_event, unreachable_fingerprint) =
        strict_oracle(AssertionKind::Unreachable, "unreachable");
    unreachable.begin_run();
    assert!(!unreachable
        .record_bound_event(&unreachable_event, true, None)
        .expect("unreachable event"));
    assert_eq!(
        unreachable.immediate_failure(),
        Some((unreachable_fingerprint, "unreachable assertion"))
    );
}

#[test]
fn bound_event_without_active_run_does_not_change_counters() {
    let (mut oracle, event, fingerprint) = strict_oracle(AssertionKind::Always, "no-run");
    let before = oracle.structured_assertions()[&fingerprint].clone();

    assert_eq!(
        oracle.record_bound_event(&event, true, None),
        Err(CatalogConflict::NoActiveRun)
    );
    let after = &oracle.structured_assertions()[&fingerprint];
    assert_eq!(after, &before);
    assert_eq!(
        oracle.catalog_status(),
        CatalogValidationStatus::FatalConflict
    );
}

#[test]
fn event_without_accepted_catalog_fails_without_legacy_output() {
    let descriptor = descriptor(AssertionKind::Always, "unknown");
    let event = BoundAssertionEvent {
        catalog_token: AssertionFingerprint::ZERO,
        fingerprint: descriptor.fingerprint().expect("fingerprint"),
        kind: descriptor.kind,
    };
    let mut oracle = PropertyOracle::new();
    oracle.begin_run();

    assert_eq!(
        oracle.record_bound_event(&event, true, None),
        Err(CatalogConflict::CatalogIncomplete)
    );
    let report = oracle.report();
    assert!(report.assertions.is_empty());
    assert!(report.structured_assertions.is_empty());
    assert!(!report.collision_safe_evidence);
}

#[test]
fn active_run_report_is_explicitly_non_promoting() {
    let (mut oracle, event, _) = strict_oracle(AssertionKind::Always, "active-report");
    oracle.begin_run();
    oracle
        .record_bound_event(&event, true, None)
        .expect("bound event");

    let active = oracle.report();
    assert!(!active.collision_safe_evidence);
    assert!(crate::oracle_validation::validate_strict_oracle_report(&active).is_err());

    oracle.end_run();
    assert!(oracle.report().collision_safe_evidence);
}

#[test]
fn accepted_active_run_survives_snapshot_restore() {
    let (mut oracle, event, fingerprint) = strict_oracle(AssertionKind::Always, "snapshot");
    oracle.begin_run();
    oracle
        .record_bound_event(&event, false, None)
        .expect("bound event");
    let snapshot = oracle.snapshot();
    crate::oracle_validation::validate_restorable_oracle_snapshot(&snapshot)
        .expect("restorable accepted snapshot");

    let mut restored = PropertyOracle::new();
    restored
        .restore(&snapshot)
        .expect("restore strict snapshot");
    assert_eq!(
        restored.immediate_failure(),
        Some((fingerprint, "snapshot assertion"))
    );
    restored.end_run();
    assert_eq!(record(&restored.report(), &fingerprint).runs_hit, 1);
}

#[test]
fn progressed_pending_snapshot_is_not_restorable() {
    let mut pending = PropertyOracle::new();
    pending.begin_run();
    let snapshot = pending.snapshot();
    let (mut target, _, fingerprint) = strict_oracle(AssertionKind::Always, "target");
    let before = target.report();

    assert_eq!(
        crate::oracle_validation::validate_restorable_oracle_snapshot(&snapshot),
        Err(crate::oracle_validation::OracleValidationError::Status)
    );
    assert!(target.restore(&snapshot).is_err());
    assert_eq!(target.report(), before);
    assert!(target.structured_assertions().contains_key(&fingerprint));
}

#[test]
fn lifecycle_state_uses_the_current_structured_run() {
    let (mut oracle, _, _) = strict_oracle(AssertionKind::Always, "lifecycle");
    oracle.begin_run();
    assert!(!oracle.is_setup_complete());
    oracle
        .record_setup_complete()
        .expect("active setup completion");
    oracle
        .record_event("leader_elected", json!({"node": "2"}))
        .expect("active lifecycle event");
    assert!(oracle.is_setup_complete());
    oracle.end_run();

    let report = oracle.report();
    assert_eq!(report.events.len(), 1);
    assert_eq!(report.events[0].run_id, 0);
}

#[test]
fn strict_run_counter_overflow_has_no_partial_update() {
    let (oracle, _, fingerprint) = strict_oracle(AssertionKind::Always, "overflow");
    let mut record = oracle.report().structured_assertions[&fingerprint].clone();
    record.runs_hit = u32::MAX;
    let records = BTreeMap::from([(fingerprint, record)]);
    let hit = BTreeSet::from([fingerprint]);
    let satisfied = BTreeSet::new();

    assert_eq!(
        prepare_run_updates(&records, &hit, &satisfied),
        Err(CatalogConflict::CounterOverflow)
    );
    assert_eq!(records[&fingerprint].runs_hit, u32::MAX);
}
