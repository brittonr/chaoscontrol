mod assertion_report_merge_support;

use assertion_report_merge_support::*;
use chaoscontrol_fault::report_merge::{merge_oracle_reports, ReportMergeConflict};
use chaoscontrol_protocol::admission::CatalogConflict;
use chaoscontrol_protocol::identity::{AssertionFingerprint, MAX_ASSERTION_EVENT_DETAILS_BYTES};
use std::collections::BTreeSet;

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
fn active_run_report_cannot_be_merged() {
    let value = descriptor("stable:active", "active");
    let (mut oracle, event) = oracle_for(&value);
    oracle.begin_run();
    oracle
        .record_bound_event(&event, true, None)
        .expect("bound event");
    let active = oracle.report();

    assert!(!active.collision_safe_evidence);
    assert_eq!(
        merge_oracle_reports(&[(0, active)]),
        Err(ReportMergeConflict::IneligibleInput)
    );
}

#[test]
fn explicit_source_demotion_cannot_be_repromoted_or_selected() {
    let value = descriptor("stable:raft", "raft");
    let mut demoted = report_for(&value, 1);
    demoted.collision_safe_evidence = false;

    assert_eq!(
        demoted.record_for_compatibility_id(COMPATIBILITY_ID),
        Err(CatalogConflict::CatalogStatusMismatch)
    );
    assert_eq!(
        merge_oracle_reports(&[(0, demoted)]),
        Err(ReportMergeConflict::IneligibleInput)
    );

    let mut forged = forged_legacy_report();
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
fn final_reports_reject_zero_run_hits_and_failure_at_total_runs() {
    let value = descriptor("stable:final-counters", "final-counters");
    let fingerprint = value.fingerprint().expect("fingerprint");

    let mut zero_run = report_for(&value, 1);
    zero_run.total_runs = 0;
    let zero_record = zero_run
        .structured_assertions
        .get_mut(&fingerprint)
        .expect("zero-run record");
    zero_record.runs_hit = 0;
    zero_record.runs_satisfied = 0;
    assert_eq!(
        merge_oracle_reports(&[(0, zero_run)]),
        Err(ReportMergeConflict::IneligibleInput)
    );

    let mut out_of_range = report_for(&value, 1);
    let failure_run = out_of_range.total_runs;
    let failure_record = out_of_range
        .structured_assertions
        .get_mut(&fingerprint)
        .expect("failure record");
    failure_record.true_count = 0;
    failure_record.false_count = failure_record.hit_count;
    failure_record.runs_satisfied = 0;
    failure_record.first_failure_run = Some(failure_run);
    out_of_range.passed = 0;
    out_of_range.failed = 1;
    assert_eq!(
        merge_oracle_reports(&[(0, out_of_range)]),
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
