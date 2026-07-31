use crate::oracle::{AssertionRecord, OracleReport, OracleSnapshot};
pub use crate::oracle_event_validation::{MAX_IDENTITY_CONFLICTS, MAX_ORACLE_EVENTS};
use crate::oracle_record_validation::{validate_legacy_records, validate_record};
use chaoscontrol_protocol::assertion_catalog::{
    validate_accepted_catalog, CatalogBuilder, CatalogValidationStatus,
    MAX_ASSERTION_CATALOG_ENTRIES,
};
use chaoscontrol_protocol::assertion_identity::AssertionFingerprint;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleValidationError {
    Catalog,
    Cardinality,
    ConflictState,
    Counter,
    Event,
    LegacyState,
    Record,
    Status,
    Summary,
    VmProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrictReportFacts {
    pub catalog_token: AssertionFingerprint,
    pub catalog_size: usize,
}

pub fn validate_oracle_snapshot(snapshot: &OracleSnapshot) -> Result<(), OracleValidationError> {
    crate::oracle_event_validation::validate_bounds(
        &snapshot.events,
        &snapshot.identity_conflicts,
        snapshot.total_runs,
    )?;
    if snapshot
        .assertions
        .len()
        .saturating_add(snapshot.structured_assertions.len())
        > MAX_ASSERTION_CATALOG_ENTRIES
    {
        return Err(OracleValidationError::Cardinality);
    }
    match snapshot.catalog_status {
        CatalogValidationStatus::Pending => {
            if snapshot.accepted_catalog.is_some()
                || !snapshot.assertions.is_empty()
                || !snapshot.structured_assertions.is_empty()
                || !snapshot.identity_conflicts.is_empty()
            {
                return Err(OracleValidationError::Status);
            }
        }
        CatalogValidationStatus::Accepted => validate_accepted_snapshot(snapshot)?,
        CatalogValidationStatus::LegacyAmbiguous => {
            if snapshot.accepted_catalog.is_some()
                || !snapshot.structured_assertions.is_empty()
                || snapshot.assertions.is_empty()
                || snapshot.identity_conflicts.is_empty()
            {
                return Err(OracleValidationError::LegacyState);
            }
            validate_legacy_records(&snapshot.assertions, snapshot.total_runs)?;
        }
        CatalogValidationStatus::FatalConflict => {
            if snapshot.identity_conflicts.is_empty() {
                return Err(OracleValidationError::ConflictState);
            }
            validate_legacy_records(&snapshot.assertions, snapshot.total_runs)?;
            if let Some(catalog) = &snapshot.accepted_catalog {
                validate_accepted_catalog(catalog).map_err(|_| OracleValidationError::Catalog)?;
                validate_strict_records(
                    &snapshot.structured_assertions,
                    snapshot.total_runs,
                    false,
                )?;
                validate_catalog_record_equality(catalog, &snapshot.structured_assertions)?;
            } else if !snapshot.structured_assertions.is_empty() {
                return Err(OracleValidationError::Catalog);
            }
        }
    }
    Ok(())
}

pub fn validate_strict_oracle_report(
    report: &OracleReport,
) -> Result<StrictReportFacts, OracleValidationError> {
    validate_oracle_report(report, true)
}

pub fn validate_aggregated_oracle_report(
    report: &OracleReport,
) -> Result<StrictReportFacts, OracleValidationError> {
    validate_oracle_report(report, false)
}

fn validate_oracle_report(
    report: &OracleReport,
    reject_vm_provenance: bool,
) -> Result<StrictReportFacts, OracleValidationError> {
    if report.catalog_status != CatalogValidationStatus::Accepted
        || !report.assertions.is_empty()
        || !report.identity_conflicts.is_empty()
    {
        return Err(OracleValidationError::Status);
    }
    crate::oracle_event_validation::validate_bounds(
        &report.events,
        &report.identity_conflicts,
        report.total_runs,
    )?;
    let facts = validate_strict_records(
        &report.structured_assertions,
        report.total_runs,
        reject_vm_provenance,
    )?;
    if report.catalog_size != facts.catalog_size {
        return Err(OracleValidationError::Summary);
    }
    let (passed, failed, unexercised) = verdict_counts(&report.structured_assertions);
    if (report.passed, report.failed, report.unexercised) != (passed, failed, unexercised) {
        return Err(OracleValidationError::Summary);
    }
    Ok(facts)
}

fn validate_accepted_snapshot(snapshot: &OracleSnapshot) -> Result<(), OracleValidationError> {
    if !snapshot.assertions.is_empty() || !snapshot.identity_conflicts.is_empty() {
        return Err(OracleValidationError::Status);
    }
    let catalog = snapshot
        .accepted_catalog
        .as_ref()
        .ok_or(OracleValidationError::Catalog)?;
    validate_accepted_catalog(catalog).map_err(|_| OracleValidationError::Catalog)?;
    let facts =
        validate_strict_records(&snapshot.structured_assertions, snapshot.total_runs, false)?;
    if facts.catalog_token != catalog.token || facts.catalog_size != catalog.assertions.len() {
        return Err(OracleValidationError::Catalog);
    }
    validate_catalog_record_equality(catalog, &snapshot.structured_assertions)
}

fn validate_catalog_record_equality(
    catalog: &chaoscontrol_protocol::assertion_catalog::AcceptedCatalog,
    records: &BTreeMap<AssertionFingerprint, AssertionRecord>,
) -> Result<(), OracleValidationError> {
    if catalog.assertions.len() != records.len() {
        return Err(OracleValidationError::Catalog);
    }
    for (fingerprint, admitted) in &catalog.assertions {
        let record = records
            .get(fingerprint)
            .ok_or(OracleValidationError::Record)?;
        if record.identity.as_ref() != Some(admitted)
            || record.message != admitted.descriptor.message
            || record.kind != admitted.descriptor.kind
            || record.guest != admitted.descriptor.guest
            || record.category != admitted.descriptor.category
            || record.compatibility_id != admitted.descriptor.compatibility_id
            || record.catalog_tokens != BTreeSet::from([catalog.token])
            || !record.vm_instances.is_empty()
        {
            return Err(OracleValidationError::Record);
        }
    }
    Ok(())
}

fn validate_strict_records(
    records: &BTreeMap<AssertionFingerprint, AssertionRecord>,
    total_runs: u32,
    reject_vm_provenance: bool,
) -> Result<StrictReportFacts, OracleValidationError> {
    if records.is_empty() || records.len() > MAX_ASSERTION_CATALOG_ENTRIES {
        return Err(OracleValidationError::Cardinality);
    }
    let mut token = None;
    let mut builder =
        CatalogBuilder::begin(records.len()).map_err(|_| OracleValidationError::Catalog)?;
    for (fingerprint, record) in records {
        validate_record(record, total_runs)?;
        if reject_vm_provenance && !record.vm_instances.is_empty() {
            return Err(OracleValidationError::VmProvenance);
        }
        let identity = record
            .identity
            .as_ref()
            .ok_or(OracleValidationError::Record)?;
        if identity.fingerprint != *fingerprint
            || identity
                .descriptor
                .fingerprint()
                .map_err(|_| OracleValidationError::Record)?
                != *fingerprint
            || identity.canonical_bytes
                != identity
                    .descriptor
                    .canonical_bytes()
                    .map_err(|_| OracleValidationError::Record)?
            || record.message != identity.descriptor.message
            || record.kind != identity.descriptor.kind
            || record.guest != identity.descriptor.guest
            || record.category != identity.descriptor.category
            || record.compatibility_id != identity.descriptor.compatibility_id
        {
            return Err(OracleValidationError::Record);
        }
        if record.catalog_tokens.len() != 1 {
            return Err(OracleValidationError::Catalog);
        }
        let record_token = *record.catalog_tokens.iter().next().expect("one token");
        if token
            .replace(record_token)
            .is_some_and(|prior| prior != record_token)
        {
            return Err(OracleValidationError::Catalog);
        }
        builder
            .insert_with_fingerprint(identity.descriptor.clone(), *fingerprint)
            .map_err(|_| OracleValidationError::Catalog)?;
    }
    let catalog_token = token.ok_or(OracleValidationError::Catalog)?;
    builder
        .complete(catalog_token)
        .map_err(|_| OracleValidationError::Catalog)?;
    Ok(StrictReportFacts {
        catalog_token,
        catalog_size: records.len(),
    })
}

fn verdict_counts(
    assertions: &BTreeMap<AssertionFingerprint, AssertionRecord>,
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
