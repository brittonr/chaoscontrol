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

use chaoscontrol_protocol::assertion_catalog::{
    validate_accepted_catalog, AcceptedCatalog, AdmittedAssertion, BoundAssertionEvent,
    CatalogConflict, CatalogValidationStatus, MAX_ASSERTION_CATALOG_ENTRIES,
};
use chaoscontrol_protocol::assertion_identity::AssertionFingerprint;
pub use chaoscontrol_protocol::assertion_identity::AssertionKind;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<AdmittedAssertion>,
    /// Compatibility ID retained for existing CLI filters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_id: Option<u32>,
    /// Catalog tokens that admitted this exact descriptor.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub catalog_tokens: BTreeSet<AssertionFingerprint>,
    /// VM instances that contributed to an aggregated record.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub vm_instances: BTreeSet<u32>,
}

impl AssertionRecord {
    fn new(message: String, kind: AssertionKind) -> Self {
        Self {
            message,
            kind,
            hit_count: 0,
            true_count: 0,
            false_count: 0,
            runs_hit: 0,
            runs_satisfied: 0,
            first_failure_run: None,
            last_failure_details: None,
            guest: "uncategorized".to_string(),
            category: "uncategorized".to_string(),
            identity: None,
            compatibility_id: None,
            catalog_tokens: BTreeSet::new(),
            vm_instances: BTreeSet::new(),
        }
    }

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
#[derive(Debug, Clone)]
struct RunState {
    /// Run index (0-based).
    run_id: u32,
    /// Assertion IDs that were hit during this run.
    hit_ids: std::collections::BTreeSet<u32>,
    /// Assertion IDs that had condition=true during this run.
    satisfied_ids: BTreeSet<u32>,
    /// Strict assertion fingerprints that were hit during this run.
    strict_hit_ids: BTreeSet<AssertionFingerprint>,
    /// Strict fingerprints that were satisfied during this run.
    strict_satisfied_ids: BTreeSet<AssertionFingerprint>,
    /// Whether setup_complete was received.
    setup_complete: bool,
    /// Whether this run had an immediate failure (always=false, unreachable hit).
    immediate_failure: Option<(u32, String)>,
}

/// The property oracle.  Tracks assertions across multiple runs.
///
/// # Example
///
/// ```
/// use chaoscontrol_fault::oracle::{PropertyOracle, AssertionKind, Verdict};
///
/// let mut oracle = PropertyOracle::new();
///
/// // Run 0: leader is valid
/// oracle.begin_run();
/// oracle.record_always(1, true, "valid leader");
/// oracle.end_run();
///
/// // Run 1: leader is still valid
/// oracle.begin_run();
/// oracle.record_always(1, true, "valid leader");
/// oracle.end_run();
///
/// let report = oracle.report();
/// assert_eq!(report.assertions[&1].verdict(), Verdict::Passed);
/// ```
#[derive(Debug, Clone)]
pub struct PropertyOracle {
    /// Diagnostic-only legacy records keyed by compatibility ID.
    assertions: BTreeMap<u32, AssertionRecord>,
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
    #[serde(default)]
    pub structured_assertions: BTreeMap<AssertionFingerprint, AssertionRecord>,
    /// Catalog state at report creation.
    #[serde(default = "pending_catalog_status")]
    pub catalog_status: CatalogValidationStatus,
    /// Fatal assertion identity diagnostics.
    #[serde(default)]
    pub identity_conflicts: Vec<String>,
    /// True only when all records bind to one accepted strict catalog.
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

