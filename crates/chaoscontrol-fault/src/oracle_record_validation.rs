use crate::oracle::AssertionRecord;
use crate::oracle_validation::OracleValidationError;
use chaoscontrol_protocol::assertion_identity::{AssertionKind, MAX_ASSERTION_EVENT_DETAILS_BYTES};
use std::collections::BTreeMap;

pub(crate) fn validate_legacy_records(
    records: &BTreeMap<u32, AssertionRecord>,
    total_runs: u32,
) -> Result<(), OracleValidationError> {
    for (id, record) in records {
        validate_record(record, total_runs)?;
        if record.identity.is_some()
            || record.compatibility_id != Some(*id)
            || !record.catalog_tokens.is_empty()
            || !record.vm_instances.is_empty()
        {
            return Err(OracleValidationError::LegacyState);
        }
    }
    Ok(())
}

pub(crate) fn validate_record(
    record: &AssertionRecord,
    total_runs: u32,
) -> Result<(), OracleValidationError> {
    let counted = record
        .true_count
        .checked_add(record.false_count)
        .ok_or(OracleValidationError::Counter)?;
    if counted != record.hit_count
        || record.runs_hit > total_runs
        || record.runs_satisfied > record.runs_hit
        || u64::from(record.runs_hit) > record.hit_count
        || u64::from(record.runs_satisfied) > record.true_count
        || record.first_failure_run.is_some_and(|run| run > total_runs)
        || record
            .last_failure_details
            .as_ref()
            .is_some_and(|details| details.len() > MAX_ASSERTION_EVENT_DETAILS_BYTES)
    {
        return Err(OracleValidationError::Counter);
    }
    let failure_run_required = match record.kind {
        AssertionKind::Always => record.false_count > 0,
        AssertionKind::Sometimes | AssertionKind::Reachable => false,
        AssertionKind::Unreachable => record.hit_count > 0,
    };
    if record.first_failure_run.is_some() != failure_run_required {
        return Err(OracleValidationError::Counter);
    }
    match record.kind {
        AssertionKind::Reachable
            if record.true_count != record.hit_count || record.false_count != 0 =>
        {
            Err(OracleValidationError::Counter)
        }
        AssertionKind::Unreachable
            if record.false_count != record.hit_count || record.true_count != 0 =>
        {
            Err(OracleValidationError::Counter)
        }
        _ => Ok(()),
    }
}
