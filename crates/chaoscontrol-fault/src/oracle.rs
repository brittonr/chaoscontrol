//! Property oracle — tracks assertion satisfaction across runs.
//!
//! The oracle receives assertion events from the guest SDK (via the VMM)
//! and maintains per-assertion state.  After all runs complete, it
//! produces a verdict: which properties passed, failed, or were never
//! exercised.
//!
//! # Assertion semantics
//!
//! | Type         | Pass condition                                |
//! |--------------|-----------------------------------------------|
//! | `always`     | Condition was true every time, in every run   |
//! | `sometimes`  | Condition was true at least once, in any run  |
//! | `reachable`  | Point was reached at least once, in any run   |
//! | `unreachable`| Point was never reached in any run             |

use chaoscontrol_protocol::admission::{
    validate_accepted_catalog, AcceptedCatalog, AdmittedAssertion, BoundAssertionEvent,
    CatalogConflict, CatalogValidationStatus, MAX_ASSERTION_CATALOG_ENTRIES,
};
use chaoscontrol_protocol::branch_marker::{
    BranchMarker, BRANCH_MARKER_EVENT, BRANCH_MARKER_LIMIT_EVENT, MAX_MARKERS_PER_RUN,
};
use chaoscontrol_protocol::fallback::{
    catalog_with_fallback, validate_fallback_sink_evidence, FallbackAdmissionError,
    FallbackAssertionScope, FallbackError, FallbackErrorKind, FallbackRecordType,
    FallbackSinkEvidence,
};
use chaoscontrol_protocol::identity::AssertionFingerprint;
pub use chaoscontrol_protocol::identity::AssertionKind;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_PROCESS_INSTANCES_PER_ASSERTION: usize = 32;

fn serialize_json_value<S>(value: &serde_json::Value, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if serializer.is_human_readable() {
        value.serialize(serializer)
    } else {
        serde_json::to_vec(value)
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

fn deserialize_json_value<'de, D>(deserializer: D) -> Result<serde_json::Value, D::Error>
where
    D: Deserializer<'de>,
{
    if deserializer.is_human_readable() {
        serde_json::Value::deserialize(deserializer)
    } else {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        serde_json::from_slice(&bytes).map_err(serde::de::Error::custom)
    }
}

/// Records for a single assertion site across all runs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionRecord {
    /// Human-readable assertion message.
    pub message: String,
    /// What kind of assertion this is.
    pub kind: AssertionKind,
    /// Total number of times this assertion was evaluated.
    pub hit_count: u64,
    /// Number of times the condition was true (for always/sometimes).
    pub true_count: u64,
    /// Number of times the condition was false (for always/sometimes).
    pub false_count: u64,
    /// Number of distinct runs that evaluated this assertion.
    pub runs_hit: u32,
    /// Number of distinct runs where the condition was true at least once.
    pub runs_satisfied: u32,
    /// ID of the first run that caused a failure (if any).
    pub first_failure_run: Option<u32>,
    /// JSON bytes from the most recent failure (None if never failed).
    pub last_failure_details: Option<Vec<u8>>,
    /// Guest that owns this assertion.
    pub guest: String,
    /// Density category for assertion exercise reporting.
    pub category: String,
    /// Structured descriptor binding for strict records.
    #[serde(
        default = "no_admitted_assertion",
        skip_serializing_if = "Option::is_none"
    )]
    pub identity: Option<AdmittedAssertion>,
    /// Compatibility ID retained for existing CLI filters.
    #[serde(
        default = "no_compatibility_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub compatibility_id: Option<u32>,
    /// Catalog tokens that admitted this exact descriptor.
    #[serde(
        default = "empty_catalog_tokens",
        skip_serializing_if = "BTreeSet::is_empty"
    )]
    pub catalog_tokens: BTreeSet<AssertionFingerprint>,
    /// VM instances that contributed to an aggregated record.
    #[serde(
        default = "empty_vm_instances",
        skip_serializing_if = "BTreeSet::is_empty"
    )]
    pub vm_instances: BTreeSet<u32>,
    /// Guest process identities that emitted this assertion.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub process_instances: BTreeSet<String>,
    /// Exact process-local fallback binding, when this record came from a fallback sink.
    #[serde(default = "no_fallback_scope", skip_serializing_if = "Option::is_none")]
    pub fallback_scope: Option<FallbackAssertionScope>,
}

