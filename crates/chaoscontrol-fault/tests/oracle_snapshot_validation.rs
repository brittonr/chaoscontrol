mod oracle_snapshot_common;
mod oracle_snapshot_validation_support;

use chaoscontrol_fault::oracle::PropertyOracle;
use chaoscontrol_fault::oracle_validation::{
    validate_oracle_snapshot, validate_strict_oracle_report, OracleValidationError,
};
use chaoscontrol_fault::resolve_snapshot_assertion_evidence;
use chaoscontrol_protocol::admission::{
    token_for_descriptors, AssertionEvidenceIdentity, CatalogBuilder, CatalogConflict,
};
use chaoscontrol_protocol::identity::ASSERTION_FINGERPRINT_BYTES;
use oracle_snapshot_common::{descriptor, first_map_value_mut, forged_snapshot, strict_oracle};
use oracle_snapshot_validation_support::{legacy_catalog, FUTURE_RUN_ID};
use serde_json::json;
use std::collections::BTreeSet;

#[test]
fn accepts_complete_strict_snapshot() {
    let snapshot = strict_oracle().snapshot();
    validate_oracle_snapshot(&snapshot).expect("strict snapshot");
}

#[test]
fn legacy_u32_is_rejected_by_activation_report_and_snapshot_validation() {
    let (catalog, admitted) = legacy_catalog();
    let fingerprint = admitted.fingerprint;
    let token = catalog.token;
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
        serde_json::to_value(std::collections::BTreeMap::from([(fingerprint, record)]))
            .expect("legacy records JSON");
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

fn snapshot_identity() -> AssertionEvidenceIdentity {
    let descriptor = descriptor();
    let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("catalog token");
    let mut builder = CatalogBuilder::begin(1).expect("catalog begins");
    builder.insert(descriptor).expect("descriptor inserts");
    let catalog = builder.complete(token).expect("catalog completes");
    let admitted = catalog
        .assertions
        .values()
        .next()
        .expect("admitted assertion");
    AssertionEvidenceIdentity::from_admitted(admitted, token).expect("snapshot identity")
}

#[test]
fn exact_snapshot_identity_resolves_the_structured_record() {
    let snapshot = strict_oracle().snapshot();
    let identity = snapshot_identity();

    let record = resolve_snapshot_assertion_evidence(&snapshot, &identity)
        .expect("exact admitted identity resolves");
    assert_eq!(record.message, identity.descriptor.message);
}

#[test]
fn exact_snapshot_resolution_rejects_token_substitution_and_missing_record() {
    let snapshot = strict_oracle().snapshot();
    let mut substituted = snapshot_identity();
    substituted.catalog_token = chaoscontrol_protocol::identity::AssertionFingerprint::ZERO;
    assert!(resolve_snapshot_assertion_evidence(&snapshot, &substituted).is_err());

    let missing_record = forged_snapshot(|value| {
        value["structured_assertions"] = json!({});
    });
    let identity = snapshot_identity();
    assert!(resolve_snapshot_assertion_evidence(&missing_record, &identity).is_err());
}