    pub fn record_for_compatibility_id(
        &self,
        compatibility_id: u32,
    ) -> Result<Option<&AssertionRecord>, CatalogConflict> {
        let mut records = self
            .all_records()
            .map(|(_, record)| record)
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
            assertions: BTreeMap::new(),
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
            hit_ids: BTreeSet::new(),
            satisfied_ids: BTreeSet::new(),
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
        let legacy_updates =
            match prepare_run_updates(&self.assertions, &run.hit_ids, &run.satisfied_ids) {
                Ok(updates) => updates,
                Err(conflict) => {
                    self.mark_identity_conflict(conflict);
                    return;
                }
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
        for (id, runs_hit, runs_satisfied) in legacy_updates {
            if let Some(record) = self.assertions.get_mut(&id) {
                record.runs_hit = runs_hit;
                record.runs_satisfied = runs_satisfied;
            }
        }
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
    pub fn immediate_failure(&self) -> Option<(u32, &str)> {
        self.current_run
            .as_ref()
            .and_then(|r| r.immediate_failure.as_ref())
            .map(|(id, msg)| (*id, msg.as_str()))
    }

    // ── Catalog registration ────────────────────────────────────

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

    pub fn mark_legacy_ambiguous(&mut self, diagnostic: &str) {
        if self.catalog_status != CatalogValidationStatus::FatalConflict {
            self.catalog_status = CatalogValidationStatus::LegacyAmbiguous;
        }
        if self.identity_conflicts.len() < crate::oracle_validation::MAX_IDENTITY_CONFLICTS {
            self.identity_conflicts.push(diagnostic.to_string());
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
            value.len()
                > chaoscontrol_protocol::assertion_identity::MAX_ASSERTION_EVENT_DETAILS_BYTES
        }) {
            return self.reject_bound_event(CatalogConflict::Descriptor(
                chaoscontrol_protocol::assertion_identity::IdentityError::FieldTooLong(
                    "event_details",
                ),
            ));
        }
        let run_id = self.current_run_id();
        let Some(record) = self.structured_assertions.get(&event.fingerprint) else {
            return self.reject_bound_event(CatalogConflict::UnknownFingerprint);
        };
        if record.identity.as_ref() != Some(&admitted) {
            return self.reject_bound_event(CatalogConflict::FingerprintCollision);
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
        if let Some(run) = &mut self.current_run {
            run.strict_hit_ids.insert(event.fingerprint);
            if satisfied {
                run.strict_satisfied_ids.insert(event.fingerprint);
            }
            if immediate_failure {
                let id = admitted.descriptor.compatibility_id.unwrap_or_default();
                run.immediate_failure = Some((id, admitted.descriptor.message.clone()));
            }
        }
        Ok(satisfied)
    }

    fn reject_bound_event<T>(&mut self, conflict: CatalogConflict) -> Result<T, CatalogConflict> {
        self.mark_identity_conflict(conflict.clone());
        Err(conflict)
    }

    /// Register an assertion site from the compile-time catalog.
    ///
    /// Creates a record with `hit_count = 0` so the oracle knows this
    /// assertion exists even if it's never evaluated at runtime.  This
    /// turns "unknown" into "Unexercised" in the final report.
    ///
    /// If the assertion was already recorded (via a runtime hit before
    /// catalog registration), the existing record is kept unchanged.
    pub fn register_catalog_entry(&mut self, id: u32, kind: AssertionKind, message: &str) {
        self.register_catalog_entry_with_metadata(
            id,
            kind,
            message,
            "uncategorized",
            "uncategorized",
        );
    }

    /// Register an assertion site with optional density metadata.
    pub fn register_catalog_entry_with_metadata(
        &mut self,
        id: u32,
        kind: AssertionKind,
        message: &str,
        guest: &str,
        category: &str,
    ) {
        let normalized_guest = if guest.is_empty() {
            "uncategorized"
        } else {
            guest
        };
        let normalized_category = if category.is_empty() {
            "uncategorized"
        } else {
            category
        };
        let Ok(record) = self.legacy_record_mut(id, kind, message) else {
            return;
        };
        if record.guest != "uncategorized" && record.guest != normalized_guest {
            self.mark_identity_conflict(CatalogConflict::GuestConflict);
            return;
        }
        if record.category != "uncategorized" && record.category != normalized_category {
            self.mark_identity_conflict(CatalogConflict::CategoryConflict);
            return;
        }
        record.guest = normalized_guest.to_string();
        record.category = normalized_category.to_string();
    }

    /// Number of registered assertion sites (from catalog + runtime).
    pub fn catalog_size(&self) -> usize {
        self.assertions.len()
    }

    // ── Recording methods ───────────────────────────────────────

    /// Record an `assert_always` evaluation.
    ///
    /// Returns `true` if the assertion passed (condition was true).
    pub fn record_always(&mut self, id: u32, condition: bool, message: &str) -> bool {
        self.record_always_with_details(id, condition, message, None)
    }

    /// Record an `assert_always` evaluation with optional failure details.
    ///
    /// Details are stored only when `condition` is false (failure).
    /// Passing assertions never overwrite stored failure details.
    pub fn record_always_with_details(
        &mut self,
        id: u32,
        condition: bool,
        message: &str,
        details: Option<&[u8]>,
    ) -> bool {
        let run_id = self.current_run_id();
        let Ok(record) = self.legacy_record_mut(id, AssertionKind::Always, message) else {
            return false;
        };

        record.hit_count = record.hit_count.saturating_add(1);
        if condition {
            record.true_count += 1;
        } else {
            record.false_count += 1;
            if record.first_failure_run.is_none() {
                record.first_failure_run = Some(run_id);
            }
            if let Some(d) = details {
                record.last_failure_details = Some(d.to_vec());
            }
        }

        if let Some(run) = &mut self.current_run {
            run.hit_ids.insert(id);
            if condition {
                run.satisfied_ids.insert(id);
            } else {
                run.immediate_failure = Some((id, message.to_string()));
            }
        }

        condition
    }

    /// Record an `assert_sometimes` evaluation.
    pub fn record_sometimes(&mut self, id: u32, condition: bool, message: &str) {
        self.record_sometimes_with_details(id, condition, message, None)
    }

    /// Record an `assert_sometimes` evaluation with optional failure details.
    ///
    /// Details are stored only when `condition` is false.
    pub fn record_sometimes_with_details(
        &mut self,
        id: u32,
        condition: bool,
        message: &str,
        details: Option<&[u8]>,
    ) {
        let Ok(record) = self.legacy_record_mut(id, AssertionKind::Sometimes, message) else {
            return;
        };

        record.hit_count = record.hit_count.saturating_add(1);
        if condition {
            record.true_count += 1;
        } else {
            record.false_count += 1;
            if let Some(d) = details {
                record.last_failure_details = Some(d.to_vec());
            }
        }

        if let Some(run) = &mut self.current_run {
            run.hit_ids.insert(id);
            if condition {
                run.satisfied_ids.insert(id);
            }
        }
    }

    /// Record an `assert_reachable` evaluation.
    pub fn record_reachable(&mut self, id: u32, message: &str) {
        let Ok(record) = self.legacy_record_mut(id, AssertionKind::Reachable, message) else {
            return;
        };

        record.hit_count = record.hit_count.saturating_add(1);
        record.true_count += 1;

        if let Some(run) = &mut self.current_run {
            run.hit_ids.insert(id);
            run.satisfied_ids.insert(id);
        }
    }

    /// Record an `assert_unreachable` evaluation.
    ///
    /// Returns `false` always (reaching an unreachable point is a failure).
    pub fn record_unreachable(&mut self, id: u32, message: &str) -> bool {
        self.record_unreachable_with_details(id, message, None)
    }

    /// Record an `assert_unreachable` evaluation with optional details.
    pub fn record_unreachable_with_details(
        &mut self,
        id: u32,
        message: &str,
        details: Option<&[u8]>,
    ) -> bool {
        let run_id = self.current_run_id();
        let Ok(record) = self.legacy_record_mut(id, AssertionKind::Unreachable, message) else {
            return false;
        };

        record.hit_count = record.hit_count.saturating_add(1);
        record.false_count += 1;
        if record.first_failure_run.is_none() {
            record.first_failure_run = Some(run_id);
        }
        if let Some(d) = details {
            record.last_failure_details = Some(d.to_vec());
        }

        if let Some(run) = &mut self.current_run {
            run.hit_ids.insert(id);
            run.immediate_failure = Some((id, message.to_string()));
        }

        false
    }

    /// Record a `setup_complete` lifecycle event.
    pub fn record_setup_complete(&mut self) {
        if let Some(run) = &mut self.current_run {
            run.setup_complete = true;
        }
    }

    /// Whether the current run's setup phase is complete.
    pub fn is_setup_complete(&self) -> bool {
        self.current_run.as_ref().is_some_and(|r| r.setup_complete)
    }

    /// Record a lifecycle event.
    pub fn record_event(&mut self, name: &str, details: serde_json::Value) {
        let run_id = self.current_run_id();
        self.events.push(OracleEvent {
            run_id,
            name: name.to_string(),
            details,
        });
    }

    // ── Reporting ───────────────────────────────────────────────

    /// Produce a summary report of all assertions across all runs.
    pub fn report(&self) -> OracleReport {
        let mut passed = 0;
        let mut failed = 0;
        let mut unexercised = 0;

        for record in self
            .assertions
            .values()
            .chain(self.structured_assertions.values())
        {
            match record.verdict() {
                Verdict::Passed => passed += 1,
                Verdict::Failed => failed += 1,
                Verdict::Unexercised => unexercised += 1,
            }
        }
        let mut report = OracleReport {
            assertions: self.assertions.clone(),
            structured_assertions: self.structured_assertions.clone(),
            catalog_status: self.catalog_status,
            identity_conflicts: self.identity_conflicts.clone(),
            collision_safe_evidence: false,
            total_runs: self.total_runs,
            passed,
            failed,
            unexercised,
            catalog_size: self
                .assertions
                .len()
                .saturating_add(self.structured_assertions.len()),
            events: self.events.clone(),
        };
        report.collision_safe_evidence =
            crate::oracle_validation::validate_strict_oracle_report(&report).is_ok();
        report
    }

    /// Get a reference to all assertion records.
    pub fn assertions(&self) -> &BTreeMap<u32, AssertionRecord> {
        &self.assertions
    }

    pub fn structured_assertions(&self) -> &BTreeMap<AssertionFingerprint, AssertionRecord> {
        &self.structured_assertions
    }

    pub fn catalog_status(&self) -> CatalogValidationStatus {
        self.catalog_status
    }

    /// Total number of completed runs.
    pub fn total_runs(&self) -> u32 {
        self.total_runs
    }

    // ── Snapshot / restore ──────────────────────────────────────

    /// Capture the oracle state for snapshot.
    pub fn snapshot(&self) -> OracleSnapshot {
        OracleSnapshot {
            assertions: self.assertions.clone(),
            structured_assertions: self.structured_assertions.clone(),
            accepted_catalog: self.accepted_catalog.clone(),
            catalog_status: self.catalog_status,
            identity_conflicts: self.identity_conflicts.clone(),
            total_runs: self.total_runs,
            events: self.events.clone(),
        }
    }

    /// Restore oracle state from a validated snapshot.
    pub fn restore(
        &mut self,
        snapshot: &OracleSnapshot,
    ) -> Result<(), crate::oracle_validation::OracleValidationError> {
        crate::oracle_validation::validate_oracle_snapshot(snapshot)?;
        self.assertions = snapshot.assertions.clone();
        self.structured_assertions = snapshot.structured_assertions.clone();
        self.accepted_catalog = snapshot.accepted_catalog.clone();
        self.catalog_status = snapshot.catalog_status;
        self.identity_conflicts = snapshot.identity_conflicts.clone();
        self.total_runs = snapshot.total_runs;
        self.events = snapshot.events.clone();
        self.current_run = None;
        Ok(())
    }

    // ── Internal ────────────────────────────────────────────────

    fn legacy_record_mut(
        &mut self,
        id: u32,
        kind: AssertionKind,
        message: &str,
    ) -> Result<&mut AssertionRecord, CatalogConflict> {
        self.mark_legacy_ambiguous("legacy u32 assertion input is diagnostic only");
        if let Some(existing) = self.assertions.get(&id) {
            if existing.kind != kind {
                self.mark_identity_conflict(CatalogConflict::KindConflict);
                return Err(CatalogConflict::KindConflict);
            }
            if existing.message != message {
                self.mark_identity_conflict(CatalogConflict::MessageConflict);
                return Err(CatalogConflict::MessageConflict);
            }
        } else {
            if self.assertions.len() >= MAX_ASSERTION_CATALOG_ENTRIES {
                self.mark_identity_conflict(CatalogConflict::CardinalityOverflow);
                return Err(CatalogConflict::CardinalityOverflow);
            }
            let mut record = AssertionRecord::new(message.to_string(), kind);
            record.compatibility_id = Some(id);
            self.assertions.insert(id, record);
        }
        self.assertions
            .get_mut(&id)
            .ok_or(CatalogConflict::UnknownFingerprint)
    }

    fn current_run_id(&self) -> u32 {
        self.current_run
            .as_ref()
            .map_or(self.total_runs, |r| r.run_id)
    }
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
    #[serde(default)]
    pub(crate) structured_assertions: BTreeMap<AssertionFingerprint, AssertionRecord>,
    #[serde(default)]
    pub(crate) accepted_catalog: Option<AcceptedCatalog>,
    #[serde(default = "pending_catalog_status")]
    pub(crate) catalog_status: CatalogValidationStatus,
    #[serde(default)]
    pub(crate) identity_conflicts: Vec<String>,
    pub(crate) total_runs: u32,
    pub(crate) events: Vec<OracleEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn oracle_event_details_round_trip_through_bytes() {
        let event = OracleEvent {
            run_id: 7,
            name: "setup_complete".to_string(),
            details: json!({"workload": "rust-workload", "attempt": 2}),
        };

        let encoded = serde_json::to_vec(&event).expect("serialize oracle event");
        let decoded: OracleEvent =
            serde_json::from_slice(&encoded).expect("deserialize oracle event");

        assert_eq!(decoded.run_id, event.run_id);
        assert_eq!(decoded.name, event.name);
        assert_eq!(decoded.details, event.details);
    }

    #[test]
    fn oracle_event_details_stay_structured_in_json() {
        let event = OracleEvent {
            run_id: 7,
            name: "setup_complete".to_string(),
            details: json!({"workload": "rust-workload"}),
        };

        let encoded = serde_json::to_value(&event).expect("serialize oracle event");
        assert_eq!(encoded["details"]["workload"], "rust-workload");
        let decoded: OracleEvent =
            serde_json::from_value(encoded).expect("deserialize oracle event");
        assert_eq!(decoded.details, event.details);
    }

    #[test]
    fn always_all_true_passes() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();
        oracle.record_always(1, true, "test");
        oracle.record_always(1, true, "test");
        oracle.end_run();

        let report = oracle.report();
        assert_eq!(report.assertions[&1].verdict(), Verdict::Passed);
        assert_eq!(report.assertions[&1].hit_count, 2);
        assert_eq!(report.assertions[&1].true_count, 2);
        assert_eq!(report.assertions[&1].false_count, 0);
    }

    #[test]
    fn always_one_false_fails() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();
        oracle.record_always(1, true, "test");
        oracle.record_always(1, false, "test");
        oracle.end_run();

        let report = oracle.report();
        assert_eq!(report.assertions[&1].verdict(), Verdict::Failed);
        assert_eq!(report.assertions[&1].first_failure_run, Some(0));
    }

