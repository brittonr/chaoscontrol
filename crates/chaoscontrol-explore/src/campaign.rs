//! Multi-seed campaign runner for ChaosControl.
//!
//! Launches N independent explorations with different seeds in parallel,
//! then aggregates bugs, coverage, and assertion verdicts into a unified
//! report. Each seed runs in its own OS thread with its own KVM VMs —
//! no shared mutable state between seeds.

use crate::checkpoint::SerializableBug;
use crate::explorer::{
    AssertionDetail, AssertionStats, ExplorationReport, Explorer, ExplorerConfig,
};
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Instant;

/// Configuration for a multi-seed campaign.
#[derive(Clone)]
pub struct CampaignConfig {
    /// Seeds to explore. Each gets its own Explorer instance.
    pub seeds: Vec<u64>,
    /// Base config — cloned per seed with seed/output_dir overridden.
    pub base_explorer_config: ExplorerConfig,
    /// Top-level output directory. Per-seed output goes to `{output_dir}/seed_{N}/`.
    pub output_dir: String,
}

/// Per-seed summary in the campaign report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedSummary {
    pub seed: u64,
    pub rounds: u64,
    pub total_branches: u64,
    pub total_edges: usize,
    pub bugs_found: usize,
    pub wall_clock_seconds: f64,
}

/// A bug deduplicated across seeds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignBug {
    /// The underlying bug report (from whichever seed found it first).
    pub bug: SerializableBug,
    /// Seeds that triggered this bug.
    pub found_by_seeds: Vec<u64>,
    /// First seed to find it.
    pub first_seed: u64,
    /// Dedup key: hash(assertion_id, sorted fault type names).
    pub dedup_key: u64,
}

/// Aggregated report across all seeds in a campaign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignReport {
    /// All seeds that were run.
    pub seeds_run: Vec<u64>,
    /// Seeds that found at least one bug.
    pub seeds_with_bugs: Vec<u64>,
    /// Sum of rounds across all seeds.
    pub total_rounds: u64,
    /// Sum of branches across all seeds.
    pub total_branches: u64,
    /// Deduplicated bugs across seeds.
    pub bugs: Vec<CampaignBug>,
    /// Per-seed summaries.
    pub per_seed: Vec<SeedSummary>,
    /// Merged assertion details (summed counts, worst verdict).
    pub assertion_details: Vec<AssertionDetail>,
    /// Merged assertion stats.
    pub assertion_stats: AssertionStats,
    /// Wall-clock time for the entire campaign.
    pub wall_clock_seconds: f64,
}

/// Orchestrates multi-seed exploration.
pub struct CampaignRunner {
    config: CampaignConfig,
}

/// Result from a single seed's exploration thread.
struct SeedResult {
    seed: u64,
    report: ExplorationReport,
    wall_clock_seconds: f64,
}

impl CampaignRunner {
    pub fn new(config: CampaignConfig) -> Self {
        Self { config }
    }

