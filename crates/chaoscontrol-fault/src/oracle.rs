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

use std::collections::BTreeMap;

/// Records for a single assertion site across all runs.
#[derive(Debug, Clone)]
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
}

/// The kind of assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionKind {
    Always,
    Sometimes,
    Reachable,
    Unreachable,
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

/// Final verdict for an assertion after all runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    satisfied_ids: std::collections::BTreeSet<u32>,
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
    /// Per-assertion records keyed by assertion ID.
    assertions: BTreeMap<u32, AssertionRecord>,
    /// Current run state (None if between runs).
    current_run: Option<RunState>,
    /// Total number of completed runs.
    total_runs: u32,
    /// Lifecycle events received.
    events: Vec<OracleEvent>,
}

/// An event recorded by the oracle.
#[derive(Debug, Clone)]
pub struct OracleEvent {
    /// Run in which this event occurred.
    pub run_id: u32,
    /// Event name.
    pub name: String,
    /// Key-value details.
    pub details: Vec<(String, String)>,
}

/// Report produced by the oracle after all runs.
#[derive(Debug, Clone)]
pub struct OracleReport {
    /// Per-assertion records.
    pub assertions: BTreeMap<u32, AssertionRecord>,
    /// Total number of runs.
    pub total_runs: u32,
    /// Number of assertions that passed.
    pub passed: usize,
    /// Number of assertions that failed.
    pub failed: usize,
    /// Number of assertions never exercised.
    pub unexercised: usize,
    /// All lifecycle events.
    pub events: Vec<OracleEvent>,
}

impl PropertyOracle {
    /// Create a new oracle with no recorded state.
    pub fn new() -> Self {
        Self {
            assertions: BTreeMap::new(),
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
            hit_ids: std::collections::BTreeSet::new(),
            satisfied_ids: std::collections::BTreeSet::new(),
            setup_complete: false,
            immediate_failure: None,
        });
    }

    /// End the current run and finalize per-run counters.
    pub fn end_run(&mut self) {
        if let Some(run) = self.current_run.take() {
            // Update runs_hit and runs_satisfied for each assertion
            for &id in &run.hit_ids {
                if let Some(record) = self.assertions.get_mut(&id) {
                    record.runs_hit += 1;
                }
            }
            for &id in &run.satisfied_ids {
                if let Some(record) = self.assertions.get_mut(&id) {
                    record.runs_satisfied += 1;
                }
            }
            self.total_runs += 1;
        }
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

    // ── Recording methods ───────────────────────────────────────

    /// Record an `assert_always` evaluation.
    ///
    /// Returns `true` if the assertion passed (condition was true).
    pub fn record_always(&mut self, id: u32, condition: bool, message: &str) -> bool {
        let run_id = self.current_run_id();
        let record = self
            .assertions
            .entry(id)
            .or_insert_with(|| AssertionRecord::new(message.to_string(), AssertionKind::Always));

        record.hit_count += 1;
        if condition {
            record.true_count += 1;
        } else {
            record.false_count += 1;
            if record.first_failure_run.is_none() {
                record.first_failure_run = Some(run_id);
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
        let record = self
            .assertions
            .entry(id)
            .or_insert_with(|| AssertionRecord::new(message.to_string(), AssertionKind::Sometimes));

        record.hit_count += 1;
        if condition {
            record.true_count += 1;
        } else {
            record.false_count += 1;
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
        let record = self
            .assertions
            .entry(id)
            .or_insert_with(|| AssertionRecord::new(message.to_string(), AssertionKind::Reachable));

        record.hit_count += 1;
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
        let run_id = self.current_run_id();
        let record = self.assertions.entry(id).or_insert_with(|| {
            AssertionRecord::new(message.to_string(), AssertionKind::Unreachable)
        });

        record.hit_count += 1;
        record.false_count += 1;
        if record.first_failure_run.is_none() {
            record.first_failure_run = Some(run_id);
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
        self.current_run
            .as_ref()
            .is_some_and(|r| r.setup_complete)
    }

    /// Record a lifecycle event.
    pub fn record_event(&mut self, name: &str, details: Vec<(String, String)>) {
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

        for record in self.assertions.values() {
            match record.verdict() {
                Verdict::Passed => passed += 1,
                Verdict::Failed => failed += 1,
                Verdict::Unexercised => unexercised += 1,
            }
        }

        OracleReport {
            assertions: self.assertions.clone(),
            total_runs: self.total_runs,
            passed,
            failed,
            unexercised,
            events: self.events.clone(),
        }
    }

    /// Get a reference to all assertion records.
    pub fn assertions(&self) -> &BTreeMap<u32, AssertionRecord> {
        &self.assertions
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
            total_runs: self.total_runs,
            events: self.events.clone(),
        }
    }

    /// Restore oracle state from a snapshot.
    pub fn restore(&mut self, snapshot: &OracleSnapshot) {
        self.assertions = snapshot.assertions.clone();
        self.total_runs = snapshot.total_runs;
        self.events = snapshot.events.clone();
        self.current_run = None;
    }

    // ── Internal ────────────────────────────────────────────────

    fn current_run_id(&self) -> u32 {
        self.current_run.as_ref().map_or(self.total_runs, |r| r.run_id)
    }
}

impl Default for PropertyOracle {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of a [`PropertyOracle`].
#[derive(Debug, Clone)]
pub struct OracleSnapshot {
    assertions: BTreeMap<u32, AssertionRecord>,
    total_runs: u32,
    events: Vec<OracleEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(oracle.report().assertions[&99].verdict(), Verdict::Unexercised);
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
        oracle.record_event("leader_elected", vec![("node".into(), "2".into())]);
        oracle.end_run();

        let report = oracle.report();
        assert_eq!(report.events.len(), 1);
        assert_eq!(report.events[0].name, "leader_elected");
        assert_eq!(report.events[0].run_id, 0);
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
        oracle.restore(&snap);
        assert_eq!(oracle.report().assertions[&1].verdict(), Verdict::Passed);
        assert_eq!(oracle.total_runs(), 1);
    }
}
