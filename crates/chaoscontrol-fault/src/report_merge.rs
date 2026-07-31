use crate::oracle::{AssertionRecord, OracleReport, Verdict};
use chaoscontrol_protocol::assertion_catalog::{
    CatalogValidationStatus, MAX_ASSERTION_REPORT_ENTRIES,
};
use chaoscontrol_protocol::assertion_identity::AssertionFingerprint;
use std::collections::BTreeMap;

pub const MAX_ORACLE_EVENTS: usize = 16_384;
pub const MAX_ORACLE_REPORTS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportMergeConflict {
    CardinalityOverflow,
    CounterOverflow,
    DescriptorConflict,
    FingerprintCollision,
    IneligibleInput,
    LegacyAmbiguous,
    MalformedIdentity,
}

pub fn merge_oracle_reports(
    reports: &[(u32, OracleReport)],
) -> Result<OracleReport, ReportMergeConflict> {
    if reports.len() > MAX_ORACLE_REPORTS {
        return Err(ReportMergeConflict::CardinalityOverflow);
    }
    let assertion_count = reports.iter().try_fold(0_usize, |total, (_, report)| {
        total.checked_add(report.structured_assertions.len())
    });
    let Some(assertion_count) = assertion_count else {
        return Err(ReportMergeConflict::CardinalityOverflow);
    };
    if assertion_count > MAX_ASSERTION_REPORT_ENTRIES {
        return Err(ReportMergeConflict::CardinalityOverflow);
    }
    let event_count = reports.iter().try_fold(0_usize, |total, (_, report)| {
        total.checked_add(report.events.len())
    });
    let Some(event_count) = event_count else {
        return Err(ReportMergeConflict::CardinalityOverflow);
    };
    if event_count > MAX_ORACLE_EVENTS {
        return Err(ReportMergeConflict::CardinalityOverflow);
    }

    let mut assertions = BTreeMap::new();
    let mut events = Vec::with_capacity(event_count);
    let mut total_runs = 0_u32;
    for (vm_instance, report) in reports {
        validate_input(report)?;
        total_runs = total_runs.max(report.total_runs);
        events.extend(report.events.iter().cloned());
        for (fingerprint, record) in &report.structured_assertions {
            validate_record(*fingerprint, record)?;
            match assertions.get_mut(fingerprint) {
                Some(existing) => merge_record(existing, record, *vm_instance)?,
                None => {
                    let mut inserted = record.clone();
                    inserted.vm_instances.insert(*vm_instance);
                    assertions.insert(*fingerprint, inserted);
                }
            }
        }
    }
    let (passed, failed, unexercised) = verdict_counts(&assertions);
    Ok(OracleReport {
        assertions: BTreeMap::new(),
        structured_assertions: assertions,
        catalog_status: if reports
            .iter()
            .any(|(_, report)| report.catalog_status == CatalogValidationStatus::Accepted)
        {
            CatalogValidationStatus::Accepted
        } else {
            CatalogValidationStatus::Pending
        },
        identity_conflicts: Vec::new(),
        collision_safe_evidence: reports
            .iter()
            .all(|(_, report)| report.collision_safe_evidence),
        total_runs,
        passed,
        failed,
        unexercised,
        catalog_size: assertion_count,
        events,
    })
}

pub fn rejected_merge_report(conflict: ReportMergeConflict) -> OracleReport {
    OracleReport {
        assertions: BTreeMap::new(),
        structured_assertions: BTreeMap::new(),
        catalog_status: CatalogValidationStatus::FatalConflict,
        identity_conflicts: vec![format!("report merge rejected: {conflict:?}")],
        collision_safe_evidence: false,
        total_runs: 0,
        passed: 0,
        failed: 0,
        unexercised: 0,
        catalog_size: 0,
        events: Vec::new(),
    }
}

fn validate_input(report: &OracleReport) -> Result<(), ReportMergeConflict> {
    if !report.assertions.is_empty() {
        return Err(ReportMergeConflict::LegacyAmbiguous);
    }
    if !report.identity_conflicts.is_empty()
        || report.catalog_status == CatalogValidationStatus::FatalConflict
        || report.catalog_status == CatalogValidationStatus::LegacyAmbiguous
    {
        return Err(ReportMergeConflict::IneligibleInput);
    }
    if report.catalog_status == CatalogValidationStatus::Pending
        && !report.structured_assertions.is_empty()
    {
        return Err(ReportMergeConflict::IneligibleInput);
    }
    Ok(())
}

fn validate_record(
    fingerprint: AssertionFingerprint,
    record: &AssertionRecord,
) -> Result<(), ReportMergeConflict> {
    let identity = record
        .identity
        .as_ref()
        .ok_or(ReportMergeConflict::MalformedIdentity)?;
    let canonical = identity
        .descriptor
        .canonical_bytes()
        .map_err(|_| ReportMergeConflict::MalformedIdentity)?;
    let computed = identity
        .descriptor
        .fingerprint()
        .map_err(|_| ReportMergeConflict::MalformedIdentity)?;
    if identity.canonical_bytes != canonical {
        return Err(ReportMergeConflict::DescriptorConflict);
    }
    if identity.fingerprint != fingerprint || computed != fingerprint {
        return Err(ReportMergeConflict::FingerprintCollision);
    }
    Ok(())
}

fn merge_record(
    existing: &mut AssertionRecord,
    candidate: &AssertionRecord,
    vm_instance: u32,
) -> Result<(), ReportMergeConflict> {
    if existing.identity != candidate.identity
        || existing.kind != candidate.kind
        || existing.message != candidate.message
        || existing.guest != candidate.guest
        || existing.category != candidate.category
        || existing.compatibility_id != candidate.compatibility_id
    {
        return Err(ReportMergeConflict::DescriptorConflict);
    }
    existing.hit_count = checked_sum(existing.hit_count, candidate.hit_count)?;
    existing.true_count = checked_sum(existing.true_count, candidate.true_count)?;
    existing.false_count = checked_sum(existing.false_count, candidate.false_count)?;
    existing.runs_hit = checked_sum_u32(existing.runs_hit, candidate.runs_hit)?;
    existing.runs_satisfied = checked_sum_u32(existing.runs_satisfied, candidate.runs_satisfied)?;
    existing.first_failure_run = match (existing.first_failure_run, candidate.first_failure_run) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    };
    if candidate.last_failure_details.is_some() {
        existing.last_failure_details = candidate.last_failure_details.clone();
    }
    existing
        .catalog_tokens
        .extend(candidate.catalog_tokens.iter().copied());
    existing.vm_instances.insert(vm_instance);
    Ok(())
}

fn checked_sum(left: u64, right: u64) -> Result<u64, ReportMergeConflict> {
    left.checked_add(right)
        .ok_or(ReportMergeConflict::CounterOverflow)
}

fn checked_sum_u32(left: u32, right: u32) -> Result<u32, ReportMergeConflict> {
    left.checked_add(right)
        .ok_or(ReportMergeConflict::CounterOverflow)
}

fn verdict_counts(
    assertions: &BTreeMap<AssertionFingerprint, AssertionRecord>,
) -> (usize, usize, usize) {
    let mut passed = 0_usize;
    let mut failed = 0_usize;
    let mut unexercised = 0_usize;
    for record in assertions.values() {
        match record.verdict() {
            Verdict::Passed => passed += 1,
            Verdict::Failed => failed += 1,
            Verdict::Unexercised => unexercised += 1,
        }
    }
    (passed, failed, unexercised)
}