    /// Run all seeds in parallel, return the aggregated report.
    pub fn run(&self) -> Result<CampaignReport, crate::explorer::ExploreError> {
        let num_seeds = self.config.seeds.len();
        let num_vms = self.config.base_explorer_config.num_vms;
        let vm_memory_mb = self.config.base_explorer_config.vm_config.memory_size / (1024 * 1024);

        // Log memory estimate.
        let estimated_mb = num_seeds * num_vms * vm_memory_mb;
        info!(
            "Campaign: {} seeds × {} VMs × {} MB = ~{:.1} GB estimated memory",
            num_seeds,
            num_vms,
            vm_memory_mb,
            estimated_mb as f64 / 1024.0,
        );
        info!(
            "Launching {} seed{} in parallel...",
            num_seeds,
            if num_seeds == 1 { "" } else { "s" }
        );

        let campaign_start = Instant::now();

        // Create output directories.
        std::fs::create_dir_all(&self.config.output_dir).ok();

        // Run seeds in parallel via scoped threads.
        let seed_results: Vec<Result<SeedResult, crate::explorer::ExploreError>> =
            std::thread::scope(|s| {
                let handles: Vec<_> = self
                    .config
                    .seeds
                    .iter()
                    .map(|&seed| {
                        let mut explorer_config = self.config.base_explorer_config.clone();
                        explorer_config.seed = seed;
                        explorer_config.num_workers = 1; // No within-round parallelism.
                        let seed_output = format!("{}/seed_{}", self.config.output_dir, seed);
                        explorer_config.output_dir = Some(seed_output);

                        s.spawn(move || {
                            let thread_start = Instant::now();
                            let mut explorer = Explorer::new(explorer_config);
                            let report = explorer.run()?;
                            let elapsed = thread_start.elapsed().as_secs_f64();
                            Ok(SeedResult {
                                seed,
                                report,
                                wall_clock_seconds: elapsed,
                            })
                        })
                    })
                    .collect();

                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });

        // Collect results, printing per-seed summaries as we go.
        let mut reports: Vec<(u64, ExplorationReport, f64)> = Vec::with_capacity(num_seeds);
        for result in seed_results {
            let sr = result?;
            eprintln!(
                "[seed {}] done: {} rounds, {} branches, {} edges, {} bug{} ({:.1}s)",
                sr.seed,
                sr.report.rounds,
                sr.report.total_branches,
                sr.report.total_edges,
                sr.report.bugs.len(),
                if sr.report.bugs.len() == 1 { "" } else { "s" },
                sr.wall_clock_seconds,
            );
            reports.push((sr.seed, sr.report, sr.wall_clock_seconds));
        }

        let wall_clock = campaign_start.elapsed().as_secs_f64();
        let campaign_report = aggregate_reports(reports, wall_clock);

        eprintln!(
            "Campaign complete: {} seeds, {} unique bug{}, {:.1}s wall-clock",
            campaign_report.seeds_run.len(),
            campaign_report.bugs.len(),
            if campaign_report.bugs.len() == 1 {
                ""
            } else {
                "s"
            },
            wall_clock,
        );

        Ok(campaign_report)
    }
}

/// Generate seed list: explicit seeds if provided, else base_seed..base_seed+n.
pub fn generate_seeds(base_seed: u64, count: usize, explicit: Option<&[u64]>) -> Vec<u64> {
    if let Some(seeds) = explicit {
        seeds.to_vec()
    } else {
        (0..count as u64).map(|i| base_seed + i).collect()
    }
}

/// Merge per-seed `ExplorationReport`s into a `CampaignReport`.
pub fn aggregate_reports(
    seed_reports: Vec<(u64, ExplorationReport, f64)>,
    wall_clock_seconds: f64,
) -> CampaignReport {
    let mut seeds_run = Vec::new();
    let mut seeds_with_bugs = Vec::new();
    let mut total_rounds = 0u64;
    let mut total_branches = 0u64;
    let mut per_seed = Vec::new();

    // Bug dedup: dedup_key → CampaignBug
    let mut bug_map: BTreeMap<u64, CampaignBug> = BTreeMap::new();

    // Assertion merge: id → merged detail
    let mut assertion_map: BTreeMap<u32, AssertionDetail> = BTreeMap::new();

    for (seed, report, elapsed) in &seed_reports {
        seeds_run.push(*seed);
        total_rounds += report.rounds;
        total_branches += report.total_branches;

        if !report.bugs.is_empty() {
            seeds_with_bugs.push(*seed);
        }

        per_seed.push(SeedSummary {
            seed: *seed,
            rounds: report.rounds,
            total_branches: report.total_branches,
            total_edges: report.total_edges,
            bugs_found: report.bugs.len(),
            wall_clock_seconds: *elapsed,
        });

        // Merge bugs by dedup_key.
        for bug in &report.bugs {
            let key = bug.dedup_key;
            if let Some(existing) = bug_map.get_mut(&key) {
                if !existing.found_by_seeds.contains(seed) {
                    existing.found_by_seeds.push(*seed);
                }
            } else {
                bug_map.insert(
                    key,
                    CampaignBug {
                        bug: bug.into(),
                        found_by_seeds: vec![*seed],
                        first_seed: *seed,
                        dedup_key: key,
                    },
                );
            }
        }

        // Merge assertion details.
        for detail in &report.assertion_details {
            if let Some(existing) = assertion_map.get_mut(&detail.id) {
                existing.hit_count += detail.hit_count;
                existing.true_count += detail.true_count;
                existing.false_count += detail.false_count;
                // Worst verdict wins: failed > unexercised > passed.
                if verdict_rank(&detail.verdict) < verdict_rank(&existing.verdict) {
                    existing.verdict = detail.verdict.clone();
                }
                // Keep latest failure details.
                if detail.last_failure_details.is_some() {
                    existing.last_failure_details = detail.last_failure_details.clone();
                }
            } else {
                assertion_map.insert(detail.id, detail.clone());
            }
        }
    }

    // Build final assertion stats from merged details.
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut unexercised = 0usize;
    let mut assertion_details: Vec<AssertionDetail> = assertion_map.into_values().collect();

    // Sort: failed first, then unexercised, then passed.
    assertion_details.sort_by(|a, b| {
        verdict_rank(&a.verdict)
            .cmp(&verdict_rank(&b.verdict))
            .then(a.id.cmp(&b.id))
    });

    for d in &assertion_details {
        match d.verdict.as_str() {
            "failed" => failed += 1,
            "unexercised" => unexercised += 1,
            _ => passed += 1,
        }
    }

    let assertion_stats = AssertionStats {
        catalog_size: assertion_details.len(),
        passed,
        failed,
        unexercised,
    };

    let bugs: Vec<CampaignBug> = bug_map.into_values().collect();

    CampaignReport {
        seeds_run,
        seeds_with_bugs,
        total_rounds,
        total_branches,
        bugs,
        per_seed,
        assertion_details,
        assertion_stats,
        wall_clock_seconds,
    }
}