impl AssertionRecord {
    fn from_admitted(assertion: &AdmittedAssertion, catalog_token: AssertionFingerprint) -> Self {
        let descriptor = &assertion.descriptor;
        Self {
            message: descriptor.message.clone(),
            kind: descriptor.kind,
            hit_count: 0,
            true_count: 0,
            false_count: 0,
            runs_hit: 0,
            runs_satisfied: 0,
            first_failure_run: None,
            last_failure_details: None,
            guest: descriptor.guest.clone(),
            category: descriptor.category.clone(),
            identity: Some(assertion.clone()),
            compatibility_id: descriptor.compatibility_id,
            catalog_tokens: BTreeSet::from([catalog_token]),
            vm_instances: BTreeSet::new(),
            process_instances: BTreeSet::new(),
            fallback_scope: None,
        }
    }

    /// Evaluate the cross-run verdict for this assertion.
    pub fn verdict(&self) -> Verdict {
        match self.kind {
            AssertionKind::Always => {
                if self.hit_count == 0 {
                    Verdict::Unexercised
                } else if self.false_count == 0 {
                    Verdict::Passed
                } else {
                    Verdict::Failed
                }
            }
            AssertionKind::Sometimes => {
                if self.hit_count == 0 {
                    Verdict::Unexercised
                } else if self.true_count > 0 {
                    Verdict::Passed
                } else {
                    Verdict::Failed
                }
            }
            AssertionKind::Reachable => {
                if self.hit_count > 0 {
                    Verdict::Passed
                } else {
                    Verdict::Unexercised
                }
            }
            AssertionKind::Unreachable => {
                if self.hit_count == 0 {
                    Verdict::Passed
                } else {
                    Verdict::Failed
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssertionRecordKey {
    Legacy(u32),
    Structured(AssertionFingerprint),
}

/// Final verdict for an assertion after all runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Verdict {
    /// Assertion passed across all runs.
    Passed,
    /// Assertion failed in at least one run.
    Failed,
    /// Assertion was never evaluated in any run.
    Unexercised,
}

/// Per-run state for the oracle.  Created at the start of each run,
/// finalized at the end.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunState {
    /// Run index (0-based).
    pub(crate) run_id: u32,
    /// Strict assertion fingerprints that were hit during this run.
    pub(crate) strict_hit_ids: BTreeSet<AssertionFingerprint>,
    /// Strict fingerprints that were satisfied during this run.
    pub(crate) strict_satisfied_ids: BTreeSet<AssertionFingerprint>,
    /// Whether setup_complete was received.
    pub(crate) setup_complete: bool,
    /// The structured identity and message for an immediate assertion failure.
    pub(crate) immediate_failure: Option<(AssertionFingerprint, String)>,
}

/// The property oracle.  Tracks assertions across multiple runs.
///
/// A live oracle accepts counters only through an activated catalog and
/// [`PropertyOracle::record_bound_event`]. Integer aliases are selectors only.
#[derive(Debug, Clone)]
pub struct PropertyOracle {
    /// Authoritative records keyed by the full descriptor fingerprint.
    structured_assertions: BTreeMap<AssertionFingerprint, AssertionRecord>,
    /// Accepted catalog used to resolve strict runtime events.
    accepted_catalog: Option<AcceptedCatalog>,
    /// Current assertion identity state.
    catalog_status: CatalogValidationStatus,
    /// Fatal identity diagnostics. Any entry makes evidence ineligible.
    identity_conflicts: Vec<String>,
    /// Current run state (None if between runs).
    current_run: Option<RunState>,
    /// Total number of completed runs.
    total_runs: u32,
    /// Lifecycle events received.
    events: Vec<OracleEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackOracleError {
    Admission(FallbackAdmissionError),
    Catalog(CatalogConflict),
    Record {
        record_index: u64,
        kind: FallbackErrorKind,
    },
    Sink(FallbackError),
}

impl core::fmt::Display for FallbackOracleError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "fallback oracle error: {self:?}")
    }
}

impl std::error::Error for FallbackOracleError {}

/// An event recorded by the oracle.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleEvent {
    /// Run in which this event occurred.
    pub run_id: u32,
    /// Event name.
    pub name: String,
    /// Structured JSON details.
    #[serde(
        serialize_with = "serialize_json_value",
        deserialize_with = "deserialize_json_value"
    )]
    pub details: serde_json::Value,
}

