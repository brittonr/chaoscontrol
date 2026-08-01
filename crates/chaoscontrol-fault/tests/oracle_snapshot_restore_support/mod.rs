use crate::oracle_snapshot_common::{
    descriptor, first_map_value_mut, forged_snapshot, strict_oracle, COMPATIBILITY_ID,
};
use chaoscontrol_fault::oracle::PropertyOracle;
use chaoscontrol_protocol::admission::{token_for_descriptors, CatalogBuilder};
use serde_json::{json, Value};

pub fn active_failure_snapshot() -> chaoscontrol_fault::oracle::OracleSnapshot {
    let descriptor = descriptor();
    let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("token");
    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    builder.insert(descriptor.clone()).expect("descriptor");
    let catalog = builder.complete(token).expect("catalog complete");
    let event = chaoscontrol_protocol::admission::BoundAssertionEvent {
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
