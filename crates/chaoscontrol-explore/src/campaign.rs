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
use crate::report::format_campaign_report;
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
    /// Seeds that panicked or returned errors.
    #[serde(default)]
    pub failed_seeds: Vec<(u64, String)>,
}

// ═══════════════════════════════════════════════════════════════════════
//  Campaign checkpoint / resume
// ═══════════════════════════════════════════════════════════════════════

/// Serializable subset of ExplorerConfig for campaign checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableCampaignConfig {
    pub kernel_path: String,
    pub initrd_path: Option<String>,
    pub num_vms: usize,
    pub branch_factor: usize,
    pub ticks_per_branch: u64,
    pub max_rounds: u64,
    pub quantum: u64,
    pub exploration_mode: String,
    pub seed: u64,
    pub disk_image_path: Option<String>,
    pub bootstrap_budget: u64,
    pub stale_round_limit: u64,
    pub num_vcpus: usize,
}

impl SerializableCampaignConfig {
    pub fn from_explorer_config(cfg: &ExplorerConfig) -> Self {
        Self {
            kernel_path: cfg.kernel_path.clone(),
            initrd_path: cfg.initrd_path.clone(),
            num_vms: cfg.num_vms,
            branch_factor: cfg.branch_factor,
            ticks_per_branch: cfg.ticks_per_branch,
            max_rounds: cfg.max_rounds,
            quantum: cfg.quantum,
            exploration_mode: match cfg.exploration_mode {
                crate::explorer::ExplorationMode::FaultSchedule => "fault-schedule".to_string(),
                crate::explorer::ExplorationMode::InputTree => "input-tree".to_string(),
                crate::explorer::ExplorationMode::Hybrid => "hybrid".to_string(),
            },
            seed: cfg.seed,
            disk_image_path: cfg.disk_image_path.clone(),
            bootstrap_budget: cfg.bootstrap_budget,
            stale_round_limit: cfg.stale_round_limit,
            num_vcpus: cfg.vm_config.num_vcpus,
        }
    }
}

/// Incremental campaign checkpoint — updated after each seed completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignProgress {
    /// All seeds in the campaign.
    pub seeds: Vec<u64>,
    /// Serializable base config.
    pub config: SerializableCampaignConfig,
    /// Output directory.
    pub output_dir: String,
    /// Completed seeds and their summaries.
    pub completed: BTreeMap<u64, SeedSummary>,
    /// Seeds that failed (panicked or returned error) with error messages.
    #[serde(default)]
    pub failed: BTreeMap<u64, String>,
}

/// Save campaign progress to `{output_dir}/campaign_progress.json`.
pub fn save_campaign_progress(
    progress: &CampaignProgress,
    output_dir: &str,
) -> Result<(), std::io::Error> {
    let path = format!("{}/campaign_progress.json", output_dir);
    let json = serde_json::to_string_pretty(progress).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)?;
    info!("Saved campaign progress: {}", path);
    Ok(())
}

/// Load campaign progress from `{dir}/campaign_progress.json`.
pub fn load_campaign_progress(dir: &str) -> Result<CampaignProgress, std::io::Error> {
    let path = format!("{}/campaign_progress.json", dir);
    let json = std::fs::read_to_string(&path)?;
    serde_json::from_str(&json).map_err(std::io::Error::other)
}

// ═══════════════════════════════════════════════════════════════════════
//  Campaign runner
// ═══════════════════════════════════════════════════════════════════════

/// Orchestrates multi-seed exploration.
pub struct CampaignRunner {
    config: CampaignConfig,
}