fn no_admitted_assertion() -> Option<AdmittedAssertion> {
    None
}

fn no_compatibility_id() -> Option<u32> {
    None
}

fn empty_catalog_tokens() -> BTreeSet<AssertionFingerprint> {
    BTreeSet::new()
}

fn empty_vm_instances() -> BTreeSet<u32> {
    BTreeSet::new()
}

fn no_fallback_scope() -> Option<FallbackAssertionScope> {
    None
}

fn empty_structured_assertions() -> BTreeMap<AssertionFingerprint, AssertionRecord> {
    BTreeMap::new()
}

fn empty_identity_conflicts() -> Vec<String> {
    Vec::new()
}

fn no_accepted_catalog() -> Option<AcceptedCatalog> {
    None
}

fn no_current_run() -> Option<RunState> {
    None
}

fn pending_catalog_status() -> CatalogValidationStatus {
    CatalogValidationStatus::Pending
}

/// Report produced by the oracle after all runs.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleReport {
    /// Diagnostic-only legacy records.
    pub assertions: BTreeMap<u32, AssertionRecord>,
    /// Collision-safe assertion records.
    #[serde(default = "empty_structured_assertions")]
    pub structured_assertions: BTreeMap<AssertionFingerprint, AssertionRecord>,
    /// Catalog state at report creation.
    #[serde(default = "pending_catalog_status")]
    pub catalog_status: CatalogValidationStatus,
    /// Fatal assertion identity diagnostics.
    #[serde(default = "empty_identity_conflicts")]
    pub identity_conflicts: Vec<String>,
    /// True only for the structured subset bound to one accepted strict catalog.
    #[serde(default)]
    pub collision_safe_evidence: bool,
    /// Total number of runs.
    pub total_runs: u32,
    /// Number of assertions that passed.
    pub passed: usize,
    /// Number of assertions that failed.
    pub failed: usize,
    /// Number of assertions never exercised.
    pub unexercised: usize,
    /// Total registered assertion sites (catalog + runtime).
    pub catalog_size: usize,
    /// All lifecycle events.
    pub events: Vec<OracleEvent>,
}

impl OracleReport {
    pub fn empty() -> Self {
        Self {
            assertions: BTreeMap::new(),
            structured_assertions: BTreeMap::new(),
            catalog_status: CatalogValidationStatus::Pending,
            identity_conflicts: Vec::new(),
            collision_safe_evidence: false,
            total_runs: 0,
            passed: 0,
            failed: 0,
            unexercised: 0,
            catalog_size: 0,
            events: Vec::new(),
        }
    }

    pub fn all_records(&self) -> impl Iterator<Item = (AssertionRecordKey, &AssertionRecord)> {
        self.assertions
            .iter()
            .map(|(id, record)| (AssertionRecordKey::Legacy(*id), record))
            .chain(
                self.structured_assertions
                    .iter()
                    .map(|(fingerprint, record)| {
                        (AssertionRecordKey::Structured(*fingerprint), record)
                    }),
            )
    }

    /// Select one validated structured record by its non-authoritative alias.
    pub fn record_for_compatibility_id(
        &self,
        compatibility_id: u32,
    ) -> Result<Option<&AssertionRecord>, CatalogConflict> {
        crate::oracle_validation::validate_oracle_report_claim(self)
            .map_err(|_| CatalogConflict::CatalogStatusMismatch)?;
        let mut records = self
            .structured_assertions
            .values()
            .filter(|record| record.compatibility_id == Some(compatibility_id));
        let first = records.next();
        if records.next().is_some() {
            return Err(CatalogConflict::CompatibilityAliasConflict);
        }
        Ok(first)
    }
}

impl PropertyOracle {
    /// Create a new oracle with no recorded state.
    pub fn new() -> Self {
        Self {
            structured_assertions: BTreeMap::new(),
            accepted_catalog: None,
            catalog_status: CatalogValidationStatus::Pending,
            identity_conflicts: Vec::new(),
            current_run: None,
            total_runs: 0,
            events: Vec::new(),
        }
    }

    /// Begin a new run.  Must be called before recording assertions.
    pub fn begin_run(&mut self) {
        let run_id = self.total_runs;
        self.current_run = Some(RunState {
            run_id,
            strict_hit_ids: BTreeSet::new(),
            strict_satisfied_ids: BTreeSet::new(),
            setup_complete: false,
            immediate_failure: None,
        });
    }

