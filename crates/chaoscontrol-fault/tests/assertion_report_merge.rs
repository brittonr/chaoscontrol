mod assertion_report_merge_support;

use assertion_report_merge_support::*;
use chaoscontrol_fault::report_merge::{merge_oracle_reports, ReportMergeConflict};
use chaoscontrol_protocol::admission::{catalog_token, AdmittedAssertion, CatalogConflict};
use chaoscontrol_protocol::identity::{AssertionDescriptor, AssertionLogicalKey};
use std::collections::BTreeSet;

fn automatic_descriptor(namespace: &str, guest: &str) -> AssertionDescriptor {
    let mut value = descriptor(namespace, guest);
    value.logical_key = AssertionLogicalKey::Automatic {
        source_site: format!(
            "{}:{}:{}",
            value.source_file, value.source_line, value.source_column
        ),
    };
    value
}

fn legacy_descriptor(namespace: &str, guest: &str) -> AssertionDescriptor {
    let mut value = descriptor(namespace, guest);
    value.logical_key = AssertionLogicalKey::LegacyU32 {
        id: COMPATIBILITY_ID,
    };
    value
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
fn automatic_descriptor_aggregates_with_vm_provenance() {
    let value = automatic_descriptor("build:raft:1", "raft");
    let reports = [(0, report_for(&value, 1)), (1, report_for(&value, 1))];
    let merged = merge_oracle_reports(&reports).expect("merge automatic reports");
    assert_eq!(merged.structured_assertions.len(), 1);
    assert!(merged.collision_safe_evidence);
}

#[test]
fn same_compatibility_alias_in_distinct_namespaces_stays_separate() {
    let raft = descriptor("stable:raft", "raft");
    let redb = descriptor("stable:redb", "redb");
    let reports = [(0, report_for(&raft, 1)), (1, report_for(&redb, 1))];
    let merged = merge_oracle_reports(&reports).expect("namespace-aware merge");
    assert_eq!(merged.structured_assertions.len(), 2);
    assert_ne!(
        raft.fingerprint().expect("raft fingerprint"),
        redb.fingerprint().expect("redb fingerprint")
    );
    assert_eq!(
        merged.record_for_compatibility_id(COMPATIBILITY_ID),
        Err(CatalogConflict::CompatibilityAliasConflict)
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
fn legacy_u32_identity_is_rejected_before_controller_merge() {
    let stable = descriptor("stable:raft", "raft");
    let mut report = report_for(&stable, 1);
    let legacy = legacy_descriptor("legacy:raft", "raft");
    let fingerprint = legacy.fingerprint().expect("legacy fingerprint");
    let admitted = AdmittedAssertion {
        canonical_bytes: legacy.canonical_bytes().expect("legacy canonical"),
        descriptor: legacy,
        fingerprint,
    };
    let catalog = std::collections::BTreeMap::from([(fingerprint, admitted.clone())]);
    let token = catalog_token(&catalog);
    let mut record = report
        .structured_assertions
        .remove(&stable.fingerprint().expect("stable fingerprint"))
        .expect("strict record");
    record.identity = Some(admitted);
    record.catalog_tokens = BTreeSet::from([token]);
    report.structured_assertions.insert(fingerprint, record);

    assert_eq!(
        merge_oracle_reports(&[(0, report)]),
        Err(ReportMergeConflict::IneligibleInput)
    );
}

#[test]
fn legacy_report_is_never_promoted_or_selected() {
    let legacy = forged_legacy_report();
    assert_eq!(
        legacy.record_for_compatibility_id(COMPATIBILITY_ID),
        Err(CatalogConflict::CatalogStatusMismatch)
    );
    assert_eq!(
        merge_oracle_reports(&[(0, legacy)]),
        Err(ReportMergeConflict::IneligibleInput)
    );
}

#[test]
fn mixed_report_is_never_promoted_or_selected() {
    let value = descriptor("stable:mixed", "mixed");
    let mut mixed = report_for(&value, 1);
    let legacy = forged_legacy_report();
    mixed.assertions = legacy.assertions;

    assert_eq!(
        mixed.record_for_compatibility_id(COMPATIBILITY_ID),
        Err(CatalogConflict::CatalogStatusMismatch)
    );
    assert_eq!(
        merge_oracle_reports(&[(0, mixed)]),
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