/// Result from a single seed's exploration thread.
enum SeedResult {
    Ok {
        seed: u64,
        report: Box<ExplorationReport>,
        wall_clock_seconds: f64,
    },
    Failed {
        seed: u64,
        error: String,
    },
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
        // Each thread catches panics so one bad seed doesn't kill the campaign.
        let seed_results: Vec<SeedResult> = std::thread::scope(|s| {
            let handles: Vec<_> = self
                .config
                .seeds
                .iter()
                .map(|&seed| {
                    let mut explorer_config = self.config.base_explorer_config.clone();
                    explorer_config.seed = seed;
                    // num_workers comes from ExplorerConfig (set by CLI --workers-per-seed).
                    let seed_output = format!("{}/seed_{}", self.config.output_dir, seed);
                    explorer_config.output_dir = Some(seed_output);

                    s.spawn(move || -> SeedResult {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let thread_start = Instant::now();
                            let mut explorer = Explorer::new(explorer_config);
                            let report = explorer.run()?;
                            let elapsed = thread_start.elapsed().as_secs_f64();
                            Ok::<_, crate::explorer::ExploreError>((report, elapsed))
                        }));

                        match result {
                            Ok(Ok((report, elapsed))) => SeedResult::Ok {
                                seed,
                                report: Box::new(report),
                                wall_clock_seconds: elapsed,
                            },
                            Ok(Err(e)) => {
                                log::error!("Seed {} failed: {}", seed, e);
                                SeedResult::Failed {
                                    seed,
                                    error: format!("{}", e),
                                }
                            }
                            Err(panic_payload) => {
                                let msg = panic_message(&panic_payload);
                                log::error!("Seed {} panicked: {}", seed, msg);
                                SeedResult::Failed {
                                    seed,
                                    error: format!("panic: {}", msg),
                                }
                            }
                        }
                    })
                })
                .collect();

            handles
                .into_iter()
                .map(|h| h.join().expect("seed thread poisoned"))
                .collect()
        });

        // Collect results, printing per-seed summaries as we go.
        // Update campaign_progress.json after each seed completes.
        let mut progress = CampaignProgress {
            seeds: self.config.seeds.clone(),
            config: SerializableCampaignConfig::from_explorer_config(
                &self.config.base_explorer_config,
            ),
            output_dir: self.config.output_dir.clone(),
            completed: BTreeMap::new(),
            failed: BTreeMap::new(),
        };

        let mut reports: Vec<(u64, ExplorationReport, f64)> = Vec::with_capacity(num_seeds);
        let mut failed_seeds: Vec<(u64, String)> = Vec::new();

        for sr in seed_results {
            match sr {
                SeedResult::Ok {
                    seed,
                    report: boxed_report,
                    wall_clock_seconds,
                } => {
                    let report = *boxed_report;
                    eprintln!(
                        "[seed {}] done: {} rounds, {} branches, {} edges, {} bug{} ({:.1}s)",
                        seed,
                        report.rounds,
                        report.total_branches,
                        report.total_edges,
                        report.bugs.len(),
                        if report.bugs.len() == 1 { "" } else { "s" },
                        wall_clock_seconds,
                    );

                    progress.completed.insert(
                        seed,
                        SeedSummary {
                            seed,
                            rounds: report.rounds,
                            total_branches: report.total_branches,
                            total_edges: report.total_edges,
                            bugs_found: report.bugs.len(),
                            wall_clock_seconds,
                        },
                    );
                    reports.push((seed, report, wall_clock_seconds));
                }
                SeedResult::Failed { seed, error } => {
                    eprintln!("[seed {}] FAILED: {}", seed, error);
                    progress.failed.insert(seed, error.clone());
                    failed_seeds.push((seed, error));
                }
            }

            if let Err(e) = save_campaign_progress(&progress, &self.config.output_dir) {
                log::warn!("Failed to save campaign progress: {}", e);
            }
        }

        let wall_clock = campaign_start.elapsed().as_secs_f64();
        let interrupted = crate::signal::shutdown_requested();
        let mut campaign_report = aggregate_reports(reports, wall_clock);
        campaign_report.failed_seeds = failed_seeds;

        let status_word = if interrupted {
            "interrupted"
        } else {
            "complete"
        };
        eprintln!(
            "Campaign {}: {} seeds, {} unique bug{}, {:.1}s wall-clock",
            status_word,
            campaign_report.seeds_run.len(),
            campaign_report.bugs.len(),
            if campaign_report.bugs.len() == 1 {
                ""
            } else {
                "s"
            },
            wall_clock,
        );

        // Save campaign report to disk.
        let json_path = format!("{}/campaign_report.json", self.config.output_dir);
        let txt_path = format!("{}/campaign_report.txt", self.config.output_dir);

        match serde_json::to_string_pretty(&campaign_report) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&json_path, &json) {
                    log::warn!("Failed to write {}: {}", json_path, e);
                } else {
                    info!("Saved {}", json_path);
                }
            }
            Err(e) => log::warn!("Failed to serialize campaign report: {}", e),
        }

        let txt = format_campaign_report(&campaign_report);
        if let Err(e) = std::fs::write(&txt_path, &txt) {
            log::warn!("Failed to write {}: {}", txt_path, e);
        } else {
            info!("Saved {}", txt_path);
        }

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
        failed_seeds: Vec::new(),
    }
}