    /// End the current run and finalize per-run counters.
    pub fn end_run(&mut self) {
        let Some(run) = self.current_run.as_ref() else {
            return;
        };
        let Some(total_runs) = self.total_runs.checked_add(1) else {
            self.mark_identity_conflict(CatalogConflict::CounterOverflow);
            return;
        };
        let strict_updates = match prepare_run_updates(
            &self.structured_assertions,
            &run.strict_hit_ids,
            &run.strict_satisfied_ids,
        ) {
            Ok(updates) => updates,
            Err(conflict) => {
                self.mark_identity_conflict(conflict);
                return;
            }
        };
        for (fingerprint, runs_hit, runs_satisfied) in strict_updates {
            if let Some(record) = self.structured_assertions.get_mut(&fingerprint) {
                record.runs_hit = runs_hit;
                record.runs_satisfied = runs_satisfied;
            }
        }
        self.total_runs = total_runs;
        self.current_run = None;
    }

    /// Whether the current run has had an immediate failure.
    pub fn has_immediate_failure(&self) -> bool {
        self.current_run
            .as_ref()
            .is_some_and(|r| r.immediate_failure.is_some())
    }

    /// Get the immediate failure details for the current run.
    pub fn immediate_failure(&self) -> Option<(AssertionFingerprint, &str)> {
        self.current_run
            .as_ref()
            .and_then(|run| run.immediate_failure.as_ref())
            .map(|(fingerprint, message)| (*fingerprint, message.as_str()))
    }

    // ── Catalog registration ────────────────────────────────────

    /// Build and activate one catalog that includes all assertion records in a validated fallback sink.
    pub fn activate_catalog_with_fallback(
        &mut self,
        base: &AcceptedCatalog,
        evidence: &FallbackSinkEvidence,
    ) -> Result<(), FallbackOracleError> {
        let catalog =
            catalog_with_fallback(base, evidence).map_err(FallbackOracleError::Admission)?;
        self.activate_catalog(catalog)
            .map_err(FallbackOracleError::Catalog)
    }

    pub fn activate_catalog(&mut self, catalog: AcceptedCatalog) -> Result<(), CatalogConflict> {
        if let Err(conflict) = validate_accepted_catalog(&catalog) {
            self.mark_identity_conflict(conflict.clone());
            return Err(conflict);
        }
        if self.catalog_status == CatalogValidationStatus::FatalConflict {
            return Err(CatalogConflict::PostConflict);
        }
        if self.accepted_catalog.is_some() {
            self.mark_identity_conflict(CatalogConflict::AlreadyBegun);
            return Err(CatalogConflict::AlreadyBegun);
        }
        if catalog.assertions.len() > MAX_ASSERTION_CATALOG_ENTRIES {
            self.mark_identity_conflict(CatalogConflict::CardinalityOverflow);
            return Err(CatalogConflict::CardinalityOverflow);
        }
        let mut records = BTreeMap::new();
        for (fingerprint, assertion) in &catalog.assertions {
            records.insert(
                *fingerprint,
                AssertionRecord::from_admitted(assertion, catalog.token),
            );
        }
        self.structured_assertions = records;
        self.catalog_status = CatalogValidationStatus::Accepted;
        self.accepted_catalog = Some(catalog);
        Ok(())
    }

    pub fn mark_identity_conflict(&mut self, conflict: CatalogConflict) {
        self.catalog_status = CatalogValidationStatus::FatalConflict;
        if self.identity_conflicts.len() < crate::oracle_validation::MAX_IDENTITY_CONFLICTS {
            self.identity_conflicts.push(format!("{conflict:?}"));
        }
    }

    pub fn record_bound_event(
        &mut self,
        event: &BoundAssertionEvent,
        condition: bool,
        details: Option<&[u8]>,
    ) -> Result<bool, CatalogConflict> {
        self.record_bound_event_core(event, None, condition, details)
    }

    pub fn record_bound_event_with_compatibility(
        &mut self,
        event: &BoundAssertionEvent,
        compatibility_id: u32,
        condition: bool,
        details: Option<&[u8]>,
    ) -> Result<bool, CatalogConflict> {
        self.record_bound_event_core(event, Some(compatibility_id), condition, details)
    }

