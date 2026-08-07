use chaoscontrol_fault::oracle::PropertyOracle;
use chaoscontrol_protocol::admission::{token_for_descriptors, CatalogBuilder};
use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
};
use serde_json::Value;

pub const COMPATIBILITY_ID: u32 = 71;
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

pub fn strict_oracle() -> PropertyOracle {
    let descriptor = descriptor();
    let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("token");
    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    builder.insert(descriptor.clone()).expect("descriptor");
    let catalog = builder.complete(token).expect("catalog complete");
    let fingerprint = descriptor.fingerprint().expect("fingerprint");
    let event = chaoscontrol_protocol::admission::BoundAssertionEvent {
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

pub fn forged_snapshot(
    mutator: impl FnOnce(&mut Value),
) -> chaoscontrol_fault::oracle::OracleSnapshot {
    let snapshot = strict_oracle().snapshot();
    let mut value = serde_json::to_value(snapshot).expect("snapshot JSON");
    mutator(&mut value);
    serde_json::from_value(value).expect("forged snapshot shape")
}

pub fn first_map_value_mut<'a>(value: &'a mut Value, field: &str) -> &'a mut Value {
    value[field]
        .as_object_mut()
        .expect("map object")
        .values_mut()
        .next()
        .expect("map entry")
}
