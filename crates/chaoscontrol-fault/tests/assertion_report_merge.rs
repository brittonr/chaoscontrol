use chaoscontrol_fault::oracle::PropertyOracle;
use chaoscontrol_fault::report_merge::{merge_oracle_reports, ReportMergeConflict};
use chaoscontrol_protocol::assertion_catalog::{
    token_for_descriptors, BoundAssertionEvent, CatalogBuilder,
};
use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, AssertionLogicalKey,
    ASSERTION_IDENTITY_VERSION, MAX_ASSERTION_EVENT_DETAILS_BYTES,
};
use std::collections::BTreeSet;

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
    assert_eq!(
        merge_oracle_reports(&[(0, report)]),
        Err(ReportMergeConflict::IneligibleInput)
    );
}

#[test]
fn legacy_report_is_never_promoted_by_merge() {
    let mut oracle = PropertyOracle::new();
    oracle.begin_run();
    oracle.record_always(COMPATIBILITY_ID, true, "legacy");
    oracle.end_run();
    assert_eq!(
        merge_oracle_reports(&[(0, oracle.report())]),
        Err(ReportMergeConflict::IneligibleInput)
    );
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
    assert_eq!(
        merge_oracle_reports(&[(0, first), (1, second)]),
        Err(ReportMergeConflict::IneligibleInput)
    );
}

#[test]
fn same_logical_key_with_different_descriptor_is_rejected() {
    let first = descriptor("stable:raft", "raft");
    let mut second = first.clone();
    second.message = "different assertion".to_string();
    let reports = [(0, report_for(&first, 1)), (1, report_for(&second, 1))];

    assert_eq!(
        merge_oracle_reports(&reports),
        Err(ReportMergeConflict::CatalogConflict)
    );
}

#[test]
fn requires_distinct_vm_ids_and_exact_accepted_inputs() {
    let value = descriptor("stable:raft", "raft");
    let accepted = report_for(&value, 1);
    assert_eq!(
        merge_oracle_reports(&[(0, accepted.clone()), (0, accepted.clone())]),
        Err(ReportMergeConflict::DuplicateVmInstance)
    );
    assert_eq!(
        merge_oracle_reports(&[
            (0, accepted),
            (1, chaoscontrol_fault::oracle::OracleReport::empty())
        ]),
        Err(ReportMergeConflict::IneligibleInput)
    );
}

#[test]
fn recomputes_eligibility_instead_of_trusting_caller_boolean() {
    let value = descriptor("stable:raft", "raft");
    let mut valid = report_for(&value, 1);
    valid.collision_safe_evidence = false;
    assert!(
        merge_oracle_reports(&[(0, valid)])
            .expect("derived eligibility")
            .collision_safe_evidence
    );

    let mut legacy = PropertyOracle::new();
    legacy.begin_run();
    legacy.record_always(COMPATIBILITY_ID, true, "legacy");
    legacy.end_run();
    let mut forged = legacy.report();
    forged.collision_safe_evidence = true;
    assert_eq!(
        merge_oracle_reports(&[(0, forged)]),
        Err(ReportMergeConflict::IneligibleInput)
    );
}

#[test]
fn rejects_forged_catalog_and_vm_dimensions() {
    const FORGED_VM_INSTANCE: u32 = 99;
    let value = descriptor("stable:raft", "raft");
    let fingerprint = value.fingerprint().expect("fingerprint");
    let mut token = report_for(&value, 1);
    token
        .structured_assertions
        .get_mut(&fingerprint)
        .expect("record")
        .catalog_tokens = BTreeSet::from([AssertionFingerprint::ZERO]);
    assert_eq!(
        merge_oracle_reports(&[(0, token)]),
        Err(ReportMergeConflict::IneligibleInput)
    );

    let mut provenance = report_for(&value, 1);
    provenance
        .structured_assertions
        .get_mut(&fingerprint)
        .expect("record")
        .vm_instances
        .insert(FORGED_VM_INSTANCE);
    assert_eq!(
        merge_oracle_reports(&[(0, provenance)]),
        Err(ReportMergeConflict::IneligibleInput)
    );
}

#[test]
fn rejects_forged_summary_counters_and_failure_details() {
    const FORGED_CATALOG_SIZE: usize = 2;
    const FORGED_SUMMARY_COUNT: usize = 2;
    let value = descriptor("stable:raft", "raft");
    let fingerprint = value.fingerprint().expect("fingerprint");
    let mut reports = Vec::new();

    let mut catalog_size = report_for(&value, 1);
    catalog_size.catalog_size = FORGED_CATALOG_SIZE;
    reports.push(catalog_size);
    let mut summary = report_for(&value, 1);
    summary.passed = FORGED_SUMMARY_COUNT;
    reports.push(summary);
    let mut hit_count = report_for(&value, 1);
    hit_count
        .structured_assertions
        .get_mut(&fingerprint)
        .expect("record")
        .hit_count += 1;
    reports.push(hit_count);
    let mut run_bound = report_for(&value, 1);
    run_bound
        .structured_assertions
        .get_mut(&fingerprint)
        .expect("record")
        .runs_hit += 1;
    reports.push(run_bound);
    let mut details = report_for(&value, 1);
    details
        .structured_assertions
        .get_mut(&fingerprint)
        .expect("record")
        .last_failure_details = Some(vec![0; MAX_ASSERTION_EVENT_DETAILS_BYTES + 1]);
    reports.push(details);

    for report in reports {
        assert_eq!(
            merge_oracle_reports(&[(0, report)]),
            Err(ReportMergeConflict::IneligibleInput)
        );
    }
}