    fn record_bound_event_core(
        &mut self,
        event: &BoundAssertionEvent,
        compatibility_id: Option<u32>,
        condition: bool,
        details: Option<&[u8]>,
    ) -> Result<bool, CatalogConflict> {
        let Some(run_id) = self.current_run.as_ref().map(|run| run.run_id) else {
            return self.reject_bound_event(CatalogConflict::NoActiveRun);
        };
        let admitted = match self
            .accepted_catalog
            .as_ref()
            .ok_or(CatalogConflict::CatalogIncomplete)
            .and_then(|catalog| catalog.resolve_event(event))
        {
            Ok(assertion) => assertion.clone(),
            Err(conflict) => return self.reject_bound_event(conflict),
        };
        if compatibility_id.is_some() && admitted.descriptor.compatibility_id != compatibility_id {
            return self.reject_bound_event(CatalogConflict::CompatibilityAliasConflict);
        }
        if details.is_some_and(|value| {
            value.len() > chaoscontrol_protocol::identity::MAX_ASSERTION_EVENT_DETAILS_BYTES
        }) {
            return self.reject_bound_event(CatalogConflict::Descriptor(
                chaoscontrol_protocol::identity::AssertionError::FieldTooLong("event_details"),
            ));
        }
        let process_identity = details
            .and_then(|value| serde_json::from_slice::<serde_json::Value>(value).ok())
            .and_then(|value| {
                value
                    .get("chaoscontrol_process_identity")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            });
        if process_identity.as_deref().is_some_and(|identity| {
            !chaoscontrol_protocol::process::validate_process_token(identity)
        }) {
            return self.reject_bound_event(CatalogConflict::Descriptor(
                chaoscontrol_protocol::identity::AssertionError::MalformedCanonical,
            ));
        }
        let Some(record) = self.structured_assertions.get(&event.fingerprint) else {
            return self.reject_bound_event(CatalogConflict::UnknownFingerprint);
        };
        if record.identity.as_ref() != Some(&admitted) {
            return self.reject_bound_event(CatalogConflict::FingerprintCollision);
        }
        if process_identity.as_ref().is_some_and(|identity| {
            !record.process_instances.contains(identity)
                && record.process_instances.len() >= MAX_PROCESS_INSTANCES_PER_ASSERTION
        }) {
            return self.reject_bound_event(CatalogConflict::CardinalityOverflow);
        }
        let satisfied = match admitted.descriptor.kind {
            AssertionKind::Always | AssertionKind::Sometimes => condition,
            AssertionKind::Reachable => true,
            AssertionKind::Unreachable => false,
        };
        let Some(hit_count) = record.hit_count.checked_add(1) else {
            return self.reject_bound_event(CatalogConflict::CounterOverflow);
        };
        let true_count = if satisfied {
            match record.true_count.checked_add(1) {
                Some(count) => count,
                None => return self.reject_bound_event(CatalogConflict::CounterOverflow),
            }
        } else {
            record.true_count
        };
        let false_count = if satisfied {
            record.false_count
        } else {
            match record.false_count.checked_add(1) {
                Some(count) => count,
                None => return self.reject_bound_event(CatalogConflict::CounterOverflow),
            }
        };
        let immediate_failure = !satisfied
            && matches!(
                admitted.descriptor.kind,
                AssertionKind::Always | AssertionKind::Unreachable
            );
        let first_failure_run = if immediate_failure && record.first_failure_run.is_none() {
            Some(run_id)
        } else {
            record.first_failure_run
        };
        let last_failure_details = if !satisfied {
            details
                .map(|value| value.to_vec())
                .or_else(|| record.last_failure_details.clone())
        } else {
            record.last_failure_details.clone()
        };
        let record = self
            .structured_assertions
            .get_mut(&event.fingerprint)
            .expect("record was validated before mutation");
        record.hit_count = hit_count;
        record.true_count = true_count;
        record.false_count = false_count;
        record.first_failure_run = first_failure_run;
        record.last_failure_details = last_failure_details;
        if let Some(identity) = process_identity {
            record.process_instances.insert(identity);
        }
        let run = self
            .current_run
            .as_mut()
            .expect("active run was validated before mutation");
        run.strict_hit_ids.insert(event.fingerprint);
        if satisfied {
            run.strict_satisfied_ids.insert(event.fingerprint);
        }
        if immediate_failure {
            run.immediate_failure = Some((event.fingerprint, admitted.descriptor.message.clone()));
        }
        Ok(satisfied)
    }

