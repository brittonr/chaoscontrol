pub(crate) fn validate_legacy_records(
    records: &std::collections::BTreeMap<u32, crate::oracle::AssertionRecord>,
    total_runs: u32,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    for (id, record) in records {
        validate_final_record(record, total_runs)?;
        if record.identity.is_some()
            || record.compatibility_id != Some(*id)
            || !record.catalog_tokens.is_empty()
            || !record.vm_instances.is_empty()
            || !record.process_instances.is_empty()
            || record.fallback_scope.is_some()
        {
            return Err(crate::oracle_validation::OracleValidationError::LegacyState);
        }
    }
    Ok(())
}

pub(crate) fn validate_strict_fallback_scope(
    record: &crate::oracle::AssertionRecord,
    identity: &chaoscontrol_protocol::admission::AssertionEvidenceIdentity,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    let is_fallback = identity.descriptor.category
        == chaoscontrol_protocol::fallback::FALLBACK_ASSERTION_CATEGORY;
    match (is_fallback, record.fallback_scope.as_ref()) {
        (false, None) => Ok(()),
        (true, Some(scope)) => scope
            .validate_against(identity)
            .map_err(|_| crate::oracle_validation::OracleValidationError::Record),
        (false, Some(_)) | (true, None) => {
            Err(crate::oracle_validation::OracleValidationError::Record)
        }
    }
}

pub(crate) fn validate_final_record(
    record: &crate::oracle::AssertionRecord,
    total_runs: u32,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    validate_record_counters(record, total_runs)?;
    if record
        .first_failure_run
        .is_some_and(|run| run >= total_runs)
        || (record.hit_count > 0 && record.runs_hit == 0)
        || (record.true_count > 0 && record.runs_satisfied == 0)
    {
        return Err(crate::oracle_validation::OracleValidationError::Counter);
    }
    Ok(())
}

pub(crate) fn validate_active_record(
    record: &crate::oracle::AssertionRecord,
    total_runs: u32,
    hit_in_active_run: bool,
    satisfied_in_active_run: bool,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    validate_record_counters(record, total_runs)?;
    if record
        .first_failure_run
        .is_some_and(|run| run == total_runs && !hit_in_active_run)
        || (record.hit_count > 0 && record.runs_hit == 0 && !hit_in_active_run)
        || (record.true_count > 0 && record.runs_satisfied == 0 && !satisfied_in_active_run)
    {
        return Err(crate::oracle_validation::OracleValidationError::Counter);
    }
    Ok(())
}

fn validate_record_counters(
    record: &crate::oracle::AssertionRecord,
    total_runs: u32,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    let counted = record
        .true_count
        .checked_add(record.false_count)
        .ok_or(crate::oracle_validation::OracleValidationError::Counter)?;
    if counted != record.hit_count
        || record.runs_hit > total_runs
        || record.runs_satisfied > record.runs_hit
        || u64::from(record.runs_hit) > record.hit_count
        || u64::from(record.runs_satisfied) > record.true_count
        || record.first_failure_run.is_some_and(|run| run > total_runs)
        || record.last_failure_details.as_ref().is_some_and(|details| {
            details.len() > ::chaoscontrol_protocol::identity::MAX_ASSERTION_EVENT_DETAILS_BYTES
        })
    {
        return Err(crate::oracle_validation::OracleValidationError::Counter);
    }
    let failure_run_required = match record.kind {
        ::chaoscontrol_protocol::identity::AssertionKind::Always => record.false_count > 0,
        ::chaoscontrol_protocol::identity::AssertionKind::Sometimes
        | ::chaoscontrol_protocol::identity::AssertionKind::Reachable => false,
        ::chaoscontrol_protocol::identity::AssertionKind::Unreachable => record.hit_count > 0,
    };
    if record.first_failure_run.is_some() != failure_run_required {
        return Err(crate::oracle_validation::OracleValidationError::Counter);
    }
    match record.kind {
        ::chaoscontrol_protocol::identity::AssertionKind::Reachable
            if record.true_count != record.hit_count || record.false_count != 0 =>
        {
            Err(crate::oracle_validation::OracleValidationError::Counter)
        }
        ::chaoscontrol_protocol::identity::AssertionKind::Unreachable
            if record.false_count != record.hit_count || record.true_count != 0 =>
        {
            Err(crate::oracle_validation::OracleValidationError::Counter)
        }
        _ => Ok(()),
    }
}
