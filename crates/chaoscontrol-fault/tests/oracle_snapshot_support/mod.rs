use chaoscontrol_fault::oracle::PropertyOracle;
use chaoscontrol_protocol::assertion_catalog::{
    catalog_token, token_for_descriptors, AcceptedCatalog, AdmittedAssertion, CatalogBuilder,
    CatalogValidationStatus, ASSERTION_CATALOG_VERSION,
};
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const COMPATIBILITY_ID: u32 = 71;
pub const FUTURE_RUN_ID: u32 = 2;
const SOURCE_LINE: u32 = 19;
const SOURCE_COLUMN: u32 = 3;

pub fn descriptor() -> AssertionDescriptor {
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

pub fn legacy_descriptor() -> AssertionDescriptor {
    let mut legacy = descriptor();
    legacy.namespace = "legacy:guest".to_string();
    legacy.logical_key = AssertionLogicalKey::LegacyU32 {
        id: COMPATIBILITY_ID,
    };
    legacy
}

pub fn strict_oracle() -> PropertyOracle {
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

pub fn active_failure_snapshot() -> chaoscontrol_fault::oracle::OracleSnapshot {
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

pub fn forged_snapshot(
    mutator: impl FnOnce(&mut Value),
) -> chaoscontrol_fault::oracle::OracleSnapshot {
    let snapshot = strict_oracle().snapshot();
    let mut value = serde_json::to_value(snapshot).expect("snapshot JSON");
    mutator(&mut value);
    serde_json::from_value(value).expect("forged snapshot shape")
}

pub fn legacy_diagnostic_snapshot() -> chaoscontrol_fault::oracle::OracleSnapshot {
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

pub fn fatal_diagnostic_snapshot() -> chaoscontrol_fault::oracle::OracleSnapshot {
    forged_snapshot(|value| {
        value["catalog_status"] = json!("fatal-conflict");
        value["identity_conflicts"] = json!(["historical assertion conflict"]);
        value["current_run"] = Value::Null;
    })
}

pub fn first_map_value_mut<'a>(value: &'a mut Value, field: &str) -> &'a mut Value {
    value[field]
        .as_object_mut()
        .expect("map object")
        .values_mut()
        .next()
        .expect("map entry")
}

pub fn legacy_catalog() -> (AcceptedCatalog, AdmittedAssertion) {
    let descriptor = legacy_descriptor();
    let fingerprint = descriptor.fingerprint().expect("legacy fingerprint");
    let admitted = AdmittedAssertion {
        canonical_bytes: descriptor.canonical_bytes().expect("legacy canonical"),
        descriptor,
        fingerprint,
    };
    let assertions = BTreeMap::from([(fingerprint, admitted.clone())]);
    let token = catalog_token(&assertions);
    (
        AcceptedCatalog {
            catalog_version: ASSERTION_CATALOG_VERSION,
            token,
            status: CatalogValidationStatus::Accepted,
            assertions,
        },
        admitted,
    )
}