    fn reject_bound_event<T>(&mut self, conflict: CatalogConflict) -> Result<T, CatalogConflict> {
        self.mark_identity_conflict(conflict.clone());
        Err(conflict)
    }

    /// Ingest one validated fallback sink into the active run in exact sink order.
    ///
    /// The update is transactional. A malformed record, identity mismatch, or
    /// oracle rejection leaves the original oracle unchanged.
    pub fn record_fallback_sink(
        &mut self,
        evidence: &FallbackSinkEvidence,
    ) -> Result<(), FallbackOracleError> {
        validate_fallback_sink_evidence(evidence).map_err(FallbackOracleError::Sink)?;
        let mut staged = self.clone();
        let catalog_token = staged
            .accepted_catalog
            .as_ref()
            .ok_or(FallbackOracleError::Catalog(
                CatalogConflict::CatalogIncomplete,
            ))?
            .token;
        for record in &evidence.records {
            if record.record_type == FallbackRecordType::Lifecycle {
                staged
                    .record_event(
                        &record.logical_key,
                        serde_json::json!({
                            "process": record.process,
                            "message": record.message,
                            "details": record.details,
                            "fallback_sink_blake3": evidence.sink_blake3,
                            "fallback_record_blake3": record.record_blake3().map_err(|kind| {
                                FallbackOracleError::Record {
                                    record_index: record.sequence,
                                    kind,
                                }
                            })?,
                        }),
                    )
                    .map_err(FallbackOracleError::Catalog)?;
                continue;
            }
            let descriptor = record
                .assertion_descriptor()
                .map_err(|kind| FallbackOracleError::Record {
                    record_index: record.sequence,
                    kind,
                })?
                .ok_or(FallbackOracleError::Record {
                    record_index: record.sequence,
                    kind: FallbackErrorKind::AssertionIdentityMismatch,
                })?;
            let fingerprint = descriptor
                .fingerprint()
                .map_err(FallbackErrorKind::Descriptor)
                .map_err(|kind| FallbackOracleError::Record {
                    record_index: record.sequence,
                    kind,
                })?;
            let kind = record
                .record_type
                .assertion_kind()
                .ok_or(FallbackOracleError::Record {
                    record_index: record.sequence,
                    kind: FallbackErrorKind::AssertionIdentityMismatch,
                })?;
            let event = BoundAssertionEvent {
                catalog_token,
                fingerprint,
                kind,
            };
            let details =
                serde_json::to_vec(&record.details).map_err(|_| FallbackOracleError::Record {
                    record_index: record.sequence,
                    kind: FallbackErrorKind::MalformedDetails,
                })?;
            let satisfied = staged
                .record_bound_event(&event, record.condition.unwrap_or(true), Some(&details))
                .map_err(FallbackOracleError::Catalog)?;
            let admitted = staged
                .accepted_catalog
                .as_ref()
                .and_then(|catalog| catalog.assertions.get(&fingerprint))
                .ok_or(FallbackOracleError::Catalog(
                    CatalogConflict::UnknownFingerprint,
                ))?;
            let identity =
                chaoscontrol_protocol::admission::AssertionEvidenceIdentity::from_admitted(
                    admitted,
                    catalog_token,
                )
                .map_err(FallbackOracleError::Catalog)?;
            let scope = record
                .assertion_scope(&evidence.sink_blake3)
                .map_err(|kind| FallbackOracleError::Record {
                    record_index: record.sequence,
                    kind,
                })?
                .ok_or(FallbackOracleError::Record {
                    record_index: record.sequence,
                    kind: FallbackErrorKind::AssertionIdentityMismatch,
                })?;
            scope
                .validate_against(&identity)
                .map_err(|kind| FallbackOracleError::Record {
                    record_index: record.sequence,
                    kind,
                })?;
            let oracle_record = staged.structured_assertions.get_mut(&fingerprint).ok_or(
                FallbackOracleError::Catalog(CatalogConflict::UnknownFingerprint),
            )?;
            if oracle_record.fallback_scope.is_none() || !satisfied {
                oracle_record.fallback_scope = Some(scope);
            }
        }
        if let Some(overflow) = &evidence.overflow {
            staged
                .record_event(
                    "fallback_assertion_sink_overflow",
                    serde_json::json!({
                        "limit": overflow.limit,
                        "rejected_sequence": overflow.rejected_sequence,
                        "process": overflow.process,
                        "fallback_sink_blake3": evidence.sink_blake3,
                    }),
                )
                .map_err(FallbackOracleError::Catalog)?;
        }
        *self = staged;
        Ok(())
    }

