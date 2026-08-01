use chaoscontrol_fault::oracle::{OracleReport, PropertyOracle};
use chaoscontrol_protocol::admission::{
    token_for_descriptors, BoundAssertionEvent, CatalogBuilder, CatalogValidationStatus,
};
use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
};
use std::collections::BTreeMap;

pub(super) const COMPATIBILITY_ID: u32 = 303;
const SOURCE_LINE: u32 = 12;
const SOURCE_COLUMN: u32 = 4;

pub(super) fn descriptor(namespace: &str, guest: &str) -> AssertionDescriptor {
    AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: namespace.to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: "commit-index-bounded".to_string(),
        },
        compatibility_id: Some(COMPATIBILITY_ID),
        kind: AssertionKind::Always,
        message: "commit index is bounded".to_string(),
        source_file: "src/assertions.rs".to_string(),
        source_line: SOURCE_LINE,
        source_column: SOURCE_COLUMN,
        guest: guest.to_string(),
        category: "invariant".to_string(),
    }
}

pub(super) fn oracle_for(value: &AssertionDescriptor) -> (PropertyOracle, BoundAssertionEvent) {
    let token = token_for_descriptors(core::slice::from_ref(value)).expect("catalog token");
    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    builder.insert(value.clone()).expect("descriptor");
    let catalog = builder.complete(token).expect("catalog complete");
    let event = BoundAssertionEvent {
        catalog_token: token,
        fingerprint: value.fingerprint().expect("fingerprint"),
        kind: value.kind,
    };
    let mut oracle = PropertyOracle::new();
    oracle.activate_catalog(catalog).expect("activate catalog");
    (oracle, event)
}

pub(super) fn report_for(value: &AssertionDescriptor, hits: usize) -> OracleReport {
    let (mut oracle, event) = oracle_for(value);
    oracle.begin_run();
    for _ in 0..hits {
        oracle
            .record_bound_event(&event, true, None)
            .expect("bound event");
    }
    oracle.end_run();
    oracle.report()
}

pub(super) fn forged_legacy_report() -> OracleReport {
    let value = descriptor("stable:legacy-fixture", "legacy-fixture");
    let fingerprint = value.fingerprint().expect("fixture fingerprint");
    let mut report = report_for(&value, 1);
    let mut record = report
        .structured_assertions
        .remove(&fingerprint)
        .expect("fixture record");
    record.identity = None;
    record.catalog_tokens.clear();
    report.assertions = BTreeMap::from([(COMPATIBILITY_ID, record)]);
    report.catalog_status = CatalogValidationStatus::LegacyAmbiguous;
    report.identity_conflicts = vec!["historical legacy identity".to_string()];
    report.collision_safe_evidence = false;
    report
}
