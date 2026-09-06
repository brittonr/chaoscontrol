//! Types shared between the exploration engine and the dashboard.
//!
//! These types are defined in `chaoscontrol-explore` so the explorer can
//! emit events without depending on the dashboard crate. The dashboard
//! crate depends on `chaoscontrol-explore` and consumes these types.

use crate::campaign::SeedSummary;
use crate::checkpoint::ExplorationCheckpoint;
use crate::coverage::CoverageStats;
use crate::explorer::{AssertionDetail, AssertionStats, RoundHistory};

/// Events emitted by the explorer for live dashboard consumption.
///
/// Each variant is a self-contained snapshot — no deltas. If an SSE
/// client misses an event, the next one carries full cumulative state
/// for its category. The client can also fetch `GET /api/state` for
/// the complete picture.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum DashboardEvent {
    /// Emitted once after bootstrap completes, before the first round.
    Started {
        num_vms: usize,
        seed: u64,
        branch_factor: usize,
        ticks_per_branch: u64,
        max_rounds: u64,
        mode: String,
        kernel_path: String,
        catalog_size: usize,
        /// Which seed produced this event (campaign mode only).
        #[serde(skip_serializing_if = "Option::is_none")]
        from_seed: Option<u64>,
    },

    /// Emitted after each exploration round completes.
    RoundComplete {
        round: u64,
        branches_run: usize,
        new_edges: usize,
        cumulative_edges: usize,
        bugs_found: usize,
        cumulative_bugs: usize,
        frontier_size: usize,
        corpus_size: usize,
        /// Full assertion stats snapshot.
        assertion_stats: AssertionStats,
        /// Wall-clock time for this round.
        #[serde(default)]
        wall_clock_seconds: f64,
        /// Which seed produced this event (campaign mode only).
        #[serde(skip_serializing_if = "Option::is_none")]
        from_seed: Option<u64>,
    },

    /// Emitted when a new bug is discovered.
    BugFound {
        bug_index: usize,
        assertion_id: u64,
        assertion_message: String,
        round: u64,
        tick: u64,
        schedule_length: usize,
        /// Which seed produced this event (campaign mode only).
        #[serde(skip_serializing_if = "Option::is_none")]
        from_seed: Option<u64>,
    },

    /// Emitted when exploration finishes (all rounds, frontier exhausted, or early stop).
    Finished {
        total_rounds: u64,
        total_branches: u64,
        total_edges: usize,
        total_bugs: usize,
        reason: String,
        /// Which seed produced this event (campaign mode only).
        #[serde(skip_serializing_if = "Option::is_none")]
        from_seed: Option<u64>,
    },

    // ── Campaign-level events ────────────────────────────────────────
    /// Emitted when a campaign starts (before any seeds launch).
    CampaignStarted { seeds: Vec<u64>, seeds_total: usize },

    /// Emitted when an individual seed finishes within a campaign.
    SeedComplete { seed: u64, summary: SeedSummary },

    /// Emitted when the entire campaign finishes.
    CampaignFinished {
        seeds_total: usize,
        seeds_completed: usize,
        unique_bugs: usize,
        wall_clock_seconds: f64,
    },
}

/// Complete dashboard state — served by `GET /api/state`.
///
/// This is a cumulative snapshot of the entire exploration. Updated
/// after each round by the event receiver loop.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DashboardState {
    /// Whether an exploration is currently running.
    pub running: bool,
    /// Exploration configuration summary.
    pub config: DashboardConfig,
    /// Rounds completed so far.
    pub rounds: u64,
    /// Total branches explored.
    pub total_branches: u64,
    /// Unique coverage edges found.
    pub total_edges: usize,
    /// All bugs discovered.
    pub bugs: Vec<DashboardBug>,
    /// Corpus size.
    pub corpus_size: usize,
    /// Coverage statistics.
    pub coverage_stats: CoverageStats,
    /// Network fabric statistics.
    pub network_stats: DashboardNetworkStats,
    /// Assertion summary.
    pub assertion_stats: AssertionStats,
    /// Per-assertion detail.
    pub assertion_details: Vec<AssertionDetail>,
    /// Per-round history for charts.
    pub round_history: Vec<RoundHistory>,
    /// How the exploration ended (empty if still running).
    pub finish_reason: String,

    // ── Campaign-mode fields ──
    /// "run" or "campaign".
    #[serde(default)]
    pub mode: String,
    /// Total seeds in the campaign (0 = single run).
    #[serde(default)]
    pub seeds_total: usize,
    /// Seeds completed so far.
    #[serde(default)]
    pub seeds_completed: usize,
    /// Per-seed summaries for completed seeds.
    #[serde(default)]
    pub seed_summaries: Vec<SeedSummary>,
}