/// Extract a human-readable message from a panic payload.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
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
            wall_clock_seconds: 0.0,
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
            failed_seeds: Vec::new(),
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

    // ── report persistence ──────────────────────────────────────────

    #[test]
    fn campaign_report_files_written() {
        let dir = std::env::temp_dir().join(format!("cc-test-report-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let reports = vec![
            (
                42,
                make_report(vec![make_bug(0, 100, "a.rs:1", 0xAA)], Vec::new()),
                1.0,
            ),
            (43, make_report(Vec::new(), Vec::new()), 2.0),
        ];
        let campaign_report = aggregate_reports(reports, 3.0);

        // Write JSON
        let json_path = dir.join("campaign_report.json");
        let json = serde_json::to_string_pretty(&campaign_report).unwrap();
        std::fs::write(&json_path, &json).unwrap();

        // Write TXT
        let txt_path = dir.join("campaign_report.txt");
        let txt = crate::report::format_campaign_report(&campaign_report);
        std::fs::write(&txt_path, &txt).unwrap();

        // Verify JSON roundtrip
        let loaded: CampaignReport =
            serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
        assert_eq!(loaded.seeds_run, vec![42, 43]);
        assert_eq!(loaded.bugs.len(), 1);

        // Verify TXT contains key sections
        let txt_content = std::fs::read_to_string(&txt_path).unwrap();
        assert!(txt_content.contains("Campaign"));
        assert!(txt_content.contains("42"));

        std::fs::remove_dir_all(&dir).ok();
    }

    // ── campaign progress checkpoint ────────────────────────────────

    #[test]
    fn campaign_progress_serde_roundtrip() {
        let progress = CampaignProgress {
            seeds: vec![42, 43, 44],
            config: SerializableCampaignConfig {
                kernel_path: "vmlinux".into(),
                initrd_path: Some("initrd.gz".into()),
                num_vms: 3,
                branch_factor: 8,
                ticks_per_branch: 1000,
                max_rounds: 100,
                quantum: 100,
                exploration_mode: "hybrid".into(),
                seed: 42,
                disk_image_path: None,
                bootstrap_budget: 10000,
                stale_round_limit: 10,
                num_vcpus: 1,
            },
            output_dir: "results/".into(),
            completed: BTreeMap::from([(
                42,
                SeedSummary {
                    seed: 42,
                    rounds: 10,
                    total_branches: 80,
                    total_edges: 256,
                    bugs_found: 1,
                    wall_clock_seconds: 23.0,
                },
            )]),
            failed: BTreeMap::new(),
        };

        let json = serde_json::to_string_pretty(&progress).unwrap();
        let loaded: CampaignProgress = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.seeds, vec![42, 43, 44]);
        assert_eq!(loaded.completed.len(), 1);
        assert!(loaded.completed.contains_key(&42));
        assert_eq!(loaded.config.num_vms, 3);
        assert_eq!(loaded.config.exploration_mode, "hybrid");
    }

    #[test]
    fn campaign_progress_save_load() {
        let dir = std::env::temp_dir().join(format!("cc-test-progress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let progress = CampaignProgress {
            seeds: vec![10, 20],
            config: SerializableCampaignConfig {
                kernel_path: "k".into(),
                initrd_path: None,
                num_vms: 1,
                branch_factor: 4,
                ticks_per_branch: 500,
                max_rounds: 50,
                quantum: 100,
                exploration_mode: "fault-schedule".into(),
                seed: 10,
                disk_image_path: None,
                bootstrap_budget: 5000,
                stale_round_limit: 5,
                num_vcpus: 1,
            },
            output_dir: dir.to_string_lossy().into(),
            completed: BTreeMap::new(),
            failed: BTreeMap::new(),
        };

        save_campaign_progress(&progress, &dir.to_string_lossy()).unwrap();
        let loaded = load_campaign_progress(&dir.to_string_lossy()).unwrap();

        assert_eq!(loaded.seeds, vec![10, 20]);
        assert!(loaded.completed.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }
}
