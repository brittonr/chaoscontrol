use chaoscontrol_protocol::admission::{
    token_for_descriptors, AssertionEvidenceIdentity, BoundAssertionEvent, CatalogBuilder,
};
use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
};

pub(crate) fn assertion_identity(alias: u64) -> AssertionEvidenceIdentity {
    let compatibility_id = u32::try_from(alias).expect("test alias fits u32");
    let descriptor = AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "org.example.test".to_string(),
        logical_key: AssertionLogicalKey::Stable {
            key: format!("assertion-{alias}"),
        },
        kind: AssertionKind::Always,
        message: format!("assertion {alias}"),
        source_file: "src/test.rs".to_string(),
        source_line: 1,
        source_column: 1,
        guest: "test-guest".to_string(),
        category: "invariant".to_string(),
        compatibility_id: Some(compatibility_id),
    };
    let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("catalog token");
    let mut builder = CatalogBuilder::begin(1).expect("catalog begins");
    builder.insert(descriptor).expect("descriptor inserts");
    let catalog = builder.complete(token).expect("catalog completes");
    let admitted = catalog
        .assertions
        .values()
        .next()
        .expect("admitted assertion");
    AssertionEvidenceIdentity::from_admitted(admitted, token).expect("evidence identity")
}

pub(crate) fn assertion_report(
    alias: u64,
    observation: bool,
) -> chaoscontrol_fault::oracle::OracleReport {
    let identity = assertion_identity(alias);
    let mut builder = CatalogBuilder::begin(1).expect("catalog begins");
    builder
        .insert(identity.descriptor.clone())
        .expect("descriptor inserts");
    let catalog = builder
        .complete(identity.catalog_token)
        .expect("catalog completes");
    let mut oracle = chaoscontrol_fault::oracle::PropertyOracle::new();
    oracle.activate_catalog(catalog).expect("catalog activates");
    oracle.begin_run();
    oracle
        .record_bound_event(
            &BoundAssertionEvent {
                catalog_token: identity.catalog_token,
                fingerprint: identity.fingerprint,
                kind: identity.descriptor.kind,
            },
            observation,
            None,
        )
        .expect("observation records");
    oracle.end_run();
    oracle.report()
}
