//! Corpus — stores interesting inputs (fault schedules) that produced new coverage or bugs.

use crate::coverage::CoverageBitmap;
use crate::snapshot_store::ReplayParentSnapshotRef;
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_protocol::admission::AssertionEvidenceIdentity;
use chaoscontrol_vmm::controller::SimulationSnapshot;
use chaoscontrol_vmm::scheduler::ScheduleVariant;

/// A bug report produced during exploration.
#[derive(Debug, Clone)]
pub struct BugReport {
    /// Unique bug ID.
    pub bug_id: u64,
    /// Non-authoritative compact alias for display and filtering.
    pub assertion_id: u64,
    /// Exact admitted assertion identity that failed.
    pub assertion_identity: AssertionEvidenceIdentity,
    /// The assertion location/message.
    pub assertion_location: String,
    /// The fault schedule that triggered it.
    pub schedule: FaultSchedule,
    /// Parent snapshot used to start the failing branch (for reproduction).
    pub snapshot: Option<SimulationSnapshot>,
    /// Tick when the bug was found.
    pub tick: u64,
    /// Depth of the frontier parent snapshot used to start the failing branch.
    ///
    /// Depth 0 means the schedule starts from the normal post-bootstrap
    /// snapshot. Depth > 0 means a standalone replay needs the saved branch
    /// snapshot context, not just the fault schedule.
    pub replay_parent_depth: u32,
    /// Durable replay parent snapshot artifact reference, when persisted.
    pub replay_parent_snapshot_ref: Option<ReplayParentSnapshotRef>,
    /// Dedup key: hash of (assertion_id, sorted fault type names).
    pub dedup_key: u64,
    /// Schedule variant used for this branch (for reproduction).
    pub schedule_variant: Option<ScheduleVariant>,
    /// Helical scenario config that generated the schedule (if any).
    pub scenario_config: Option<chaoscontrol_fault::scenario::ScenarioConfig>,
    /// Materialized phase summary (if a scenario was used).
    pub scenario_summary: Option<chaoscontrol_fault::scenario::PhaseSummary>,
}

/// An entry in the corpus — a schedule that found new coverage or bugs.
#[derive(Clone)]
pub struct CorpusEntry {
    /// Unique entry ID.
    pub id: u64,
    /// The fault schedule.
    pub schedule: FaultSchedule,
    /// Coverage bitmap produced by this schedule.
    pub coverage: CoverageBitmap,
    /// Number of new edges this schedule found.
    pub new_edges: usize,
    /// Bugs found by this schedule.
    pub bugs_found: Vec<BugReport>,
    /// Depth in the exploration tree.
    pub depth: u32,
}

/// Stores interesting inputs (fault schedules) that produced new coverage or bugs.
pub struct Corpus {
    /// Interesting schedules, indexed by ID.
    entries: Vec<CorpusEntry>,
    /// Global coverage union.
    global_coverage: CoverageBitmap,
    /// Next entry ID.
    next_id: u64,
    /// Next bug ID.
    next_bug_id: u64,
}