/// Lower rank = worse verdict (for "worst wins" merge).
fn verdict_rank(verdict: &str) -> u8 {
    match verdict {
        "failed" => 0,
        "unexercised" => 1,
        _ => 2, // "passed"
    }
}

/// Format the memory estimate string.
pub fn format_memory_estimate(num_seeds: usize, num_vms: usize, vm_memory_mb: usize) -> String {
    let total_mb = num_seeds * num_vms * vm_memory_mb;
    format!(
        "Estimated memory: {:.1} GB ({} seeds × {} VMs × {} MB)",
        total_mb as f64 / 1024.0,
        num_seeds,
        num_vms,
        vm_memory_mb,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::SerializableSchedule;
    use crate::corpus::BugReport;
    use crate::coverage::CoverageStats;
    use chaoscontrol_fault::schedule::FaultSchedule;

    // ── 6.1: seed generation ────────────────────────────────────────

    #[test]
    fn seed_generation_default_sequence() {
        let seeds = generate_seeds(42, 5, None);
        assert_eq!(seeds, vec![42, 43, 44, 45, 46]);
    }

    #[test]
    fn seed_generation_explicit_list() {
        let explicit = vec![10, 99, 200];
        let seeds = generate_seeds(42, 5, Some(&explicit));
        assert_eq!(seeds, vec![10, 99, 200]);
    }

    #[test]
    fn seed_generation_single() {
        let seeds = generate_seeds(0, 1, None);
        assert_eq!(seeds, vec![0]);
    }

    // ── 6.2: bug dedup across seeds ─────────────────────────────────

    fn make_bug(id: u64, assertion_id: u64, location: &str, dedup_key: u64) -> BugReport {
        BugReport {
            bug_id: id,
            assertion_id,
            assertion_location: location.to_string(),
            schedule: FaultSchedule::new(),
            snapshot: None,
            tick: 1000,
            dedup_key,
            schedule_variant: None,
        }
    }

    fn make_report(bugs: Vec<BugReport>, details: Vec<AssertionDetail>) -> ExplorationReport {
        ExplorationReport {
            rounds: 10,
            total_branches: 80,
            total_edges: 100,
            bugs,
            corpus_size: 5,
            coverage_stats: CoverageStats {
                total_edges: 100,
                total_runs: 80,
                edges_per_run_avg: 1.25,
            },
            network_stats: Default::default(),
            assertion_stats: Default::default(),
            assertion_details: details,
            round_history: Vec::new(),
        }
    }

    #[test]
    fn bug_dedup_same_key_merged() {
        let bug_a = make_bug(0, 100, "safety.rs:10", 0xAAAA);
        let bug_b = make_bug(1, 100, "safety.rs:10", 0xAAAA);

        let reports = vec![
            (42, make_report(vec![bug_a], Vec::new()), 1.0),
            (43, make_report(vec![bug_b], Vec::new()), 1.0),
        ];

        let campaign = aggregate_reports(reports, 2.0);
        assert_eq!(campaign.bugs.len(), 1);
        assert_eq!(campaign.bugs[0].found_by_seeds, vec![42, 43]);
        assert_eq!(campaign.bugs[0].first_seed, 42);
    }

    #[test]
    fn bug_dedup_different_keys_kept() {
        let bug_a = make_bug(0, 100, "safety.rs:10", 0xAAAA);
        let bug_b = make_bug(1, 200, "liveness.rs:20", 0xBBBB);

        let reports = vec![
            (42, make_report(vec![bug_a], Vec::new()), 1.0),
            (43, make_report(vec![bug_b], Vec::new()), 1.0),
        ];

        let campaign = aggregate_reports(reports, 2.0);
        assert_eq!(campaign.bugs.len(), 2);
    }

    // ── 6.3: assertion merging ──────────────────────────────────────

    fn make_detail(id: u32, verdict: &str, hits: u64, t: u64, f: u64) -> AssertionDetail {
        AssertionDetail {
            id,
            message: format!("assertion_{}", id),
            kind: "always".to_string(),
            verdict: verdict.to_string(),
            hit_count: hits,
            true_count: t,
            false_count: f,
            last_failure_details: None,
        }
    }

    #[test]
    fn assertion_merge_sums_counts() {
        let d1 = make_detail(100, "passed", 50, 50, 0);
        let d2 = make_detail(100, "passed", 30, 30, 0);

        let reports = vec![
            (42, make_report(Vec::new(), vec![d1]), 1.0),
            (43, make_report(Vec::new(), vec![d2]), 1.0),
        ];

        let campaign = aggregate_reports(reports, 2.0);
        let merged = campaign
            .assertion_details
            .iter()
            .find(|d| d.id == 100)
            .unwrap();
        assert_eq!(merged.hit_count, 80);
        assert_eq!(merged.true_count, 80);
    }

    #[test]
    fn assertion_merge_worst_verdict_wins() {
        let d1 = make_detail(100, "passed", 50, 50, 0);
        let d2 = make_detail(100, "failed", 30, 20, 10);

        let reports = vec![
            (42, make_report(Vec::new(), vec![d1]), 1.0),
            (43, make_report(Vec::new(), vec![d2]), 1.0),
        ];

        let campaign = aggregate_reports(reports, 2.0);
        let merged = campaign
            .assertion_details
            .iter()
            .find(|d| d.id == 100)
            .unwrap();
        assert_eq!(merged.verdict, "failed");
        assert_eq!(merged.hit_count, 80);
        assert_eq!(merged.false_count, 10);
    }

    // ── 6.4: serde roundtrip ────────────────────────────────────────

    #[test]
    fn campaign_report_serde_roundtrip() {
        let report = CampaignReport {
            seeds_run: vec![42, 43],
            seeds_with_bugs: vec![42],
            total_rounds: 20,
            total_branches: 160,
            bugs: vec![CampaignBug {
                bug: SerializableBug {
                    bug_id: 0,
                    assertion_id: 100,
                    assertion_location: "test.rs:1".into(),
                    schedule: SerializableSchedule { faults: Vec::new() },
                    tick: 500,
                    dedup_key: Some(0xAAAA),
                    schedule_variant: None,
                },
                found_by_seeds: vec![42],
                first_seed: 42,
                dedup_key: 0xAAAA,
            }],
            per_seed: vec![
                SeedSummary {
                    seed: 42,
                    rounds: 10,
                    total_branches: 80,
                    total_edges: 100,
                    bugs_found: 1,
                    wall_clock_seconds: 23.4,
                },
                SeedSummary {
                    seed: 43,
                    rounds: 10,
                    total_branches: 80,
                    total_edges: 80,
                    bugs_found: 0,
                    wall_clock_seconds: 21.1,
                },
            ],
            assertion_details: Vec::new(),
            assertion_stats: AssertionStats::default(),
            wall_clock_seconds: 25.0,
        };

        let json = serde_json::to_string_pretty(&report).unwrap();
        let roundtrip: CampaignReport = serde_json::from_str(&json).unwrap();

        assert_eq!(roundtrip.seeds_run, vec![42, 43]);
        assert_eq!(roundtrip.bugs.len(), 1);
        assert_eq!(roundtrip.per_seed.len(), 2);
        assert_eq!(roundtrip.total_rounds, 20);
    }

    // ── 6.6: memory estimate ────────────────────────────────────────

    #[test]
    fn memory_estimate_format() {
        let s = format_memory_estimate(5, 3, 256);
        assert!(s.contains("3.8 GB"));
        assert!(s.contains("5 seeds"));
        assert!(s.contains("3 VMs"));
        assert!(s.contains("256 MB"));
    }

    #[test]
    fn memory_estimate_small() {
        let s = format_memory_estimate(1, 1, 256);
        assert!(s.contains("0.3 GB") || s.contains("0.2 GB"));
        assert!(s.contains("1 seeds"));
    }
}