    #[test]
    fn always_immediate_failure() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();
        assert!(!oracle.has_immediate_failure());
        let passed = oracle.record_always(1, false, "oops");
        assert!(!passed);
        assert!(oracle.has_immediate_failure());
        assert_eq!(oracle.immediate_failure(), Some((1, "oops")));
    }

    #[test]
    fn sometimes_all_false_fails() {
        let mut oracle = PropertyOracle::new();
        for _ in 0..3 {
            oracle.begin_run();
            oracle.record_sometimes(1, false, "test");
            oracle.end_run();
        }

        let report = oracle.report();
        assert_eq!(report.assertions[&1].verdict(), Verdict::Failed);
    }

    #[test]
    fn sometimes_one_true_passes() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();
        oracle.record_sometimes(1, false, "test");
        oracle.end_run();

        oracle.begin_run();
        oracle.record_sometimes(1, true, "test");
        oracle.end_run();

        let report = oracle.report();
        assert_eq!(report.assertions[&1].verdict(), Verdict::Passed);
        assert_eq!(report.assertions[&1].true_count, 1);
    }

    #[test]
    fn reachable_hit_passes() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();
        oracle.record_reachable(1, "error path");
        oracle.end_run();

        assert_eq!(oracle.report().assertions[&1].verdict(), Verdict::Passed);
    }

    #[test]
    fn reachable_never_hit_unexercised() {
        let oracle = PropertyOracle::new();
        // No assertions recorded at all
        assert_eq!(oracle.report().assertions.len(), 0);
    }

    #[test]
    fn unreachable_never_hit_passes() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();
        // Don't record unreachable — it was never reached
        oracle.end_run();
        // Unreachable assertions that are never registered pass vacuously
        assert_eq!(oracle.report().assertions.len(), 0);
    }

    #[test]
    fn unreachable_hit_fails() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();
        let passed = oracle.record_unreachable(1, "impossible state");
        assert!(!passed);
        oracle.end_run();

        assert_eq!(oracle.report().assertions[&1].verdict(), Verdict::Failed);
    }

    #[test]
    fn never_evaluated_is_unexercised() {
        let mut oracle = PropertyOracle::new();
        // Register assertion manually but never evaluate it
        oracle.begin_run();
        oracle.end_run();

        // Insert a phantom record to test the Unexercised path
        oracle.assertions.insert(
            99,
            AssertionRecord::new("phantom".to_string(), AssertionKind::Always),
        );
        assert_eq!(
            oracle.report().assertions[&99].verdict(),
            Verdict::Unexercised
        );
    }

    #[test]
    fn multiple_runs_tracking() {
        let mut oracle = PropertyOracle::new();

        for i in 0..5 {
            oracle.begin_run();
            oracle.record_always(1, true, "stable");
            oracle.record_sometimes(2, i == 3, "rare event");
            oracle.end_run();
        }

        let report = oracle.report();
        assert_eq!(report.total_runs, 5);
        assert_eq!(report.assertions[&1].verdict(), Verdict::Passed);
        assert_eq!(report.assertions[&1].runs_hit, 5);
        assert_eq!(report.assertions[&2].verdict(), Verdict::Passed); // true in run 3
        assert_eq!(report.assertions[&2].true_count, 1);
        assert_eq!(report.assertions[&2].runs_satisfied, 1);
    }

    #[test]
    fn report_summary_counts() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();
        oracle.record_always(1, true, "pass");
        oracle.record_always(2, false, "fail");
        oracle.end_run();

        oracle.assertions.insert(
            3,
            AssertionRecord::new("ghost".to_string(), AssertionKind::Sometimes),
        );

        let report = oracle.report();
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.unexercised, 1);
    }

    #[test]
    fn setup_complete_tracking() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();
        assert!(!oracle.is_setup_complete());
        oracle.record_setup_complete();
        assert!(oracle.is_setup_complete());
        oracle.end_run();
    }

    #[test]
    fn lifecycle_events() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();
        oracle.record_event("leader_elected", serde_json::json!({"node": "2"}));
        oracle.end_run();

        let report = oracle.report();
        assert_eq!(report.events.len(), 1);
        assert_eq!(report.events[0].name, "leader_elected");
        assert_eq!(report.events[0].run_id, 0);
    }

    #[test]
    fn catalog_entry_creates_unexercised() {
        let mut oracle = PropertyOracle::new();
        oracle.register_catalog_entry(10, AssertionKind::Always, "never hit");
        oracle.register_catalog_entry(11, AssertionKind::Sometimes, "never hit either");
        oracle.register_catalog_entry(12, AssertionKind::Reachable, "never reached");

        let report = oracle.report();
        assert_eq!(report.catalog_size, 3);
        assert_eq!(report.unexercised, 3);
        assert_eq!(report.assertions[&10].verdict(), Verdict::Unexercised);
        assert_eq!(report.assertions[&11].verdict(), Verdict::Unexercised);
        assert_eq!(report.assertions[&12].verdict(), Verdict::Unexercised);
    }

    #[test]
    fn catalog_entry_does_not_overwrite_runtime() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();
        oracle.record_always(10, true, "passes");
        oracle.end_run();

        // Catalog registration after runtime hit — should NOT reset the record.
        oracle.register_catalog_entry(10, AssertionKind::Always, "passes");

        let report = oracle.report();
        assert_eq!(report.assertions[&10].verdict(), Verdict::Passed);
        assert_eq!(report.assertions[&10].hit_count, 1);
    }

    #[test]
    fn catalog_then_runtime_hit() {
        let mut oracle = PropertyOracle::new();
        oracle.register_catalog_entry(10, AssertionKind::Sometimes, "event");

        // Initially unexercised
        assert_eq!(
            oracle.report().assertions[&10].verdict(),
            Verdict::Unexercised
        );

        // Now hit it
        oracle.begin_run();
        oracle.record_sometimes(10, true, "event");
        oracle.end_run();

        let report = oracle.report();
        assert_eq!(report.assertions[&10].verdict(), Verdict::Passed);
        assert_eq!(report.assertions[&10].hit_count, 1);
    }

    #[test]
    fn catalog_size_in_report() {
        let mut oracle = PropertyOracle::new();
        oracle.register_catalog_entry(1, AssertionKind::Always, "a");
        oracle.register_catalog_entry(2, AssertionKind::Sometimes, "b");

        oracle.begin_run();
        oracle.record_always(1, true, "a");
        oracle.record_always(3, true, "c"); // runtime-only, not in catalog
        oracle.end_run();

        let report = oracle.report();
        assert_eq!(report.catalog_size, 3); // 2 from catalog + 1 runtime-only
        assert_eq!(report.passed, 2); // id=1 (always true), id=3 (always true)
        assert_eq!(report.unexercised, 1); // id=2 (never hit)
    }

    #[test]
    fn snapshot_restore() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();
        oracle.record_always(1, true, "test");
        oracle.end_run();

        let snap = oracle.snapshot();

        // More runs
        oracle.begin_run();
        oracle.record_always(1, false, "test");
        oracle.end_run();

        assert_eq!(oracle.report().assertions[&1].verdict(), Verdict::Failed);

        // Restore
        oracle.restore(&snap).expect("restore oracle");
        assert_eq!(oracle.report().assertions[&1].verdict(), Verdict::Passed);
        assert_eq!(oracle.total_runs(), 1);
    }

    #[test]
    fn failure_stores_details_pass_does_not_overwrite() {
        let mut oracle = PropertyOracle::new();
        oracle.begin_run();

        // Pass — no details stored
        oracle.record_always_with_details(1, true, "test", Some(b"{\"pass\":true}"));
        assert!(oracle.report().assertions[&1]
            .last_failure_details
            .is_none());

        // Fail — details stored
        oracle.record_always_with_details(1, false, "test", Some(b"{\"x\":1}"));
        assert_eq!(
            oracle.report().assertions[&1]
                .last_failure_details
                .as_deref(),
            Some(b"{\"x\":1}".as_slice()),
        );

        // Another pass — failure details NOT overwritten
        oracle.record_always_with_details(1, true, "test", Some(b"{\"pass\":true}"));
        assert_eq!(
            oracle.report().assertions[&1]
                .last_failure_details
                .as_deref(),
            Some(b"{\"x\":1}".as_slice()),
        );

        // Another fail — details updated
        oracle.record_always_with_details(1, false, "test", Some(b"{\"x\":2}"));
        assert_eq!(
            oracle.report().assertions[&1]
                .last_failure_details
                .as_deref(),
            Some(b"{\"x\":2}".as_slice()),
        );
        oracle.end_run();
    }

    #[test]
    fn strict_run_counter_overflow_has_no_partial_update() {
        let mut record = AssertionRecord::new("strict".to_string(), AssertionKind::Always);
        record.runs_hit = u32::MAX;
        let records = BTreeMap::from([(AssertionFingerprint::ZERO, record)]);
        let hit = BTreeSet::from([AssertionFingerprint::ZERO]);
        let satisfied = BTreeSet::new();

        assert_eq!(
            prepare_run_updates(&records, &hit, &satisfied),
            Err(CatalogConflict::CounterOverflow)
        );
        assert_eq!(records[&AssertionFingerprint::ZERO].runs_hit, u32::MAX);
    }
}
