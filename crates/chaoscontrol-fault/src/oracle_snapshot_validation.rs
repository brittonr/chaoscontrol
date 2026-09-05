pub fn validate_oracle_snapshot(
    snapshot: &crate::oracle::OracleSnapshot,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    crate::oracle_event_validation::validate_bounds(
        &snapshot.events,
        &snapshot.identity_conflicts,
        snapshot.total_runs,
    )?;
    if snapshot
        .assertions
        .len()
        .saturating_add(snapshot.structured_assertions.len())
        > ::chaoscontrol_protocol::admission::MAX_ASSERTION_CATALOG_ENTRIES
    {
        return Err(crate::oracle_validation::OracleValidationError::Cardinality);
    }
    match snapshot.catalog_status {
        ::chaoscontrol_protocol::admission::CatalogValidationStatus::Pending => {
            validate_pending_snapshot(snapshot)
        }
        ::chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted => {
            validate_accepted_snapshot(snapshot)
        }
        ::chaoscontrol_protocol::admission::CatalogValidationStatus::LegacyAmbiguous => {
            validate_legacy_snapshot(snapshot)
        }
        ::chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict => {
            validate_fatal_snapshot(snapshot)
        }
    }
}

pub fn validate_restorable_oracle_snapshot(
    snapshot: &crate::oracle::OracleSnapshot,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    validate_oracle_snapshot(snapshot)?;
    if !matches!(
        snapshot.catalog_status,
        ::chaoscontrol_protocol::admission::CatalogValidationStatus::Pending
            | ::chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted
    ) {
        return Err(crate::oracle_validation::OracleValidationError::Status);
    }
    Ok(())
}

pub fn validate_orchestration_oracle_snapshot(
    snapshot: &crate::oracle::OracleSnapshot,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    crate::oracle_event_validation::validate_bounds(
        &snapshot.events,
        &snapshot.identity_conflicts,
        snapshot.total_runs,
    )?;
    if snapshot.catalog_status
        != ::chaoscontrol_protocol::admission::CatalogValidationStatus::Pending
        || snapshot.accepted_catalog.is_some()
        || !snapshot.assertions.is_empty()
        || !snapshot.structured_assertions.is_empty()
        || !snapshot.identity_conflicts.is_empty()
        || !snapshot.events.is_empty()
        || snapshot.total_runs != 0
    {
        return Err(crate::oracle_validation::OracleValidationError::Status);
    }
    let run = snapshot
        .current_run
        .as_ref()
        .ok_or(crate::oracle_validation::OracleValidationError::Status)?;
    if run.run_id != 0
        || !run.strict_hit_ids.is_empty()
        || !run.strict_satisfied_ids.is_empty()
        || run.immediate_failure.is_some()
    {
        return Err(crate::oracle_validation::OracleValidationError::Status);
    }
    Ok(())
}

pub fn resolve_snapshot_assertion_evidence<'a>(
    snapshot: &'a crate::oracle::OracleSnapshot,
    identity: &::chaoscontrol_protocol::admission::AssertionEvidenceIdentity,
) -> Result<&'a crate::oracle::AssertionRecord, crate::oracle_validation::OracleValidationError> {
    validate_accepted_snapshot(snapshot)?;
    let catalog = snapshot
        .accepted_catalog
        .as_ref()
        .ok_or(crate::oracle_validation::OracleValidationError::Catalog)?;
    identity
        .validate_for_catalog(catalog)
        .map_err(|_| crate::oracle_validation::OracleValidationError::Catalog)?;
    snapshot
        .structured_assertions
        .get(&identity.fingerprint)
        .ok_or(crate::oracle_validation::OracleValidationError::Record)
}

fn validate_pending_snapshot(
    snapshot: &crate::oracle::OracleSnapshot,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    if snapshot.accepted_catalog.is_some()
        || !snapshot.assertions.is_empty()
        || !snapshot.structured_assertions.is_empty()
        || !snapshot.identity_conflicts.is_empty()
        || !snapshot.events.is_empty()
        || snapshot.total_runs != 0
        || snapshot.current_run.is_some()
    {
        return Err(crate::oracle_validation::OracleValidationError::Status);
    }
    Ok(())
}

fn validate_legacy_snapshot(
    snapshot: &crate::oracle::OracleSnapshot,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    if snapshot.accepted_catalog.is_some()
        || !snapshot.structured_assertions.is_empty()
        || snapshot.assertions.is_empty()
        || snapshot.identity_conflicts.is_empty()
        || snapshot.current_run.is_some()
    {
        return Err(crate::oracle_validation::OracleValidationError::LegacyState);
    }
    crate::oracle_record_validation::validate_legacy_records(
        &snapshot.assertions,
        snapshot.total_runs,
    )
}

