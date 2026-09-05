pub const MAX_ORACLE_REPORTS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportMergeConflict {
    CardinalityOverflow,
    CatalogConflict,
    CounterOverflow,
    DescriptorConflict,
    DuplicateVmInstance,
    IneligibleInput,
}

pub fn merge_oracle_reports(
    reports: &[(u32, crate::oracle::OracleReport)],
) -> Result<crate::oracle::OracleReport, ReportMergeConflict> {
    let bounds = validate_report_set(reports)?;
    let mut assertions = std::collections::BTreeMap::new();
    let mut events = Vec::with_capacity(bounds.event_count);
    let mut total_runs = 0_u32;
    for (vm_instance, report) in reports {
        crate::oracle_validation::validate_strict_oracle_report(report)
            .map_err(|_| ReportMergeConflict::IneligibleInput)?;
        total_runs = total_runs.max(report.total_runs);
        events.extend(report.events.iter().cloned());
        for (fingerprint, record) in &report.structured_assertions {
            match assertions.get(fingerprint) {
                Some(existing) => {
                    let merged = merged_record(existing, record, *vm_instance)?;
                    assertions.insert(*fingerprint, merged);
                }
                None => {
                    let mut inserted = record.clone();
                    inserted.vm_instances = std::collections::BTreeSet::from([*vm_instance]);
                    inserted.catalog_tokens.clear();
                    assertions.insert(*fingerprint, inserted);
                }
            }
        }
    }
    let descriptors = assertions
        .values()
        .map(|record| {
            record
                .identity
                .as_ref()
                .map(|identity| identity.descriptor.clone())
                .ok_or(ReportMergeConflict::DescriptorConflict)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let merged_token = ::chaoscontrol_protocol::admission::token_for_descriptors(&descriptors)
        .map_err(|_| ReportMergeConflict::CatalogConflict)?;
    for record in assertions.values_mut() {
        record.catalog_tokens = std::collections::BTreeSet::from([merged_token]);
    }
    let (passed, failed, unexercised) = verdict_counts(&assertions);
    let mut output = crate::oracle::OracleReport {
        assertions: std::collections::BTreeMap::new(),
        catalog_size: assertions.len(),
        structured_assertions: assertions,
        catalog_status: ::chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted,
        identity_conflicts: Vec::new(),
        collision_safe_evidence: false,
        total_runs,
        passed,
        failed,
        unexercised,
        events,
    };
    crate::oracle_validation::validate_prepared_oracle_report(&output)
        .map_err(|_| ReportMergeConflict::IneligibleInput)?;
    output.collision_safe_evidence = true;
    Ok(output)
}

pub fn rejected_merge_report(conflict: ReportMergeConflict) -> crate::oracle::OracleReport {
    crate::oracle::OracleReport {
        assertions: std::collections::BTreeMap::new(),
        structured_assertions: std::collections::BTreeMap::new(),
        catalog_status: ::chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict,
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

struct ReportSetBounds {
    event_count: usize,
}

fn validate_report_set(
    reports: &[(u32, crate::oracle::OracleReport)],
) -> Result<ReportSetBounds, ReportMergeConflict> {
    if reports.is_empty() || reports.len() > MAX_ORACLE_REPORTS {
        return Err(ReportMergeConflict::CardinalityOverflow);
    }
    let mut vm_instances = std::collections::BTreeSet::new();
    let mut assertion_count = 0_usize;
    let mut event_count = 0_usize;
    for (vm_instance, report) in reports {
        if !vm_instances.insert(*vm_instance) {
            return Err(ReportMergeConflict::DuplicateVmInstance);
        }
        assertion_count = assertion_count
            .checked_add(report.structured_assertions.len())
            .ok_or(ReportMergeConflict::CardinalityOverflow)?;
        event_count = event_count
            .checked_add(report.events.len())
            .ok_or(ReportMergeConflict::CardinalityOverflow)?;
    }
    if assertion_count > ::chaoscontrol_protocol::admission::MAX_ASSERTION_REPORT_ENTRIES
        || event_count > crate::oracle_validation::MAX_ORACLE_EVENTS
    {
        return Err(ReportMergeConflict::CardinalityOverflow);
    }
    Ok(ReportSetBounds { event_count })
}

fn merged_record(
    existing: &crate::oracle::AssertionRecord,
    candidate: &crate::oracle::AssertionRecord,
    vm_instance: u32,
) -> Result<crate::oracle::AssertionRecord, ReportMergeConflict> {
    if existing.identity != candidate.identity
        || existing.kind != candidate.kind
        || existing.message != candidate.message
        || existing.guest != candidate.guest
        || existing.category != candidate.category
        || existing.compatibility_id != candidate.compatibility_id
        || existing.fallback_scope != candidate.fallback_scope
    {
        return Err(ReportMergeConflict::DescriptorConflict);
    }
    let mut merged = existing.clone();
    merged.hit_count = checked_sum(existing.hit_count, candidate.hit_count)?;
    merged.true_count = checked_sum(existing.true_count, candidate.true_count)?;
    merged.false_count = checked_sum(existing.false_count, candidate.false_count)?;
    merged.runs_hit = existing.runs_hit.max(candidate.runs_hit);
    merged.runs_satisfied = existing.runs_satisfied.max(candidate.runs_satisfied);
    merged.first_failure_run = match (existing.first_failure_run, candidate.first_failure_run) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    };
    if candidate.last_failure_details.is_some() {
        merged.last_failure_details = candidate.last_failure_details.clone();
    }
    merged.vm_instances.insert(vm_instance);
    merged
        .process_instances
        .extend(candidate.process_instances.iter().cloned());
    if merged.process_instances.len() > crate::oracle::MAX_PROCESS_INSTANCES_PER_ASSERTION {
        return Err(ReportMergeConflict::CardinalityOverflow);
    }
    merged.catalog_tokens.clear();
    Ok(merged)
}

fn checked_sum(left: u64, right: u64) -> Result<u64, ReportMergeConflict> {
    left.checked_add(right)
        .ok_or(ReportMergeConflict::CounterOverflow)
}

fn verdict_counts(
    assertions: &std::collections::BTreeMap<
        ::chaoscontrol_protocol::identity::AssertionFingerprint,
        crate::oracle::AssertionRecord,
    >,
) -> (usize, usize, usize) {
    assertions.values().fold((0, 0, 0), |mut counts, record| {
        match record.verdict() {
            crate::oracle::Verdict::Passed => counts.0 += 1,
            crate::oracle::Verdict::Failed => counts.1 += 1,
            crate::oracle::Verdict::Unexercised => counts.2 += 1,
        }
        counts
    })
}