    /// Record a `setup_complete` lifecycle event.
    pub fn record_setup_complete(&mut self) -> Result<(), CatalogConflict> {
        let Some(run) = &mut self.current_run else {
            return self.reject_bound_event(CatalogConflict::NoActiveRun);
        };
        run.setup_complete = true;
        Ok(())
    }

    /// Clear setup state for a restarted active run.
    pub fn reset_setup_complete(&mut self) {
        if let Some(run) = &mut self.current_run {
            run.setup_complete = false;
        }
    }

    /// Whether the current run's setup phase is complete.
    pub fn is_setup_complete(&self) -> bool {
        self.current_run
            .as_ref()
            .is_some_and(|run| run.setup_complete)
    }

    /// Record a lifecycle event.
    pub fn record_event(
        &mut self,
        name: &str,
        details: serde_json::Value,
    ) -> Result<(), CatalogConflict> {
        let Some(run_id) = self.current_run.as_ref().map(|run| run.run_id) else {
            return self.reject_bound_event(CatalogConflict::NoActiveRun);
        };
        if name == BRANCH_MARKER_EVENT {
            let marker =
                BranchMarker::from_value(&details).map_err(|_| CatalogConflict::MarkerInvalid)?;
            let mut marker_count = 0_usize;
            for event in self
                .events
                .iter()
                .filter(|event| event.run_id == run_id && event.name == BRANCH_MARKER_EVENT)
            {
                marker_count = marker_count
                    .checked_add(1)
                    .ok_or(CatalogConflict::CounterOverflow)?;
                if BranchMarker::from_value(&event.details)
                    .is_ok_and(|existing| existing.collapse_key() == marker.collapse_key())
                {
                    return Ok(());
                }
            }
            if marker_count >= MAX_MARKERS_PER_RUN {
                let limit_recorded = self
                    .events
                    .iter()
                    .any(|event| event.run_id == run_id && event.name == BRANCH_MARKER_LIMIT_EVENT);
                if !limit_recorded {
                    self.events.push(OracleEvent {
                        run_id,
                        name: BRANCH_MARKER_LIMIT_EVENT.to_string(),
                        details: serde_json::json!({
                            "limit": MAX_MARKERS_PER_RUN,
                            "rejected_identity": marker.identity,
                        }),
                    });
                }
                return Err(CatalogConflict::MarkerLimitExceeded);
            }
        }
        self.events.push(OracleEvent {
            run_id,
            name: name.to_string(),
            details,
        });
        Ok(())
    }

    // ── Reporting ───────────────────────────────────────────────

    /// Produce a finalized report projection without changing live run state.
    pub fn finalized_report_projection(&self) -> OracleReport {
        let mut finalized = self.clone();
        if finalized.current_run.is_some() {
            finalized.end_run();
        }
        finalized.report()
    }

    /// Produce a summary report of all completed runs.
    pub fn report(&self) -> OracleReport {
        let mut passed = 0;
        let mut failed = 0;
        let mut unexercised = 0;

        for record in self.structured_assertions.values() {
            match record.verdict() {
                Verdict::Passed => passed += 1,
                Verdict::Failed => failed += 1,
                Verdict::Unexercised => unexercised += 1,
            }
        }
        let mut report = OracleReport {
            assertions: BTreeMap::new(),
            structured_assertions: self.structured_assertions.clone(),
            catalog_status: self.catalog_status,
            identity_conflicts: self.identity_conflicts.clone(),
            collision_safe_evidence: false,
            total_runs: self.total_runs,
            passed,
            failed,
            unexercised,
            catalog_size: self.structured_assertions.len(),
            events: self.events.clone(),
        };
        report.collision_safe_evidence = self.current_run.is_none()
            && crate::oracle_validation::validate_prepared_oracle_report(&report).is_ok();
        report
    }

