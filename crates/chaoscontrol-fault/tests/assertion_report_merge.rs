use chaoscontrol_fault::oracle::PropertyOracle;
use chaoscontrol_fault::report_merge::{merge_oracle_reports, ReportMergeConflict};
use chaoscontrol_protocol::assertion_catalog::{
    token_for_descriptors, BoundAssertionEvent, CatalogBuilder,
};
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
};

const COMPATIBILITY_ID: u32 = 303;
const SOURCE_LINE: u32 = 12;
const SOURCE_COLUMN: u32 = 4;

fn descriptor(namespace: &str, guest: &str) -> AssertionDescriptor {
    AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: namespace.to_string(),
        logical_key: AssertionLogicalKey::LegacyU32 {
            id: COMPATIBILITY_ID,
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

fn report_for(
    value: &AssertionDescriptor,
    hits: usize,
) -> chaoscontrol_fault::oracle::OracleReport {
    let token = token_for_descriptors(core::slice::from_ref(value)).expect("catalog token");
    let mut builder = CatalogBuilder::begin(1).expect("catalog begin");
    builder.insert(value.clone()).expect("descriptor");
    let catalog = builder.complete(token).expect("catalog complete");
    let fingerprint = value.fingerprint().expect("fingerprint");
    let event = BoundAssertionEvent {
        catalog_token: token,
        fingerprint,
        kind: value.kind,
    };
    let mut oracle = PropertyOracle::new();
    oracle.activate_catalog(catalog).expect("activate catalog");
    oracle.begin_run();
    for _ in 0..hits {
        oracle
            .record_bound_event(&event, true, None)
            .expect("bound event");
    }
    oracle.end_run();
    oracle.report()
}

#[test]
fn same_descriptor_aggregates_with_vm_provenance() {
    const FIRST_HITS: usize = 2;
    const SECOND_HITS: usize = 3;
    let value = descriptor("stable:raft", "raft");
    let reports = [
        (0, report_for(&value, FIRST_HITS)),
        (1, report_for(&value, SECOND_HITS)),
    ];
    let merged = merge_oracle_reports(&reports).expect("merge reports");
    let fingerprint = value.fingerprint().expect("fingerprint");
    let record = &merged.structured_assertions[&fingerprint];
    assert_eq!(record.hit_count, (FIRST_HITS + SECOND_HITS) as u64);
    assert_eq!(record.vm_instances.len(), 2);
    assert!(merged.collision_safe_evidence);
}

#[test]
fn same_legacy_number_in_distinct_namespaces_stays_separate() {
    let raft = descriptor("stable:raft", "raft");
    let redb = descriptor("stable:redb", "redb");
    let reports = [(0, report_for(&raft, 1)), (1, report_for(&redb, 1))];
    let merged = merge_oracle_reports(&reports).expect("namespace-aware merge");
    assert_eq!(merged.structured_assertions.len(), 2);
    assert_ne!(
        raft.fingerprint().expect("raft fingerprint"),
        redb.fingerprint().expect("redb fingerprint")
    );
}

#[test]
fn tampered_descriptor_and_fingerprint_fail_closed() {
    let value = descriptor("stable:raft", "raft");
    let fingerprint = value.fingerprint().expect("fingerprint");
    let mut report = report_for(&value, 1);
    report
        .structured_assertions
        .get_mut(&fingerprint)
        .expect("record")
        .identity
        .as_mut()
        .expect("identity")
        .canonical_bytes
        .push(0);
    assert!(matches!(
        merge_oracle_reports(&[(0, report)]),
        Err(ReportMergeConflict::DescriptorConflict)
    ));
}

#[test]
fn legacy_report_is_never_promoted_by_merge() {
    let mut oracle = PropertyOracle::new();
    oracle.begin_run();
    oracle.record_always(COMPATIBILITY_ID, true, "legacy");
    oracle.end_run();
    assert!(matches!(
        merge_oracle_reports(&[(0, oracle.report())]),
        Err(ReportMergeConflict::LegacyAmbiguous)
    ));
}

#[test]
fn report_metadata_conflict_is_rejected_before_aggregation() {
    let value = descriptor("stable:raft", "raft");
    let fingerprint = value.fingerprint().expect("fingerprint");
    let first = report_for(&value, 1);
    let mut second = report_for(&value, 1);
    second
        .structured_assertions
        .get_mut(&fingerprint)
        .expect("record")
        .guest = "spoofed".to_string();
    assert!(matches!(
        merge_oracle_reports(&[(0, first), (1, second)]),
        Err(ReportMergeConflict::DescriptorConflict)
    ));
}
