use chaoscontrol_fault::oracle::{PropertyOracle, Verdict};
use chaoscontrol_fault::oracle_validation::{validate_oracle_snapshot, OracleValidationError};
use chaoscontrol_protocol::assertion_catalog::{
    token_for_descriptors, CatalogBuilder, CatalogValidationStatus,
};
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_FINGERPRINT_BYTES,
    ASSERTION_IDENTITY_VERSION,
};
use serde_json::{json, Value};

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

fn forged_snapshot(mutator: impl FnOnce(&mut Value)) -> chaoscontrol_fault::oracle::OracleSnapshot {
    let snapshot = strict_oracle().snapshot();
    let mut value = serde_json::to_value(snapshot).expect("snapshot JSON");
    mutator(&mut value);
    serde_json::from_value(value).expect("forged snapshot shape")
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