    pub fn structured_assertions(&self) -> &BTreeMap<AssertionFingerprint, AssertionRecord> {
        &self.structured_assertions
    }

    pub fn catalog_status(&self) -> CatalogValidationStatus {
        self.catalog_status
    }

    pub fn accepted_catalog(&self) -> Option<&AcceptedCatalog> {
        self.accepted_catalog.as_ref()
    }

    /// Total number of completed runs.
    pub fn total_runs(&self) -> u32 {
        self.total_runs
    }

    // ── Snapshot / restore ──────────────────────────────────────

    /// Capture the oracle state for snapshot.
    pub fn snapshot(&self) -> OracleSnapshot {
        OracleSnapshot {
            assertions: BTreeMap::new(),
            structured_assertions: self.structured_assertions.clone(),
            accepted_catalog: self.accepted_catalog.clone(),
            catalog_status: self.catalog_status,
            identity_conflicts: self.identity_conflicts.clone(),
            total_runs: self.total_runs,
            events: self.events.clone(),
            current_run: self.current_run.clone(),
        }
    }

    /// Restore oracle state from a validated assertion-authority snapshot.
    pub fn restore(
        &mut self,
        snapshot: &OracleSnapshot,
    ) -> Result<(), crate::oracle_validation::OracleValidationError> {
        crate::oracle_validation::validate_restorable_oracle_snapshot(snapshot)?;
        self.apply_snapshot(snapshot);
        Ok(())
    }

    pub(crate) fn restore_orchestration(
        &mut self,
        snapshot: &OracleSnapshot,
    ) -> Result<(), crate::oracle_validation::OracleValidationError> {
        crate::oracle_snapshot_validation::validate_orchestration_oracle_snapshot(snapshot)?;
        self.apply_snapshot(snapshot);
        Ok(())
    }

    fn apply_snapshot(&mut self, snapshot: &OracleSnapshot) {
        self.structured_assertions = snapshot.structured_assertions.clone();
        self.accepted_catalog = snapshot.accepted_catalog.clone();
        self.catalog_status = snapshot.catalog_status;
        self.identity_conflicts = snapshot.identity_conflicts.clone();
        self.total_runs = snapshot.total_runs;
        self.events = snapshot.events.clone();
        self.current_run = snapshot.current_run.clone();
    }

    // ── Internal ────────────────────────────────────────────────
}

fn prepare_run_updates<K: Copy + Ord>(
    records: &BTreeMap<K, AssertionRecord>,
    hit: &BTreeSet<K>,
    satisfied: &BTreeSet<K>,
) -> Result<Vec<(K, u32, u32)>, CatalogConflict> {
    let keys = hit.union(satisfied).copied().collect::<Vec<_>>();
    let mut updates = Vec::with_capacity(keys.len());
    for key in keys {
        let record = records
            .get(&key)
            .ok_or(CatalogConflict::UnknownFingerprint)?;
        let runs_hit = if hit.contains(&key) {
            record
                .runs_hit
                .checked_add(1)
                .ok_or(CatalogConflict::CounterOverflow)?
        } else {
            record.runs_hit
        };
        let runs_satisfied = if satisfied.contains(&key) {
            record
                .runs_satisfied
                .checked_add(1)
                .ok_or(CatalogConflict::CounterOverflow)?
        } else {
            record.runs_satisfied
        };
        updates.push((key, runs_hit, runs_satisfied));
    }
    Ok(updates)
}

impl Default for PropertyOracle {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of a [`PropertyOracle`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleSnapshot {
    pub(crate) assertions: BTreeMap<u32, AssertionRecord>,
    #[serde(default = "empty_structured_assertions")]
    pub(crate) structured_assertions: BTreeMap<AssertionFingerprint, AssertionRecord>,
    #[serde(default = "no_accepted_catalog")]
    pub(crate) accepted_catalog: Option<AcceptedCatalog>,
    #[serde(default = "pending_catalog_status")]
    pub(crate) catalog_status: CatalogValidationStatus,
    #[serde(default = "empty_identity_conflicts")]
    pub(crate) identity_conflicts: Vec<String>,
    pub(crate) total_runs: u32,
    pub(crate) events: Vec<OracleEvent>,
    #[serde(default = "no_current_run")]
    pub(crate) current_run: Option<RunState>,
}

#[cfg(test)]
#[path = "oracle_tests.rs"]
mod tests;
