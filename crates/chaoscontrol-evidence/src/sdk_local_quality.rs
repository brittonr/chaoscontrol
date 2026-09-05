use crate::{EvidenceError, EvidenceResult};
use chaoscontrol_protocol::admission::{CatalogBuilder, MAX_ASSERTION_REPORT_ENTRIES};
use chaoscontrol_protocol::identity::{AssertionDescriptor, AssertionFingerprint, AssertionKind};
use serde::de::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const LOCAL_REPORT_SCHEMA: &str = "chaoscontrol.sdk.local_report.v2";
pub const LOCAL_REPLAY_BOUNDARY: &str = "local SDK JSONL proves instrumentation shape only; VM campaign and replay artifacts must be reviewed separately";
const LOCAL_EVIDENCE_CLASS: &str = "instrumentation-dry-run";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalReportIdentity {
    pub descriptor: AssertionDescriptor,
    pub fingerprint: AssertionFingerprint,
    pub canonical_descriptor: String,
    pub catalog_token: AssertionFingerprint,
}

impl LocalReportIdentity {
    pub fn from_resolved(
        resolved: &crate::sdk_local_identity::ResolvedLocalIdentity,
    ) -> EvidenceResult<Self> {
        let canonical = resolved
            .descriptor
            .canonical_bytes()
            .map_err(|error| EvidenceError::new(format!("invalid descriptor: {error}")))?;
        Ok(Self {
            descriptor: resolved.descriptor.clone(),
            fingerprint: resolved.fingerprint,
            canonical_descriptor: crate::sdk_local_identity_value::encode_hex(&canonical),
            catalog_token: resolved.catalog_token,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct QualityFacts {
    pub collision_safe: bool,
    pub setup_complete: bool,
    pub cataloged: u64,
    pub failed: u64,
    pub uncategorized: u64,
    pub unobserved: Vec<String>,
    pub reachable_without_hit: Vec<String>,
    pub sometimes_without_success: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalReportV2 {
    adoption_tracks: BTreeMap<String, u64>,
    assertion_coverage: Vec<CoverageEntry>,
    cataloged_assertions: u64,
    catalog_status: String,
    collision_safe_evidence: bool,
    evidence_class: String,
    exercised_assertions: u64,
    failed_assertions: u64,
    gaps: Vec<String>,
    instrumentation_sources: BTreeMap<String, u64>,
    lifecycle_events: BTreeMap<String, u64>,
    observed_assertions: u64,
    random_choice_calls: u64,
    reachable_without_hit: Vec<String>,
    registered_assertions: u64,
    replay_boundary: String,
    replay_evidence: bool,
    schema: String,
    setup_complete: bool,
    sometimes_without_success: Vec<String>,
    uncategorized_assertions: u64,
    unobserved_assertion_count: u64,
    unobserved_assertions: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CoverageEntry {
    id: String,
    message: String,
    assert_type: String,
    category: String,
    observed: bool,
    observed_hits: u64,
    success_count: u64,
    failure_count: u64,
    adoption_tracks: Vec<String>,
    identity: Option<LocalReportIdentity>,
}

pub(crate) fn validate_quality_report(value: &Value) -> EvidenceResult<QualityFacts> {
    let report = LocalReportV2::deserialize(value)
        .map_err(|error| EvidenceError::new(format!("invalid SDK local report v2: {error}")))?;
    if report.schema != LOCAL_REPORT_SCHEMA
        || report.replay_evidence
        || report.evidence_class != LOCAL_EVIDENCE_CLASS
        || report.replay_boundary != LOCAL_REPLAY_BOUNDARY
        || report.assertion_coverage.len() > MAX_ASSERTION_REPORT_ENTRIES
    {
        return Err(EvidenceError::new("invalid SDK local report v2 boundary"));
    }
    let identity_count = report
        .assertion_coverage
        .iter()
        .filter(|entry| entry.identity.is_some())
        .count();
    let collision_safe = if identity_count == 0 {
        if !matches!(
            report.catalog_status.as_str(),
            "legacy-ambiguous" | "pending"
        ) {
            return Err(EvidenceError::new(
                "legacy report has invalid catalog status",
            ));
        }
        false
    } else if identity_count == report.assertion_coverage.len() {
        validate_strict_entries(&report.assertion_coverage)?;
        if report.catalog_status != "accepted" {
            return Err(EvidenceError::new("strict report catalog is not accepted"));
        }
        true
    } else {
        validate_strict_entries(&report.assertion_coverage)?;
        if report.catalog_status != "legacy-ambiguous" {
            return Err(EvidenceError::new("mixed report is not quarantined"));
        }
        false
    };
    if report.collision_safe_evidence != collision_safe {
        return Err(EvidenceError::new(
            "caller-owned collision-safe marker conflicts with recomputed identity",
        ));
    }
    validate_derived_totals(&report)?;
    let setup_count = report
        .lifecycle_events
        .get("setup_complete")
        .copied()
        .unwrap_or(0);
    if setup_count > 1 || report.setup_complete != (setup_count == 1) {
        return Err(EvidenceError::new(
            "setup completion disagrees with validated lifecycle count",
        ));
    }
    let _review_only = (
        &report.adoption_tracks,
        &report.gaps,
        &report.instrumentation_sources,
        &report.lifecycle_events,
        report.random_choice_calls,
    );
    Ok(QualityFacts {
        collision_safe,
        setup_complete: report.setup_complete,
        cataloged: report.cataloged_assertions,
        failed: report.failed_assertions,
        uncategorized: report.uncategorized_assertions,
        unobserved: report.unobserved_assertions,
        reachable_without_hit: report.reachable_without_hit,
        sometimes_without_success: report.sometimes_without_success,
    })
}

fn validate_strict_entries(entries: &[CoverageEntry]) -> EvidenceResult<()> {
    if entries.is_empty() {
        return Err(EvidenceError::new("strict report catalog is empty"));
    }
    let strict_entries = entries
        .iter()
        .filter(|entry| entry.identity.is_some())
        .collect::<Vec<_>>();
    let mut fingerprints = BTreeSet::new();
    let mut catalog_token = None;
    let mut builder = CatalogBuilder::begin(strict_entries.len())
        .map_err(|error| EvidenceError::new(format!("invalid report catalog: {error:?}")))?;
    for entry in strict_entries {
        validate_entry_counts(entry)?;
        let identity = entry.identity.as_ref().expect("entry was filtered");
        let canonical = identity
            .descriptor
            .canonical_bytes()
            .map_err(|error| EvidenceError::new(format!("invalid descriptor: {error}")))?;
        let fingerprint = identity
            .descriptor
            .fingerprint()
            .map_err(|error| EvidenceError::new(format!("invalid fingerprint: {error}")))?;
        if fingerprint != identity.fingerprint
            || crate::sdk_local_identity_value::encode_hex(&canonical)
                != identity.canonical_descriptor
            || !fingerprints.insert(fingerprint)
            || entry.message != identity.descriptor.message
            || entry.assert_type
                != crate::sdk_local_identity_value::exact_kind(identity.descriptor.kind)
            || entry.category != identity.descriptor.category
            || entry.id
                != crate::sdk_local_identity_value::report_id(&identity.descriptor, fingerprint)
            || !crate::sdk_local_verdict::counts_match_kind(
                identity.descriptor.kind,
                entry.success_count,
                entry.failure_count,
            )
        {
            return Err(EvidenceError::new("report assertion identity mismatch"));
        }
        if catalog_token
            .replace(identity.catalog_token)
            .is_some_and(|token| token != identity.catalog_token)
        {
            return Err(EvidenceError::new("report catalog tokens disagree"));
        }
        builder
            .insert_with_fingerprint(identity.descriptor.clone(), fingerprint)
            .map_err(|error| EvidenceError::new(format!("report catalog conflict: {error:?}")))?;
    }
    builder
        .complete(catalog_token.expect("strict entries are non-empty"))
        .map_err(|error| EvidenceError::new(format!("invalid report token: {error:?}")))?;
    Ok(())
}

fn validate_derived_totals(report: &LocalReportV2) -> EvidenceResult<()> {
    let mut failed = 0_u64;
    let mut observed = 0_u64;
    let mut uncategorized = 0_u64;
    let mut unobserved = Vec::new();
    let mut reachable = Vec::new();
    let mut sometimes = Vec::new();
    for entry in &report.assertion_coverage {
        validate_entry_counts(entry)?;
        let kind = crate::sdk_local_verdict::report_kind(&entry.assert_type)
            .ok_or_else(|| EvidenceError::new("report assertion kind is unknown"))?;
        let verdict = crate::sdk_local_verdict::derive_local_verdict(
            kind,
            entry.success_count,
            entry.failure_count,
        );
        failed += u64::from(verdict == crate::sdk_local_verdict::LocalAssertionVerdict::Failed);
        observed += u64::from(entry.observed);
        uncategorized += u64::from(entry.category == "uncategorized");
        if crate::sdk_local_verdict::blocks_as_unobserved(kind, entry.observed) {
            unobserved.push(entry.message.clone());
        }
        if kind == AssertionKind::Reachable && entry.success_count == 0 {
            reachable.push(entry.message.clone());
        }
        if kind == AssertionKind::Sometimes && entry.success_count == 0 {
            sometimes.push(entry.message.clone());
        }
        let _review_only = &entry.adoption_tracks;
    }
    let cataloged = u64::try_from(report.assertion_coverage.len())
        .map_err(|_| EvidenceError::new("report catalog size overflow"))?;
    if report.cataloged_assertions != cataloged
        || report.registered_assertions != cataloged
        || report.exercised_assertions != observed
        || report.observed_assertions != observed
        || report.failed_assertions != failed
        || report.uncategorized_assertions != uncategorized
        || report.unobserved_assertion_count != unobserved.len() as u64
        || crate::sdk_local_verdict::sorted_strings(&report.unobserved_assertions)
            != crate::sdk_local_verdict::sorted_owned(unobserved)
        || crate::sdk_local_verdict::sorted_strings(&report.reachable_without_hit)
            != crate::sdk_local_verdict::sorted_owned(reachable)
        || crate::sdk_local_verdict::sorted_strings(&report.sometimes_without_success)
            != crate::sdk_local_verdict::sorted_owned(sometimes)
    {
        return Err(EvidenceError::new(
            "SDK local report derived totals disagree",
        ));
    }
    Ok(())
}

fn validate_entry_counts(entry: &CoverageEntry) -> EvidenceResult<()> {
    let hits = entry
        .success_count
        .checked_add(entry.failure_count)
        .ok_or_else(|| EvidenceError::new("assertion count overflow"))?;
    if hits != entry.observed_hits || entry.observed != (entry.observed_hits > 0) {
        return Err(EvidenceError::new("assertion coverage counts disagree"));
    }
    Ok(())
}
