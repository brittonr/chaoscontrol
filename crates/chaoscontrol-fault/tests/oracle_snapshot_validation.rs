use chaoscontrol_fault::engine::{validate_engine_snapshot, EngineConfig, FaultEngine};
use chaoscontrol_fault::oracle::{PropertyOracle, Verdict};
use chaoscontrol_fault::oracle_validation::{
    validate_oracle_snapshot, validate_restorable_oracle_snapshot, validate_strict_oracle_report,
    OracleValidationError,
};
use chaoscontrol_protocol::assertion_catalog::{
    catalog_token, token_for_descriptors, AcceptedCatalog, AdmittedAssertion, CatalogBuilder,
    CatalogConflict, CatalogValidationStatus, ASSERTION_CATALOG_VERSION,
};
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_FINGERPRINT_BYTES,
    ASSERTION_IDENTITY_VERSION,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const COMPATIBILITY_ID: u32 = 71;
const SOURCE_LINE: u32 = 19;
const SOURCE_COLUMN: u32 = 3;
const FUTURE_RUN_ID: u32 = 2;

fn descriptor() -> AssertionDescriptor {
    AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "org.example.snapshot".to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: "snapshot-key".to_string(),
        },
        compatibility_id: Some(COMPATIBILITY_ID),
        kind: AssertionKind::Always,
        message: "snapshot assertion".to_string(),
        source_file: "src/main.rs".to_string(),
        source_line: SOURCE_LINE,
        source_column: SOURCE_COLUMN,
        guest: "guest".to_string(),
        category: "invariant".to_string(),
    }
}

fn legacy_descriptor() -> AssertionDescriptor {
    let mut legacy = descriptor();
    legacy.namespace = "legacy:guest".to_string();
    legacy.logical_key = AssertionLogicalKey::LegacyU32 {
        id: COMPATIBILITY_ID,
    };
    legacy
}

fn strict_oracle() -> PropertyOracle {
    let descriptor = descriptor();
    let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("token");
    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    builder.insert(descriptor.clone()).expect("descriptor");
    let catalog = builder.complete(token).expect("catalog complete");
    let fingerprint = descriptor.fingerprint().expect("fingerprint");
    let event = chaoscontrol_protocol::assertion_catalog::BoundAssertionEvent {
        catalog_token: token,
        fingerprint,
        kind: descriptor.kind,
    };
    let mut oracle = PropertyOracle::new();
    oracle.activate_catalog(catalog).expect("activate catalog");
    oracle.begin_run();
    oracle
        .record_bound_event(&event, true, None)
        .expect("record event");
    oracle.end_run();
    oracle
}

fn active_failure_snapshot() -> chaoscontrol_fault::oracle::OracleSnapshot {
    let descriptor = descriptor();
    let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("token");
    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    builder.insert(descriptor.clone()).expect("descriptor");
    let catalog = builder.complete(token).expect("catalog complete");
    let event = chaoscontrol_protocol::assertion_catalog::BoundAssertionEvent {
        catalog_token: token,
        fingerprint: descriptor.fingerprint().expect("fingerprint"),
        kind: descriptor.kind,
    };
    let mut oracle = PropertyOracle::new();
    oracle.activate_catalog(catalog).expect("activate catalog");
    oracle.begin_run();
    oracle
        .record_bound_event(&event, false, None)
        .expect("record failure");
    oracle.snapshot()
}

fn forged_snapshot(mutator: impl FnOnce(&mut Value)) -> chaoscontrol_fault::oracle::OracleSnapshot {
    let snapshot = strict_oracle().snapshot();
    let mut value = serde_json::to_value(snapshot).expect("snapshot JSON");
    mutator(&mut value);
    serde_json::from_value(value).expect("forged snapshot shape")
}

fn legacy_diagnostic_snapshot() -> chaoscontrol_fault::oracle::OracleSnapshot {
    let mut value = serde_json::to_value(strict_oracle().snapshot()).expect("snapshot JSON");
    let mut record = first_map_value_mut(&mut value, "structured_assertions").clone();
    let record_object = record.as_object_mut().expect("legacy record object");
    record_object.remove("identity");
    record_object.remove("catalog_tokens");
    record_object.remove("vm_instances");
    value["assertions"] = json!({COMPATIBILITY_ID.to_string(): record});
    value["structured_assertions"] = json!({});
    value["accepted_catalog"] = Value::Null;
    value["catalog_status"] = json!("legacy-ambiguous");
    value["identity_conflicts"] = json!(["historical legacy assertion"]);
    value["current_run"] = Value::Null;
    serde_json::from_value(value).expect("legacy diagnostic snapshot")
}

fn fatal_diagnostic_snapshot() -> chaoscontrol_fault::oracle::OracleSnapshot {
    forged_snapshot(|value| {
        value["catalog_status"] = json!("fatal-conflict");
        value["identity_conflicts"] = json!(["historical assertion conflict"]);
        value["current_run"] = Value::Null;
    })
}

