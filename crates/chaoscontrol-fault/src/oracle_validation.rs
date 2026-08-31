use crate::oracle::{AssertionRecord, OracleReport};
pub use crate::oracle_event_validation::{MAX_IDENTITY_CONFLICTS, MAX_ORACLE_EVENTS};
use crate::oracle_record_validation::{validate_active_record, validate_final_record};
pub use crate::oracle_snapshot_validation::{
    validate_oracle_snapshot, validate_restorable_oracle_snapshot,
};
use chaoscontrol_protocol::admission::{
    AcceptedCatalog, AssertionEvidenceIdentity, CatalogBuilder, CatalogValidationStatus,
    MAX_ASSERTION_CATALOG_ENTRIES,
};
use chaoscontrol_protocol::identity::AssertionFingerprint;
use std::collections::BTreeMap;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrictReportFacts {
    pub catalog_token: AssertionFingerprint,
    pub catalog_size: usize,
    pub catalog: AcceptedCatalog,
}

pub fn validate_strict_oracle_report(
    report: &OracleReport,
) -> Result<StrictReportFacts, OracleValidationError> {
    validate_final_oracle_report(report, true)
}

pub fn validate_oracle_report_claim(
    report: &OracleReport,
) -> Result<StrictReportFacts, OracleValidationError> {
    validate_final_oracle_report(report, false)
}

pub fn resolve_assertion_evidence<'a>(
    report: &'a OracleReport,
    identity: &AssertionEvidenceIdentity,
) -> Result<&'a AssertionRecord, OracleValidationError> {
    let facts = validate_oracle_report_claim(report)?;
    identity
        .validate_for_catalog(&facts.catalog)
        .map_err(|_| OracleValidationError::Record)?;
    let record = report
        .structured_assertions
        .get(&identity.fingerprint)
        .ok_or(OracleValidationError::Record)?;
    let admitted = record
        .identity
        .as_ref()
        .ok_or(OracleValidationError::Record)?;
    if admitted.descriptor != identity.descriptor
        || admitted.fingerprint != identity.fingerprint
        || admitted.canonical_bytes != identity.canonical_descriptor
        || record.catalog_tokens.len() != 1
        || !record.catalog_tokens.contains(&identity.catalog_token)
    {
        return Err(OracleValidationError::Record);
    }
    Ok(record)
}

pub(crate) fn validate_prepared_oracle_report(
    report: &OracleReport,
) -> Result<StrictReportFacts, OracleValidationError> {
    validate_oracle_report_facts(report, false)
}

fn validate_final_oracle_report(
    report: &OracleReport,
    reject_vm_provenance: bool,
) -> Result<StrictReportFacts, OracleValidationError> {
    let facts = validate_oracle_report_facts(report, reject_vm_provenance)?;
    if !report.collision_safe_evidence {
        return Err(OracleValidationError::Status);
    }
    Ok(facts)
}

fn validate_oracle_report_facts(
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
        None,
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

pub(crate) fn validate_strict_records(
    records: &BTreeMap<AssertionFingerprint, AssertionRecord>,
    total_runs: u32,
    reject_vm_provenance: bool,
    active_run: Option<&crate::oracle::RunState>,
) -> Result<StrictReportFacts, OracleValidationError> {
    if records.is_empty() || records.len() > MAX_ASSERTION_CATALOG_ENTRIES {
        return Err(OracleValidationError::Cardinality);
    }
    let mut token = None;
    let mut builder =
        CatalogBuilder::begin(records.len()).map_err(|_| OracleValidationError::Catalog)?;
    for (fingerprint, record) in records {
        match active_run {
            Some(run) => validate_active_record(
                record,
                total_runs,
                run.strict_hit_ids.contains(fingerprint),
                run.strict_satisfied_ids.contains(fingerprint),
            )?,
            None => validate_final_record(record, total_runs)?,
        }
        if reject_vm_provenance && !record.vm_instances.is_empty() {
            return Err(OracleValidationError::VmProvenance);
        }
        if record.process_instances.len() > crate::oracle::MAX_PROCESS_INSTANCES_PER_ASSERTION
            || record
                .process_instances
                .iter()
                .any(|identity| !chaoscontrol_protocol::process::validate_process_token(identity))
        {
            return Err(OracleValidationError::Record);
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
        if record.fallback_scope.is_some()
            || identity.descriptor.category
                == chaoscontrol_protocol::fallback::FALLBACK_ASSERTION_CATEGORY
        {
            let evidence_identity =
                chaoscontrol_protocol::admission::AssertionEvidenceIdentity::from_admitted(
                    identity,
                    record_token,
                )
                .map_err(|_| OracleValidationError::Record)?;
            crate::oracle_record_validation::validate_strict_fallback_scope(
                record,
                &evidence_identity,
            )?;
        }
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
    let catalog = builder
        .complete(catalog_token)
        .map_err(|_| OracleValidationError::Catalog)?;
    Ok(StrictReportFacts {
        catalog_token,
        catalog_size: records.len(),
        catalog,
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
