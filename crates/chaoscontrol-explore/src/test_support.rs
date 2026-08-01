use chaoscontrol_protocol::admission::{
    token_for_descriptors, AssertionEvidenceIdentity, CatalogBuilder,
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