fn first_map_value_mut<'a>(value: &'a mut Value, field: &str) -> &'a mut Value {
    value[field]
        .as_object_mut()
        .expect("map object")
        .values_mut()
        .next()
        .expect("map entry")
}

#[test]
fn accepts_complete_strict_snapshot() {
    let snapshot = strict_oracle().snapshot();
    validate_oracle_snapshot(&snapshot).expect("strict snapshot");
}

#[test]
fn legacy_u32_is_rejected_by_activation_report_and_snapshot_validation() {
    let legacy = legacy_descriptor();
    let fingerprint = legacy.fingerprint().expect("legacy fingerprint");
    let admitted = AdmittedAssertion {
        canonical_bytes: legacy.canonical_bytes().expect("legacy canonical"),
        descriptor: legacy,
        fingerprint,
    };
    let catalog_assertions = BTreeMap::from([(fingerprint, admitted.clone())]);
    let token = catalog_token(&catalog_assertions);
    let catalog = AcceptedCatalog {
        catalog_version: ASSERTION_CATALOG_VERSION,
        token,
        status: CatalogValidationStatus::Accepted,
        assertions: catalog_assertions,
    };
    let mut activation = PropertyOracle::new();
    assert_eq!(
        activation.activate_catalog(catalog.clone()),
        Err(CatalogConflict::LegacyIdentityForbidden)
    );

    let strict = strict_oracle();
    let stable_fingerprint = descriptor().fingerprint().expect("stable fingerprint");
    let mut report = strict.report();
    let mut record = report
        .structured_assertions
        .remove(&stable_fingerprint)
        .expect("strict record");
    record.identity = Some(admitted);
    record.catalog_tokens = BTreeSet::from([token]);
    report
        .structured_assertions
        .insert(fingerprint, record.clone());
    assert_eq!(
        validate_strict_oracle_report(&report),
        Err(OracleValidationError::Catalog)
    );

    let mut snapshot = serde_json::to_value(strict.snapshot()).expect("snapshot JSON");
    snapshot["accepted_catalog"] = serde_json::to_value(catalog).expect("legacy catalog JSON");
    snapshot["structured_assertions"] =
        serde_json::to_value(BTreeMap::from([(fingerprint, record)])).expect("legacy records JSON");
    let snapshot = serde_json::from_value(snapshot).expect("legacy snapshot shape");
    assert_eq!(
        validate_oracle_snapshot(&snapshot),
        Err(OracleValidationError::Catalog)
    );
}

#[test]
fn rejects_malformed_catalog_token_and_descriptor() {
    let wrong_token = forged_snapshot(|value| {
        value["accepted_catalog"]["token"] = json!("00".repeat(ASSERTION_FINGERPRINT_BYTES));
    });
    assert_eq!(
        validate_oracle_snapshot(&wrong_token),
        Err(OracleValidationError::Catalog)
    );

    let wrong_descriptor = forged_snapshot(|value| {
        let assertion = first_map_value_mut(&mut value["accepted_catalog"], "assertions");
        assertion["descriptor"]["source_line"] = json!(0);
    });
    assert_eq!(
        validate_oracle_snapshot(&wrong_descriptor),
        Err(OracleValidationError::Catalog)
    );
}

#[test]
fn rejects_record_map_status_and_counter_forgery() {
    let map_mismatch = forged_snapshot(|value| {
        value["structured_assertions"] = json!({});
    });
    assert!(validate_oracle_snapshot(&map_mismatch).is_err());

    let status = forged_snapshot(|value| {
        value["catalog_status"] = json!("pending");
    });
    assert_eq!(
        validate_oracle_snapshot(&status),
        Err(OracleValidationError::Status)
    );

    let counter = forged_snapshot(|value| {
        let record = first_map_value_mut(value, "structured_assertions");
        record["hit_count"] = json!(1);
        record["true_count"] = json!(0);
        record["false_count"] = json!(0);
    });
    assert_eq!(
        validate_oracle_snapshot(&counter),
        Err(OracleValidationError::Counter)
    );
}

#[test]
fn rejects_spoofed_record_metadata_and_event_run() {
    let metadata = forged_snapshot(|value| {
        let record = first_map_value_mut(value, "structured_assertions");
        record["message"] = json!("spoofed message");
    });
    assert_eq!(
        validate_oracle_snapshot(&metadata),
        Err(OracleValidationError::Record)
    );

    let event = forged_snapshot(|value| {
        value["events"] = json!([{
            "run_id": FUTURE_RUN_ID,
            "name": "future-event",
            "details": {}
        }]);
    });
    assert_eq!(
        validate_oracle_snapshot(&event),
        Err(OracleValidationError::Event)
    );
}

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