/// Exploration config summary for dashboard display.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DashboardConfig {
    pub num_vms: usize,
    pub seed: u64,
    pub branch_factor: usize,
    pub ticks_per_branch: u64,
    pub max_rounds: u64,
    pub mode: String,
    pub kernel_path: String,
}

/// Serializable mirror of `chaoscontrol_vmm::controller::NetworkStats`.
///
/// We can't impl Serialize on the VMM type (orphan rule), so the
/// dashboard uses its own copy.  Conversion is via `From`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DashboardNetworkStats {
    pub packets_sent: u64,
    pub packets_delivered: u64,
    pub packets_dropped_partition: u64,
    pub packets_dropped_loss: u64,
    pub packets_corrupted: u64,
    pub packets_duplicated: u64,
    pub packets_bandwidth_delayed: u64,
    pub packets_jittered: u64,
    pub packets_reordered: u64,
    pub total_jitter_ticks: u64,
    pub total_bandwidth_delay_ticks: u64,
}

impl From<&chaoscontrol_vmm::controller::NetworkStats> for DashboardNetworkStats {
    fn from(ns: &chaoscontrol_vmm::controller::NetworkStats) -> Self {
        Self {
            packets_sent: ns.packets_sent,
            packets_delivered: ns.packets_delivered,
            packets_dropped_partition: ns.packets_dropped_partition,
            packets_dropped_loss: ns.packets_dropped_loss,
            packets_corrupted: ns.packets_corrupted,
            packets_duplicated: ns.packets_duplicated,
            packets_bandwidth_delayed: ns.packets_bandwidth_delayed,
            packets_jittered: ns.packets_jittered,
            packets_reordered: ns.packets_reordered,
            total_jitter_ticks: ns.total_jitter_ticks,
            total_bandwidth_delay_ticks: ns.total_bandwidth_delay_ticks,
        }
    }
}

/// Bug summary for the dashboard.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DashboardBug {
    pub bug_id: u64,
    pub assertion_id: u64,
    pub assertion_message: String,
    pub round: u64,
    pub tick: u64,
    pub schedule_length: usize,
}

impl DashboardState {
    /// Create an empty state (before exploration starts).
    pub fn empty() -> Self {
        Self {
            running: false,
            config: DashboardConfig {
                num_vms: 0,
                seed: 0,
                branch_factor: 0,
                ticks_per_branch: 0,
                max_rounds: 0,
                mode: String::new(),
                kernel_path: String::new(),
            },
            rounds: 0,
            total_branches: 0,
            total_edges: 0,
            bugs: Vec::new(),
            corpus_size: 0,
            coverage_stats: CoverageStats {
                total_edges: 0,
                total_runs: 0,
                edges_per_run_avg: 0.0,
            },
            network_stats: DashboardNetworkStats::default(),
            assertion_stats: AssertionStats::default(),
            assertion_details: Vec::new(),
            round_history: Vec::new(),
            finish_reason: String::new(),
            mode: "run".to_string(),
            seeds_total: 0,
            seeds_completed: 0,
            seed_summaries: Vec::new(),
        }
    }

    /// Construct from a checkpoint after exact report-backed bug validation.
    pub fn from_checkpoint(checkpoint: &ExplorationCheckpoint) -> Result<Self, String> {
        let restored_bugs = crate::checkpoint::replay_bug_set(
            &checkpoint.bugs,
            checkpoint.assertion_report.as_ref(),
        )
        .map_err(|error| error.to_string())?;
        let (assertion_details, assertion_stats) =
            if let Some(report) = checkpoint.assertion_report.as_ref() {
                let projection = crate::assertion_report::strict_projection(report)
                    .map_err(|error| format!("{error:?}"))?;
                (projection.details, projection.stats)
            } else {
                (Vec::new(), AssertionStats::default())
            };
        let bugs: Vec<DashboardBug> = restored_bugs
            .iter()
            .map(|bug| DashboardBug {
                bug_id: bug.bug_id,
                assertion_id: bug.assertion_id,
                assertion_message: bug.assertion_location.clone(),
                round: 0,
                tick: bug.tick,
                schedule_length: bug.schedule.faults().len(),
            })
            .collect();

        Ok(Self {
            running: false,
            config: DashboardConfig {
                num_vms: checkpoint.config.num_vms,
                seed: checkpoint.config.seed,
                branch_factor: checkpoint.config.branch_factor,
                ticks_per_branch: checkpoint.config.ticks_per_branch,
                max_rounds: checkpoint.config.max_rounds,
                mode: String::new(), // not stored in checkpoint
                kernel_path: checkpoint.config.kernel_path.clone(),
            },
            rounds: checkpoint.rounds_completed,
            total_branches: checkpoint.total_branches_run,
            total_edges: checkpoint.total_edges,
            bugs,
            corpus_size: 0, // not stored in checkpoint
            coverage_stats: CoverageStats {
                total_edges: checkpoint.total_edges,
                total_runs: checkpoint.total_branches_run,
                edges_per_run_avg: if checkpoint.total_branches_run > 0 {
                    checkpoint.total_edges as f64 / checkpoint.total_branches_run as f64
                } else {
                    0.0
                },
            },
            network_stats: DashboardNetworkStats::default(),
            assertion_stats,
            assertion_details,
            round_history: checkpoint.round_history.clone().unwrap_or_default(),
            finish_reason: "completed".to_string(),
            mode: "run".to_string(),
            seeds_total: 0,
            seeds_completed: 0,
            seed_summaries: Vec::new(),
        })
    }