fn validate_fatal_snapshot(
    snapshot: &crate::oracle::OracleSnapshot,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    if snapshot.identity_conflicts.is_empty() || snapshot.current_run.is_some() {
        return Err(crate::oracle_validation::OracleValidationError::ConflictState);
    }
    crate::oracle_record_validation::validate_legacy_records(
        &snapshot.assertions,
        snapshot.total_runs,
    )?;
    if let Some(catalog) = &snapshot.accepted_catalog {
        ::chaoscontrol_protocol::admission::validate_accepted_catalog(catalog)
            .map_err(|_| crate::oracle_validation::OracleValidationError::Catalog)?;
        crate::oracle_validation::validate_strict_records(
            &snapshot.structured_assertions,
            snapshot.total_runs,
            false,
            None,
        )?;
        validate_catalog_record_equality(catalog, &snapshot.structured_assertions)
    } else if snapshot.structured_assertions.is_empty() {
        Ok(())
    } else {
        Err(crate::oracle_validation::OracleValidationError::Catalog)
    }
}

fn validate_accepted_snapshot(
    snapshot: &crate::oracle::OracleSnapshot,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    if !snapshot.assertions.is_empty() || !snapshot.identity_conflicts.is_empty() {
        return Err(crate::oracle_validation::OracleValidationError::Status);
    }
    let catalog = snapshot
        .accepted_catalog
        .as_ref()
        .ok_or(crate::oracle_validation::OracleValidationError::Catalog)?;
    ::chaoscontrol_protocol::admission::validate_accepted_catalog(catalog)
        .map_err(|_| crate::oracle_validation::OracleValidationError::Catalog)?;
    let facts = crate::oracle_validation::validate_strict_records(
        &snapshot.structured_assertions,
        snapshot.total_runs,
        false,
        snapshot.current_run.as_ref(),
    )?;
    if facts.catalog_token != catalog.token || facts.catalog_size != catalog.assertions.len() {
        return Err(crate::oracle_validation::OracleValidationError::Catalog);
    }
    validate_catalog_record_equality(catalog, &snapshot.structured_assertions)?;
    validate_active_run(snapshot)
}

fn validate_active_run(
    snapshot: &crate::oracle::OracleSnapshot,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    let Some(run) = &snapshot.current_run else {
        return Ok(());
    };
    if run.run_id != snapshot.total_runs
        || !run.strict_satisfied_ids.is_subset(&run.strict_hit_ids)
        || run.strict_hit_ids.len()
            > ::chaoscontrol_protocol::admission::MAX_ASSERTION_CATALOG_ENTRIES
    {
        return Err(crate::oracle_validation::OracleValidationError::Counter);
    }
    for fingerprint in &run.strict_hit_ids {
        let record = snapshot
            .structured_assertions
            .get(fingerprint)
            .ok_or(crate::oracle_validation::OracleValidationError::Record)?;
        if record.hit_count == 0 {
            return Err(crate::oracle_validation::OracleValidationError::Counter);
        }
    }
    for fingerprint in &run.strict_satisfied_ids {
        if snapshot.structured_assertions[fingerprint].true_count == 0 {
            return Err(crate::oracle_validation::OracleValidationError::Counter);
        }
    }
    validate_immediate_failure(snapshot)
}

fn validate_immediate_failure(
    snapshot: &crate::oracle::OracleSnapshot,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    let Some(run) = &snapshot.current_run else {
        return Ok(());
    };
    let Some((fingerprint, message)) = &run.immediate_failure else {
        return Ok(());
    };
    let record = snapshot
        .structured_assertions
        .get(fingerprint)
        .ok_or(crate::oracle_validation::OracleValidationError::Record)?;
    if !run.strict_hit_ids.contains(fingerprint)
        || record.message != *message
        || record.false_count == 0
        || !matches!(
            record.kind,
            crate::oracle::AssertionKind::Always | crate::oracle::AssertionKind::Unreachable
        )
    {
        return Err(crate::oracle_validation::OracleValidationError::Record);
    }
    Ok(())
}

fn validate_catalog_record_equality(
    catalog: &::chaoscontrol_protocol::admission::AcceptedCatalog,
    records: &std::collections::BTreeMap<
        ::chaoscontrol_protocol::identity::AssertionFingerprint,
        crate::oracle::AssertionRecord,
    >,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    if catalog.assertions.len() != records.len() {
        return Err(crate::oracle_validation::OracleValidationError::Catalog);
    }
    for (fingerprint, admitted) in &catalog.assertions {
        let record = records
            .get(fingerprint)
            .ok_or(crate::oracle_validation::OracleValidationError::Record)?;
        if record.identity.as_ref() != Some(admitted)
            || record.message != admitted.descriptor.message
            || record.kind != admitted.descriptor.kind
            || record.guest != admitted.descriptor.guest
            || record.category != admitted.descriptor.category
            || record.compatibility_id != admitted.descriptor.compatibility_id
            || record.catalog_tokens != std::collections::BTreeSet::from([catalog.token])
            || !record.vm_instances.is_empty()
        {
            return Err(crate::oracle_validation::OracleValidationError::Record);
        }
        if record.fallback_scope.is_some()
            || admitted.descriptor.category
                == chaoscontrol_protocol::fallback::FALLBACK_ASSERTION_CATEGORY
        {
            let evidence_identity =
                chaoscontrol_protocol::admission::AssertionEvidenceIdentity::from_admitted(
                    admitted,
                    catalog.token,
                )
                .map_err(|_| crate::oracle_validation::OracleValidationError::Record)?;
            crate::oracle_record_validation::validate_strict_fallback_scope(
                record,
                &evidence_identity,
            )?;
        }
    }
    Ok(())
}