impl Corpus {
    /// Create a new empty corpus.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            global_coverage: CoverageBitmap::new(),
            next_id: 0,
            next_bug_id: 0,
        }
    }

    /// Add an entry to the corpus.
    ///
    /// Automatically assigns an ID and updates global coverage.
    pub fn add(&mut self, mut entry: CorpusEntry) {
        entry.id = self.next_id;
        self.next_id += 1;

        // Assign bug IDs
        for bug in &mut entry.bugs_found {
            bug.bug_id = self.next_bug_id;
            self.next_bug_id += 1;
        }

        self.global_coverage.merge(&entry.coverage);
        self.entries.push(entry);
    }

    /// Get all bugs found across all corpus entries.
    pub fn bugs(&self) -> Vec<BugReport> {
        self.entries
            .iter()
            .flat_map(|e| e.bugs_found.iter())
            .cloned()
            .collect()
    }

    /// Get corpus statistics.
    pub fn stats(&self) -> CorpusStats {
        let total_bugs = self.bugs().len();
        let total_edges = self.global_coverage.count_bits();
        let avg_depth = if self.entries.is_empty() {
            0.0
        } else {
            self.entries.iter().map(|e| e.depth as f64).sum::<f64>() / self.entries.len() as f64
        };

        CorpusStats {
            total_entries: self.entries.len(),
            total_bugs,
            total_edges,
            avg_depth,
        }
    }

    /// Get all entries.
    pub fn entries(&self) -> &[CorpusEntry] {
        &self.entries
    }

    /// Get a reference to the global coverage.
    pub fn global_coverage(&self) -> &CoverageBitmap {
        &self.global_coverage
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the corpus is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for Corpus {
    fn default() -> Self {
        Self::new()
    }
}

/// Corpus statistics.
#[derive(Debug, Clone)]
pub struct CorpusStats {
    /// Total number of entries.
    pub total_entries: usize,
    /// Total bugs found.
    pub total_bugs: usize,
    /// Total unique edges.
    pub total_edges: usize,
    /// Average exploration depth.
    pub avg_depth: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(new_edges: usize, depth: u32, bugs: usize) -> CorpusEntry {
        let mut coverage = CoverageBitmap::new();
        for i in 0..new_edges {
            coverage.record_hit(i);
        }

        let mut bugs_found = Vec::new();
        for i in 0..bugs {
            bugs_found.push(BugReport {
                bug_id: 0,
                assertion_id: i as u64,
                assertion_identity: crate::test_support::assertion_identity(i as u64),
                assertion_location: format!("bug_{}", i),
                schedule: FaultSchedule::new(),
                snapshot: None,
                tick: 1000,
                replay_parent_depth: 0,
                replay_parent_snapshot_ref: None,
                dedup_key: 0,
                schedule_variant: None,
                scenario_config: None,
                scenario_summary: None,
            });
        }

        CorpusEntry {
            id: 0,
            schedule: FaultSchedule::new(),
            coverage,
            new_edges,
            bugs_found,
            depth,
        }
    }

    #[test]
    fn test_corpus_new() {
        let corpus = Corpus::new();
        assert_eq!(corpus.len(), 0);
        assert!(corpus.is_empty());
    }

    #[test]
    fn test_corpus_add() {
        let mut corpus = Corpus::new();
        let entry = make_entry(5, 1, 0);

        corpus.add(entry);
        assert_eq!(corpus.len(), 1);
        assert_eq!(corpus.entries[0].id, 0);
    }

    #[test]
    fn test_corpus_auto_assign_ids() {
        let mut corpus = Corpus::new();
        corpus.add(make_entry(5, 1, 0));
        corpus.add(make_entry(3, 2, 0));

        assert_eq!(corpus.entries[0].id, 0);
        assert_eq!(corpus.entries[1].id, 1);
    }

    #[test]
    fn test_corpus_global_coverage() {
        let mut corpus = Corpus::new();

        let mut entry1 = make_entry(0, 1, 0);
        entry1.coverage.record_hit(1);
        entry1.coverage.record_hit(2);

        corpus.add(entry1);
        assert_eq!(corpus.global_coverage.count_bits(), 2);

        let mut entry2 = make_entry(0, 1, 0);
        entry2.coverage.record_hit(2);
        entry2.coverage.record_hit(3);

        corpus.add(entry2);
        assert_eq!(corpus.global_coverage.count_bits(), 3); // edges 1, 2, 3
    }

    #[test]
    fn test_corpus_bugs_collection() {
        let mut corpus = Corpus::new();

        corpus.add(make_entry(5, 1, 2)); // 2 bugs
        corpus.add(make_entry(3, 2, 1)); // 1 bug

        let bugs = corpus.bugs();
        assert_eq!(bugs.len(), 3);
    }

    #[test]
    fn test_corpus_bug_id_assignment() {
        let mut corpus = Corpus::new();

        corpus.add(make_entry(5, 1, 2)); // 2 bugs

        let bugs = corpus.bugs();
        assert_eq!(bugs[0].bug_id, 0);
        assert_eq!(bugs[1].bug_id, 1);
    }

    #[test]
    fn test_corpus_stats() {
        let mut corpus = Corpus::new();

        corpus.add(make_entry(10, 1, 1));
        corpus.add(make_entry(5, 3, 2));

        let stats = corpus.stats();
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.total_bugs, 3);
        assert_eq!(stats.total_edges, 10); // First entry covers 0-9, second covers 0-4 (overlap)
        assert_eq!(stats.avg_depth, 2.0); // (1 + 3) / 2
    }

    #[test]
    fn test_corpus_stats_empty() {
        let corpus = Corpus::new();
        let stats = corpus.stats();

        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.total_bugs, 0);
        assert_eq!(stats.total_edges, 0);
        assert_eq!(stats.avg_depth, 0.0);
    }

    #[test]
    fn test_bug_report_structure() {
        let bug = BugReport {
            bug_id: 42,
            assertion_id: 100,
            assertion_identity: crate::test_support::assertion_identity(100),
            assertion_location: "test.rs:123".to_string(),
            schedule: FaultSchedule::new(),
            snapshot: None,
            tick: 5000,
            replay_parent_depth: 0,
            replay_parent_snapshot_ref: None,
            dedup_key: 0,
            schedule_variant: None,
            scenario_config: None,
            scenario_summary: None,
        };

        assert_eq!(bug.bug_id, 42);
        assert_eq!(bug.assertion_id, 100);
        assert_eq!(bug.assertion_location, "test.rs:123");
        assert_eq!(bug.tick, 5000);
    }
}