    /// Update state from a RoundComplete event.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_round_complete(
        &mut self,
        round: u64,
        branches_run: usize,
        new_edges: usize,
        cumulative_edges: usize,
        bugs_found: usize,
        cumulative_bugs: usize,
        frontier_size: usize,
        corpus_size: usize,
        assertion_stats: &AssertionStats,
    ) {
        self.rounds = round;
        self.total_edges = cumulative_edges;
        self.corpus_size = corpus_size;
        self.assertion_stats = assertion_stats.clone();

        self.round_history.push(RoundHistory {
            round,
            branches_run,
            new_edges,
            cumulative_edges,
            bugs_found,
            cumulative_bugs,
            frontier_size,
            corpus_size,
            restore_ms: 0.0,
            run_ms: 0.0,
            snapshot_ms: 0.0,
            coverage_ms: 0.0,
            wall_clock_seconds: 0.0,
        });
    }

    /// Update state from a BugFound event.
    pub fn apply_bug_found(&mut self, bug: DashboardBug) {
        self.bugs.push(bug);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::SerializableBug;

    #[test]
    fn test_dashboard_event_serialization() {
        let event = DashboardEvent::RoundComplete {
            round: 5,
            branches_run: 8,
            new_edges: 12,
            cumulative_edges: 100,
            bugs_found: 1,
            cumulative_bugs: 2,
            frontier_size: 10,
            corpus_size: 15,
            assertion_stats: AssertionStats {
                catalog_size: 30,
                passed: 25,
                failed: 2,
                unexercised: 3,
            },
            wall_clock_seconds: 1.5,
            from_seed: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"RoundComplete\""));
        assert!(json.contains("\"round\":5"));

        let roundtrip: DashboardEvent = serde_json::from_str(&json).unwrap();
        match roundtrip {
            DashboardEvent::RoundComplete { round, .. } => assert_eq!(round, 5),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_dashboard_state_empty() {
        let state = DashboardState::empty();
        assert_eq!(state.rounds, 0);
        assert!(!state.running);
        assert!(state.bugs.is_empty());
        assert!(state.round_history.is_empty());
    }

    #[test]
    fn test_dashboard_state_apply_round() {
        let mut state = DashboardState::empty();
        state.running = true;

        state.apply_round_complete(
            1,
            8,
            42,
            42,
            0,
            0,
            3,
            3,
            &AssertionStats {
                catalog_size: 10,
                passed: 8,
                failed: 0,
                unexercised: 2,
            },
        );

        assert_eq!(state.rounds, 1);
        assert_eq!(state.total_edges, 42);
        assert_eq!(state.round_history.len(), 1);
        assert_eq!(state.round_history[0].new_edges, 42);
    }

    #[test]
    fn test_dashboard_state_apply_bug() {
        let mut state = DashboardState::empty();
        state.apply_bug_found(DashboardBug {
            bug_id: 0,
            assertion_id: 12345,
            assertion_message: "leader completeness".to_string(),
            round: 3,
            tick: 500,
            schedule_length: 5,
        });

        assert_eq!(state.bugs.len(), 1);
        assert_eq!(state.bugs[0].assertion_id, 12345);
    }

    #[test]
    fn test_dashboard_state_from_checkpoint() {
        const ASSERTION_ALIAS: u64 = 99;
        let identity = crate::test_support::assertion_identity(ASSERTION_ALIAS);
        let assertion_location = identity.descriptor.message.clone();
        let assertion_report = crate::test_support::assertion_report(ASSERTION_ALIAS, false);
        let mut checkpoint = ExplorationCheckpoint {
            config: crate::checkpoint::CheckpointConfig {
                num_vms: 3,
                kernel_path: "/path/to/kernel".to_string(),
                initrd_path: Some("/path/to/initrd".to_string()),
                seed: 42,
                branch_factor: 8,
                ticks_per_branch: 1000,
                max_rounds: 100,
                max_frontier: 50,
                quantum: 100,
                coverage_gpa: 0xE0000,
                disk_image_path: None,
                bootstrap_budget: 10_000,
                schedule_diversity: false,
                schedule_mutation_ratio: 0.0,
                rare_edge_threshold: None,
                rare_edge_weight: None,
                havoc_after_stale: None,
                havoc_mutations: None,
                scenario: None,
            },
            global_coverage: vec![1, 2, 3],
            bugs: vec![SerializableBug {
                bug_id: 0,
                assertion_id: ASSERTION_ALIAS,
                assertion_identity: Some(identity),
                fallback_scope: None,
                assertion_location,
                schedule: crate::checkpoint::SerializableSchedule { faults: vec![] },
                tick: 500,
                replay_parent_depth: 0,
                replay_parent_snapshot_ref: None,
                dedup_key: Some(0),
                schedule_variant: None,
                scenario_config: None,
                scenario_summary: None,
            }],
            assertion_report: Some(assertion_report),
            rounds_completed: 10,
            total_branches_run: 80,
            total_edges: 200,
            seed: 42,
            round_history: Some(vec![RoundHistory {
                round: 1,
                branches_run: 8,
                new_edges: 50,
                cumulative_edges: 50,
                bugs_found: 0,
                cumulative_bugs: 0,
                frontier_size: 3,
                corpus_size: 3,
                restore_ms: 0.0,
                run_ms: 0.0,
                snapshot_ms: 0.0,
                coverage_ms: 0.0,
                wall_clock_seconds: 0.0,
            }]),
            seen_dedup_keys: None,
            scenario: None,
            scenario_summary: None,
        };

        let state = DashboardState::from_checkpoint(&checkpoint).expect("valid checkpoint");

        assert!(!state.running);
        assert_eq!(state.rounds, 10);
        assert_eq!(state.total_branches, 80);
        assert_eq!(state.total_edges, 200);
        assert_eq!(state.bugs.len(), 1);
        assert_eq!(state.bugs[0].assertion_id, ASSERTION_ALIAS);
        assert_eq!(state.assertion_stats.failed, 1);
        assert_eq!(state.round_history.len(), 1);
        assert_eq!(state.config.num_vms, 3);

        checkpoint.bugs[0]
            .assertion_identity
            .as_mut()
            .expect("bug identity")
            .catalog_token = chaoscontrol_protocol::identity::AssertionFingerprint::ZERO;
        assert!(DashboardState::from_checkpoint(&checkpoint).is_err());
    }

    #[test]
    fn test_dashboard_state_json_roundtrip() {
        let state = DashboardState::empty();
        let json = serde_json::to_string(&state).unwrap();
        let roundtrip: DashboardState = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.rounds, 0);
        assert!(roundtrip.bugs.is_empty());
    }

    #[test]
    fn test_all_event_variants_serialize() {
        let events = vec![
            DashboardEvent::Started {
                num_vms: 3,
                seed: 42,
                branch_factor: 8,
                ticks_per_branch: 1000,
                max_rounds: 100,
                mode: "fault-schedule".to_string(),
                kernel_path: "/path/to/kernel".to_string(),
                catalog_size: 35,
                from_seed: None,
            },
            DashboardEvent::RoundComplete {
                round: 1,
                branches_run: 8,
                new_edges: 50,
                cumulative_edges: 50,
                bugs_found: 0,
                cumulative_bugs: 0,
                frontier_size: 3,
                corpus_size: 3,
                assertion_stats: AssertionStats::default(),
                wall_clock_seconds: 0.0,
                from_seed: None,
            },
            DashboardEvent::BugFound {
                bug_index: 0,
                assertion_id: 123,
                assertion_message: "election safety".to_string(),
                round: 5,
                tick: 750,
                schedule_length: 3,
                from_seed: None,
            },
            DashboardEvent::Finished {
                total_rounds: 100,
                total_branches: 800,
                total_edges: 500,
                total_bugs: 2,
                reason: "completed".to_string(),
                from_seed: None,
            },
        ];

        for event in &events {
            let json = serde_json::to_string(event).unwrap();
            let _: DashboardEvent = serde_json::from_str(&json).unwrap();
        }
    }
}
