//! Multi-seed campaign runner for ChaosControl.
//!
//! Launches N independent explorations with different seeds in parallel,
//! then aggregates bugs, coverage, and assertion verdicts into a unified
//! report. Each seed runs in its own OS thread with its own KVM VMs —
//! no shared mutable state between seeds.

use crate::checkpoint::{BugSetIdentityError, SerializableBug};
use crate::corpus::BugReport;
use crate::explorer::{
    AssertionDetail, AssertionStats, ExplorationReport, Explorer, ExplorerConfig,
};
use crate::report::format_campaign_report;
use log::info;
use serde::{Deserialize, Serialize};

use std::io::Write;

const BYTES_PER_MIB: usize = 1024 * 1024;
const MIB_PER_GIB: f64 = 1024.0;
pub const MAX_CAMPAIGN_SEEDS: usize = 1024;
const MAX_CAMPAIGN_PROGRESS_BYTES: u64 = 1024 * 1024;
const MAX_CAMPAIGN_PATH_BYTES: usize = 4096;
const MAX_CAMPAIGN_ERROR_BYTES: usize = 4096;

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
#[serde(deny_unknown_fields)]
pub struct SeedSummary {
    pub seed: u64,
    pub rounds: u64,
    pub total_branches: u64,
    pub total_edges: usize,
    pub bugs_found: usize,
    pub wall_clock_seconds: f64,
    /// Materialized phase summary for this seed (if a scenario was used).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario_summary: Option<chaoscontrol_fault::scenario::PhaseSummary>,
}

/// A bug deduplicated across seeds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignBug {
    /// The underlying bug report (from whichever seed found it first).
    pub bug: SerializableBug,
    /// Seeds that triggered this bug.
    pub found_by_seeds: Vec<u64>,
    /// First seed to find it.
    pub first_seed: u64,
    /// Dedup key: hash(assertion fingerprint, sorted fault type names).
    pub dedup_key: u64,
}

/// Aggregated report across all seeds in a campaign.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Merged assertion details with checked counts and kind-derived verdicts.
    pub assertion_details: Vec<AssertionDetail>,
    /// Merged assertion stats.
    pub assertion_stats: AssertionStats,
    /// Fatal identity conflicts found during report aggregation.
    #[serde(default)]
    pub assertion_identity_conflicts: Vec<String>,
    /// Catalog authority derived from all source reports and merged details.
    #[serde(default = "pending_catalog_status")]
    pub assertion_catalog_status: chaoscontrol_protocol::admission::CatalogValidationStatus,
    /// True only when every source and merged detail is accepted and strict.
    #[serde(default)]
    pub collision_safe_assertion_evidence: bool,
    /// Wall-clock time for the entire campaign.
    pub wall_clock_seconds: f64,
    /// Seeds that panicked or returned errors.
    #[serde(default)]
    pub failed_seeds: Vec<(u64, String)>,
    /// Helical scenario config used across all seeds (if any).
    #[serde(default)]
    pub scenario_config: Option<chaoscontrol_fault::scenario::ScenarioConfig>,
}

fn pending_catalog_status() -> chaoscontrol_protocol::admission::CatalogValidationStatus {
    chaoscontrol_protocol::admission::CatalogValidationStatus::Pending
}

pub fn campaign_bugs_for_minimization(
    report: &CampaignReport,
) -> Result<Vec<BugReport>, BugSetIdentityError> {
    validate_campaign_bug_carriers(&report.bugs)?;
    let Some(first_bug) = report.bugs.first() else {
        return Ok(Vec::new());
    };
    let summary =
        crate::assertion_summary::AssertionSummaryV2::from_campaign(report).map_err(|_| {
            BugSetIdentityError {
                bug_id: first_bug.bug.bug_id,
                source: crate::bug::identity::BugIdentityError::ReportMismatch,
            }
        })?;
    for campaign_bug in &report.bugs {
        let identity = campaign_bug
            .bug
            .require_replay_identity()
            .map_err(|source| BugSetIdentityError {
                bug_id: campaign_bug.bug.bug_id,
                source,
            })?;
        let exact_matches = summary
            .assertions()
            .iter()
            .filter(|detail| crate::bug::identity::detail_matches_identity(detail, identity))
            .count();
        if exact_matches != 1 {
            return Err(BugSetIdentityError {
                bug_id: campaign_bug.bug.bug_id,
                source: crate::bug::identity::BugIdentityError::ReportMismatch,
            });
        }
    }
    report
        .bugs
        .iter()
        .map(|campaign_bug| {
            let mut bug =
                BugReport::try_from(&campaign_bug.bug).map_err(|source| BugSetIdentityError {
                    bug_id: campaign_bug.bug.bug_id,
                    source,
                })?;
            bug.dedup_key = campaign_bug.dedup_key;
            Ok(bug)
        })
        .collect()
}

fn validate_campaign_bug_carriers(bugs: &[CampaignBug]) -> Result<(), BugSetIdentityError> {
    for campaign_bug in bugs {
        BugReport::try_from(&campaign_bug.bug).map_err(|source| BugSetIdentityError {
            bug_id: campaign_bug.bug.bug_id,
            source,
        })?;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
//  Campaign checkpoint / resume
// ═══════════════════════════════════════════════════════════════════════

/// Serializable subset of ExplorerConfig for campaign checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Helical scenario config (if the campaign used one).
    #[serde(default)]
    pub scenario: Option<chaoscontrol_fault::scenario::ScenarioConfig>,
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
            scenario: cfg.scenario.clone(),
        }
    }
}

/// Incremental campaign checkpoint — updated after each seed completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignProgress {
    /// All seeds in the campaign.
    pub seeds: Vec<u64>,
    /// Serializable base config.
    pub config: SerializableCampaignConfig,
    /// Output directory.
    pub output_dir: String,
    /// Completed seeds and their summaries.
    pub completed: std::collections::BTreeMap<u64, SeedSummary>,
    /// Seeds that failed (panicked or returned error) with error messages.
    #[serde(default)]
    pub failed: std::collections::BTreeMap<u64, String>,
}

fn validate_campaign_progress(progress: &CampaignProgress, output_dir: &str) -> Result<(), String> {
    validate_unique_seeds(&progress.seeds).map_err(str::to_string)?;
    if progress.seeds.is_empty()
        || progress.seeds.len() > MAX_CAMPAIGN_SEEDS
        || progress.output_dir != output_dir
        || progress.output_dir.is_empty()
        || progress.output_dir.len() > MAX_CAMPAIGN_PATH_BYTES
    {
        return Err("campaign progress output or seed bounds are invalid".to_string());
    }
    let config = &progress.config;
    if config.kernel_path.is_empty()
        || config.kernel_path.len() > MAX_CAMPAIGN_PATH_BYTES
        || config.num_vms == 0
        || config.branch_factor == 0
        || config.ticks_per_branch == 0
        || config.max_rounds == 0
        || config.quantum == 0
        || config.num_vcpus == 0
        || !matches!(
            config.exploration_mode.as_str(),
            "fault-schedule" | "input-tree" | "hybrid"
        )
    {
        return Err("campaign progress configuration is invalid".to_string());
    }
    for optional_path in [&config.initrd_path, &config.disk_image_path] {
        if optional_path
            .as_ref()
            .is_some_and(|path| path.is_empty() || path.len() > MAX_CAMPAIGN_PATH_BYTES)
        {
            return Err("campaign progress path is invalid".to_string());
        }
    }
    if config.scenario.as_ref().is_some_and(|scenario| {
        scenario.num_vms != config.num_vms || scenario.phase_ticks == 0 || scenario.turns == 0
    }) {
        return Err("campaign progress scenario is invalid".to_string());
    }
    let seeds = progress
        .seeds
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    for (seed, summary) in &progress.completed {
        if !seeds.contains(seed) || summary.seed != *seed || progress.failed.contains_key(seed) {
            return Err("campaign progress completed seed is invalid".to_string());
        }
    }
    for (seed, error) in &progress.failed {
        if !seeds.contains(seed) || error.is_empty() || error.len() > MAX_CAMPAIGN_ERROR_BYTES {
            return Err("campaign progress failed seed is invalid".to_string());
        }
    }
    Ok(())
}

/// Save campaign progress to `{output_dir}/campaign_progress.json`.
pub fn save_campaign_progress(
    progress: &CampaignProgress,
    output_dir: &str,
) -> Result<(), std::io::Error> {
    validate_campaign_progress(progress, output_dir).map_err(std::io::Error::other)?;
    let bytes = serde_json::to_vec_pretty(progress).map_err(std::io::Error::other)?;
    if bytes.len() as u64 > MAX_CAMPAIGN_PROGRESS_BYTES {
        return Err(std::io::Error::other(
            "campaign progress exceeds the byte limit",
        ));
    }
    let output_dir = std::path::Path::new(output_dir);
    let path = output_dir.join("campaign_progress.json");
    let mut temporary = tempfile::NamedTempFile::new_in(output_dir)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(&path).map_err(|error| error.error)?;
    std::fs::File::open(output_dir)?.sync_all()?;
    info!("Saved campaign progress: {}", path.display());
    Ok(())
}

/// Load campaign progress from `{dir}/campaign_progress.json`.
pub fn load_campaign_progress(dir: &str) -> Result<CampaignProgress, std::io::Error> {
    let path = std::path::Path::new(dir).join("campaign_progress.json");
    let json = crate::bounded_json::read_bounded_json(&path, MAX_CAMPAIGN_PROGRESS_BYTES)?;
    let progress: CampaignProgress = serde_json::from_str(&json).map_err(std::io::Error::other)?;
    validate_campaign_progress(&progress, dir).map_err(std::io::Error::other)?;
    Ok(progress)
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

fn validate_unique_seeds(seeds: &[u64]) -> Result<(), &'static str> {
    let mut seen = std::collections::BTreeSet::new();
    for seed in seeds {
        if !seen.insert(*seed) {
            return Err("campaign seeds must be unique");
        }
    }
    Ok(())
}

fn checked_campaign_memory_mb(
    num_seeds: usize,
    num_vms: usize,
    vm_memory_mb: usize,
) -> Option<usize> {
    num_seeds
        .checked_mul(num_vms)
        .and_then(|value| value.checked_mul(vm_memory_mb))
}

impl CampaignRunner {
    pub fn new(config: CampaignConfig) -> Self {
        Self { config }
    }

    /// Run all seeds in parallel, return the aggregated report.
    pub fn run(&self) -> Result<CampaignReport, crate::explorer::ExploreError> {
        validate_unique_seeds(&self.config.seeds).map_err(|message| {
            crate::explorer::ExploreError::Config {
                message: message.to_string(),
            }
        })?;
        let num_seeds = self.config.seeds.len();
        let num_vms = self.config.base_explorer_config.num_vms;
        let vm_memory_mb = self.config.base_explorer_config.vm_config.memory_size / BYTES_PER_MIB;
        let estimated_mb = checked_campaign_memory_mb(num_seeds, num_vms, vm_memory_mb)
            .ok_or_else(|| crate::explorer::ExploreError::Config {
                message: "campaign memory estimate overflow".to_string(),
            })?;
        info!(
            "Campaign: {} seeds × {} VMs × {} MB = ~{:.1} GB estimated memory",
            num_seeds,
            num_vms,
            vm_memory_mb,
            estimated_mb as f64 / MIB_PER_GIB,
        );
        info!(
            "Launching {} seed{} in parallel...",
            num_seeds,
            if num_seeds == 1 { "" } else { "s" }
        );

        let campaign_start = std::time::Instant::now();

        // Reserve the output path before any seed can mutate a guest.
        std::fs::create_dir_all(&self.config.output_dir).map_err(|error| {
            crate::explorer::ExploreError::Config {
                message: format!("cannot create campaign output directory: {error}"),
            }
        })?;

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
                            let thread_start = std::time::Instant::now();
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
            completed: std::collections::BTreeMap::new(),
            failed: std::collections::BTreeMap::new(),
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
                            scenario_summary: report.scenario_summary.clone(),
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

/// Generate a bounded unique seed list.
pub fn generate_seeds(
    base_seed: u64,
    count: usize,
    explicit: Option<&[u64]>,
) -> Result<Vec<u64>, &'static str> {
    let requested_count = explicit.map_or(count, <[u64]>::len);
    if requested_count == 0 || requested_count > MAX_CAMPAIGN_SEEDS {
        return Err("campaign seed count is outside the supported range");
    }
    let seeds = if let Some(seeds) = explicit {
        seeds.to_vec()
    } else {
        let count = u64::try_from(count).map_err(|_| "campaign seed count does not fit u64")?;
        (0..count)
            .map(|offset| {
                base_seed
                    .checked_add(offset)
                    .ok_or("campaign seed sequence overflow")
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    validate_unique_seeds(&seeds)?;
    Ok(seeds)
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
    let mut bug_map: std::collections::BTreeMap<u64, CampaignBug> =
        std::collections::BTreeMap::new();

    // Assertion merge: structured fingerprint or explicit legacy quarantine key.
    let mut assertion_map: std::collections::BTreeMap<String, AssertionDetail> =
        std::collections::BTreeMap::new();
    let mut assertion_identity_conflicts = Vec::new();
    let mut rejected_assertion_keys = std::collections::BTreeSet::new();
    let mut all_sources_accepted = !seed_reports.is_empty();
    let mut source_fatal = false;
    let mut seen_seeds = std::collections::BTreeSet::new();

    for (seed, report, elapsed) in &seed_reports {
        if !seen_seeds.insert(*seed) {
            source_fatal = true;
            all_sources_accepted = false;
            assertion_identity_conflicts.push(format!("duplicate campaign seed: {seed}"));
            continue;
        }
        seeds_run.push(*seed);
        match total_rounds.checked_add(report.rounds) {
            Some(value) => total_rounds = value,
            None => {
                source_fatal = true;
                assertion_identity_conflicts.push("campaign round count overflow".to_string());
            }
        }
        match total_branches.checked_add(report.total_branches) {
            Some(value) => total_branches = value,
            None => {
                source_fatal = true;
                assertion_identity_conflicts.push("campaign branch count overflow".to_string());
            }
        }
        let source_assertion_status =
            crate::assertion_summary::validate_assertion_details(&report.assertion_details);
        let source_assertions_valid = source_assertion_status.is_ok();
        if let Err(error) = &source_assertion_status {
            source_fatal = true;
            assertion_identity_conflicts.push(format!("seed {seed}: {error}"));
        }
        source_fatal |= report.assertion_catalog_status
            == chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict;
        all_sources_accepted &= source_assertion_status
            == Ok(chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted)
            && report.assertion_catalog_status
                == chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted
            && report.collision_safe_assertion_evidence;
        assertion_identity_conflicts.extend(
            report
                .assertion_identity_conflicts
                .iter()
                .map(|error| format!("seed {seed}: {error}")),
        );

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
            scenario_summary: report.scenario_summary.clone(),
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

        if !source_assertions_valid {
            continue;
        }
        for detail in &report.assertion_details {
            let key = assertion_detail_key(detail);
            if rejected_assertion_keys.contains(&key) {
                continue;
            }
            if let Some(existing) = assertion_map.get_mut(&key) {
                if let Err(error) = merge_assertion_detail(existing, detail) {
                    assertion_map.remove(&key);
                    rejected_assertion_keys.insert(key.clone());
                    assertion_identity_conflicts.push(format!("{key}: {error}"));
                }
            } else {
                assertion_map.insert(key, detail.clone());
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

    // Take scenario config from first report (shared across all seeds).
    let scenario_config = seed_reports
        .first()
        .and_then(|(_, r, _)| r.scenario_config.clone());

    let recomputed_status =
        crate::assertion_summary::validate_assertion_details(&assertion_details)
            .unwrap_or(chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict);
    let assertion_catalog_status = if source_fatal
        || !assertion_identity_conflicts.is_empty()
        || recomputed_status
            == chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict
    {
        chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict
    } else if recomputed_status
        == chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted
    {
        if all_sources_accepted {
            chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted
        } else {
            chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict
        }
    } else {
        chaoscontrol_protocol::admission::CatalogValidationStatus::LegacyAmbiguous
    };
    let collision_safe_assertion_evidence = assertion_catalog_status
        == chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted;

    CampaignReport {
        seeds_run,
        seeds_with_bugs,
        total_rounds,
        total_branches,
        bugs,
        per_seed,
        assertion_details,
        assertion_stats,
        assertion_identity_conflicts,
        assertion_catalog_status,
        collision_safe_assertion_evidence,
        wall_clock_seconds,
        failed_seeds: Vec::new(),
        scenario_config,
    }
}

fn assertion_detail_key(detail: &AssertionDetail) -> String {
    detail.identity.as_ref().map_or_else(
        || format!("legacy-ambiguous:{:08x}", detail.id),
        |identity| identity.fingerprint.to_hex(),
    )
}

fn merge_assertion_detail(
    existing: &mut AssertionDetail,
    candidate: &AssertionDetail,
) -> Result<(), &'static str> {
    let existing_verdict = crate::assertion_summary::derive_detail_verdict(existing)
        .map_err(|_| "invalid existing assertion counters")?;
    let candidate_verdict = crate::assertion_summary::derive_detail_verdict(candidate)
        .map_err(|_| "invalid candidate assertion counters")?;
    if existing.verdict != existing_verdict || candidate.verdict != candidate_verdict {
        return Err("assertion verdict conflicts with counters");
    }
    if existing.identity != candidate.identity
        || existing.id != candidate.id
        || existing.message != candidate.message
        || existing.kind != candidate.kind
        || existing.guest != candidate.guest
        || existing.category != candidate.category
    {
        return Err("descriptor metadata conflict");
    }
    let hit_count = existing
        .hit_count
        .checked_add(candidate.hit_count)
        .ok_or("hit count overflow")?;
    let true_count = existing
        .true_count
        .checked_add(candidate.true_count)
        .ok_or("true count overflow")?;
    let false_count = existing
        .false_count
        .checked_add(candidate.false_count)
        .ok_or("false count overflow")?;
    let mut merged = existing.clone();
    merged.hit_count = hit_count;
    merged.true_count = true_count;
    merged.false_count = false_count;
    merged.verdict = crate::assertion_summary::derive_detail_verdict(&merged)
        .map_err(|_| "invalid merged assertion counters")?
        .to_string();
    if candidate.last_failure_details.is_some() {
        merged.last_failure_details = candidate.last_failure_details.clone();
    }
    *existing = merged;
    Ok(())
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

/// Display rank used only to sort failed and unexercised assertions first.
fn verdict_rank(verdict: &str) -> u8 {
    match verdict {
        "failed" => 0,
        "unexercised" => 1,
        _ => 2, // "passed"
    }
}

/// Format the memory estimate string.
pub fn format_memory_estimate(
    num_seeds: usize,
    num_vms: usize,
    vm_memory_mb: usize,
) -> Result<String, &'static str> {
    let total_mb = checked_campaign_memory_mb(num_seeds, num_vms, vm_memory_mb)
        .ok_or("campaign memory estimate overflow")?;
    Ok(format!(
        "Estimated memory: {:.1} GB ({} seeds × {} VMs × {} MB)",
        total_mb as f64 / MIB_PER_GIB,
        num_seeds,
        num_vms,
        vm_memory_mb,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::SerializableSchedule;
    use crate::corpus::BugReport;
    use crate::coverage::CoverageStats;
    use crate::explorer::AssertionIdentityDetail;
    use chaoscontrol_fault::schedule::FaultSchedule;
    use chaoscontrol_protocol::admission::{token_for_descriptors, CatalogValidationStatus};
    use chaoscontrol_protocol::identity::{
        AssertionDescriptor, AssertionKind, AssertionLogicalKey, ASSERTION_IDENTITY_VERSION,
    };

    const FIRST_TEST_SEED: u64 = 1;
    const SECOND_TEST_SEED: u64 = 2;
    const TEST_COMPATIBILITY_ID: u32 = 100;
    const SINGLE_REPORT_SECONDS: f64 = 1.0;
    const TWO_REPORT_SECONDS: f64 = 2.0;

    // ── 6.1: seed generation ────────────────────────────────────────

    #[test]
    fn seed_generation_default_sequence() {
        let seeds = generate_seeds(42, 5, None).expect("seed sequence");
        assert_eq!(seeds, vec![42, 43, 44, 45, 46]);
    }

    #[test]
    fn seed_generation_explicit_list() {
        let explicit = vec![10, 99, 200];
        let seeds = generate_seeds(42, 5, Some(&explicit)).expect("explicit seeds");
        assert_eq!(seeds, vec![10, 99, 200]);
    }

    #[test]
    fn seed_generation_single() {
        let seeds = generate_seeds(0, 1, None).expect("single seed");
        assert_eq!(seeds, vec![0]);
        assert!(generate_seeds(0, 1, Some(&[1, 1])).is_err());
        assert!(generate_seeds(u64::MAX, MAX_CAMPAIGN_SEEDS, None).is_err());
    }

    // ── 6.2: bug dedup across seeds ─────────────────────────────────

    fn make_bug(id: u64, assertion_id: u64, location: &str, dedup_key: u64) -> BugReport {
        BugReport {
            bug_id: id,
            assertion_id,
            assertion_identity: crate::test_support::assertion_identity(assertion_id),
            fallback_scope: None,
            assertion_location: location.to_string(),
            schedule: FaultSchedule::new(),
            snapshot: None,
            tick: 1000,
            replay_parent_depth: 0,
            replay_parent_snapshot_ref: None,
            dedup_key,
            schedule_variant: None,
            scenario_config: None,
            scenario_summary: None,
        }
    }

    fn make_report(bugs: Vec<BugReport>, details: Vec<AssertionDetail>) -> ExplorationReport {
        let collision_safe =
            !details.is_empty() && details.iter().all(|detail| detail.identity.is_some());
        let status = if collision_safe {
            chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted
        } else {
            chaoscontrol_protocol::admission::CatalogValidationStatus::LegacyAmbiguous
        };
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
            assertion_catalog_status: status,
            collision_safe_assertion_evidence: collision_safe,
            assertion_identity_conflicts: Vec::new(),
            round_history: Vec::new(),
            wall_clock_seconds: 0.0,
            branches_per_second: 0.0,
            edges_per_second: 0.0,
            scenario_config: None,
            scenario_summary: None,
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

    fn bug_for_detail(bug_id: u64, detail: &AssertionDetail) -> BugReport {
        let detail_identity = detail.identity.as_ref().expect("strict detail identity");
        BugReport {
            bug_id,
            assertion_id: u64::from(detail.id),
            assertion_identity: chaoscontrol_protocol::admission::AssertionEvidenceIdentity {
                descriptor: detail_identity.descriptor.clone(),
                fingerprint: detail_identity.fingerprint,
                canonical_descriptor: detail_identity
                    .descriptor
                    .canonical_bytes()
                    .expect("canonical descriptor"),
                catalog_token: detail_identity.catalog_tokens[0],
            },
            fallback_scope: None,
            assertion_location: "src/main.rs:10".to_string(),
            schedule: FaultSchedule::new(),
            snapshot: None,
            tick: 1,
            replay_parent_depth: 0,
            replay_parent_snapshot_ref: None,
            dedup_key: 1,
            schedule_variant: None,
            scenario_config: None,
            scenario_summary: None,
        }
    }

    #[test]
    fn minimization_accepts_bug_joined_to_campaign_catalog() {
        let detail = make_strict_detail(AssertionKind::Always, "failed", 1, 0, 1);
        let bug = bug_for_detail(0, &detail);
        let report = aggregate_reports(
            vec![(
                FIRST_TEST_SEED,
                make_report(vec![bug], vec![detail]),
                SINGLE_REPORT_SECONDS,
            )],
            SINGLE_REPORT_SECONDS,
        );

        let bugs = campaign_bugs_for_minimization(&report).expect("catalog-bound bug is valid");
        assert_eq!(bugs.len(), 1);
    }

    #[test]
    fn minimization_rejects_catalog_token_substitution() {
        let detail = make_strict_detail(AssertionKind::Always, "failed", 1, 0, 1);
        let bug = bug_for_detail(0, &detail);
        let mut report = aggregate_reports(
            vec![(
                FIRST_TEST_SEED,
                make_report(vec![bug], vec![detail]),
                SINGLE_REPORT_SECONDS,
            )],
            SINGLE_REPORT_SECONDS,
        );
        report.bugs[0]
            .bug
            .assertion_identity
            .as_mut()
            .expect("identity")
            .catalog_token = chaoscontrol_protocol::identity::AssertionFingerprint::ZERO;

        let error = campaign_bugs_for_minimization(&report)
            .expect_err("catalog token substitution is rejected");
        assert_eq!(
            error.source,
            crate::bug::identity::BugIdentityError::ReportMismatch
        );
    }

    #[test]
    fn minimization_rejects_mixed_valid_and_legacy_bug_set() {
        let valid = make_bug(0, 100, "safety.rs:10", 0xAAAA);
        let legacy = make_bug(1, 200, "safety.rs:20", 0xBBBB);
        let mut report = aggregate_reports(
            vec![
                (
                    FIRST_TEST_SEED,
                    make_report(vec![valid], Vec::new()),
                    SINGLE_REPORT_SECONDS,
                ),
                (
                    SECOND_TEST_SEED,
                    make_report(vec![legacy], Vec::new()),
                    SINGLE_REPORT_SECONDS,
                ),
            ],
            TWO_REPORT_SECONDS,
        );
        report.bugs[1].bug.assertion_identity = None;

        let error = campaign_bugs_for_minimization(&report)
            .expect_err("one legacy bug rejects the complete carrier");
        assert_eq!(error.bug_id, 1);
    }

    // ── 6.3: assertion merging ──────────────────────────────────────

    fn make_detail(id: u32, verdict: &str, hits: u64, t: u64, f: u64) -> AssertionDetail {
        AssertionDetail {
            id,
            identity: None,
            message: format!("assertion_{}", id),
            kind: "always".to_string(),
            guest: "uncategorized".to_string(),
            category: "uncategorized".to_string(),
            verdict: verdict.to_string(),
            hit_count: hits,
            true_count: t,
            false_count: f,
            last_failure_details: None,
        }
    }

    fn make_strict_detail(
        kind: AssertionKind,
        verdict: &str,
        hits: u64,
        true_count: u64,
        false_count: u64,
    ) -> AssertionDetail {
        const SOURCE_LINE: u32 = 10;
        const SOURCE_COLUMN: u32 = 4;
        let descriptor = AssertionDescriptor {
            identity_version: ASSERTION_IDENTITY_VERSION,
            namespace: "org.example.campaign".to_string(),
            logical_key: AssertionLogicalKey::Stable {
                key: format!("{kind:?}").to_lowercase(),
            },
            compatibility_id: Some(TEST_COMPATIBILITY_ID),
            kind,
            message: "campaign assertion".to_string(),
            source_file: "src/main.rs".to_string(),
            source_line: SOURCE_LINE,
            source_column: SOURCE_COLUMN,
            guest: "guest".to_string(),
            category: "invariant".to_string(),
        };
        let fingerprint = descriptor.fingerprint().expect("fingerprint");
        let canonical = descriptor.canonical_bytes().expect("canonical descriptor");
        let token = token_for_descriptors(std::slice::from_ref(&descriptor)).expect("token");
        AssertionDetail {
            id: TEST_COMPATIBILITY_ID,
            identity: Some(AssertionIdentityDetail {
                descriptor,
                fingerprint,
                canonical_descriptor: encode_test_hex(&canonical),
                catalog_tokens: vec![token],
            }),
            message: "campaign assertion".to_string(),
            kind: format!("{kind:?}").to_lowercase(),
            guest: "guest".to_string(),
            category: "invariant".to_string(),
            verdict: verdict.to_string(),
            hit_count: hits,
            true_count,
            false_count,
            last_failure_details: None,
        }
    }

    fn encode_test_hex(bytes: &[u8]) -> String {
        chaoscontrol_protocol::identity::encode_lower_hex(bytes)
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
    fn always_merge_derives_failure_from_aggregate_counts() {
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

    #[test]
    fn campaign_authority_rejects_empty_legacy_and_conflicted_sources() {
        let empty = aggregate_reports(
            vec![(
                FIRST_TEST_SEED,
                make_report(Vec::new(), Vec::new()),
                SINGLE_REPORT_SECONDS,
            )],
            SINGLE_REPORT_SECONDS,
        );
        assert!(!empty.collision_safe_assertion_evidence);
        assert_eq!(
            empty.assertion_catalog_status,
            CatalogValidationStatus::LegacyAmbiguous
        );

        let legacy = aggregate_reports(
            vec![(
                1,
                make_report(Vec::new(), vec![make_detail(100, "passed", 1, 1, 0)]),
                SINGLE_REPORT_SECONDS,
            )],
            SINGLE_REPORT_SECONDS,
        );
        assert_eq!(
            legacy.assertion_catalog_status,
            CatalogValidationStatus::LegacyAmbiguous
        );

        let mut conflicted = make_report(
            Vec::new(),
            vec![make_strict_detail(AssertionKind::Always, "passed", 1, 1, 0)],
        );
        conflicted.assertion_catalog_status = CatalogValidationStatus::FatalConflict;
        conflicted.collision_safe_assertion_evidence = false;
        conflicted
            .assertion_identity_conflicts
            .push("forged source".to_string());
        let campaign = aggregate_reports(
            vec![(FIRST_TEST_SEED, conflicted, SINGLE_REPORT_SECONDS)],
            SINGLE_REPORT_SECONDS,
        );
        assert_eq!(
            campaign.assertion_catalog_status,
            CatalogValidationStatus::FatalConflict
        );
        assert!(!campaign.collision_safe_assertion_evidence);

        let strict = make_report(
            Vec::new(),
            vec![make_strict_detail(AssertionKind::Always, "passed", 1, 1, 0)],
        );
        let empty_source = make_report(Vec::new(), Vec::new());
        let mixed_sources = aggregate_reports(
            vec![
                (FIRST_TEST_SEED, strict, SINGLE_REPORT_SECONDS),
                (SECOND_TEST_SEED, empty_source, SINGLE_REPORT_SECONDS),
            ],
            TWO_REPORT_SECONDS,
        );
        assert_eq!(
            mixed_sources.assertion_catalog_status,
            CatalogValidationStatus::FatalConflict
        );
    }

    #[test]
    fn duplicate_seed_and_counter_overflow_are_fatal() {
        let detail = make_strict_detail(AssertionKind::Always, "passed", 1, 1, 0);
        let duplicate = aggregate_reports(
            vec![
                (
                    FIRST_TEST_SEED,
                    make_report(Vec::new(), vec![detail.clone()]),
                    SINGLE_REPORT_SECONDS,
                ),
                (
                    FIRST_TEST_SEED,
                    make_report(Vec::new(), vec![detail.clone()]),
                    SINGLE_REPORT_SECONDS,
                ),
            ],
            TWO_REPORT_SECONDS,
        );
        assert_eq!(
            duplicate.assertion_catalog_status,
            CatalogValidationStatus::FatalConflict
        );
        assert_eq!(duplicate.seeds_run, vec![FIRST_TEST_SEED]);

        let mut first = make_report(Vec::new(), vec![detail.clone()]);
        first.rounds = u64::MAX;
        let second = make_report(Vec::new(), vec![detail]);
        let overflow = aggregate_reports(
            vec![
                (FIRST_TEST_SEED, first, SINGLE_REPORT_SECONDS),
                (SECOND_TEST_SEED, second, SINGLE_REPORT_SECONDS),
            ],
            TWO_REPORT_SECONDS,
        );
        assert_eq!(
            overflow.assertion_catalog_status,
            CatalogValidationStatus::FatalConflict
        );
        assert!(!overflow.collision_safe_assertion_evidence);
    }

    #[test]
    fn aggregate_verdicts_use_exact_kind_semantics_across_seeds() {
        for (kind, first, second) in [
            (
                AssertionKind::Sometimes,
                make_strict_detail(AssertionKind::Sometimes, "failed", 1, 0, 1),
                make_strict_detail(AssertionKind::Sometimes, "passed", 1, 1, 0),
            ),
            (
                AssertionKind::Reachable,
                make_strict_detail(AssertionKind::Reachable, "unexercised", 0, 0, 0),
                make_strict_detail(AssertionKind::Reachable, "passed", 1, 1, 0),
            ),
        ] {
            let reports = vec![
                (
                    FIRST_TEST_SEED,
                    make_report(Vec::new(), vec![first]),
                    SINGLE_REPORT_SECONDS,
                ),
                (
                    SECOND_TEST_SEED,
                    make_report(Vec::new(), vec![second]),
                    SINGLE_REPORT_SECONDS,
                ),
            ];
            let campaign = aggregate_reports(reports, TWO_REPORT_SECONDS);
            assert_eq!(
                campaign.assertion_details[0].kind,
                format!("{kind:?}").to_lowercase()
            );
            assert_eq!(campaign.assertion_details[0].verdict, "passed");
            assert_eq!(
                campaign.assertion_catalog_status,
                CatalogValidationStatus::Accepted
            );
        }
    }

    #[test]
    fn legacy_u32_identity_cannot_enter_an_accepted_campaign() {
        let mut detail = make_strict_detail(AssertionKind::Always, "passed", 1, 1, 0);
        let identity = detail.identity.as_mut().expect("strict identity");
        identity.descriptor.namespace = "legacy:campaign".to_string();
        identity.descriptor.logical_key = AssertionLogicalKey::LegacyU32 {
            id: TEST_COMPATIBILITY_ID,
        };
        identity.fingerprint = identity
            .descriptor
            .fingerprint()
            .expect("legacy fingerprint");
        identity.canonical_descriptor = chaoscontrol_protocol::identity::encode_lower_hex(
            &identity
                .descriptor
                .canonical_bytes()
                .expect("legacy canonical"),
        );
        identity.catalog_tokens = vec![identity.fingerprint];
        let report = make_report(Vec::new(), vec![detail]);

        let campaign = aggregate_reports(
            vec![(FIRST_TEST_SEED, report, SINGLE_REPORT_SECONDS)],
            SINGLE_REPORT_SECONDS,
        );
        assert_eq!(
            campaign.assertion_catalog_status,
            CatalogValidationStatus::FatalConflict
        );
        assert!(!campaign.collision_safe_assertion_evidence);
    }

    #[test]
    fn caller_verdict_spoofs_are_fatal_for_first_and_later_sources() {
        let spoof = make_strict_detail(AssertionKind::Always, "failed", 1, 1, 0);
        let first = aggregate_reports(
            vec![(
                FIRST_TEST_SEED,
                make_report(Vec::new(), vec![spoof.clone()]),
                SINGLE_REPORT_SECONDS,
            )],
            SINGLE_REPORT_SECONDS,
        );
        assert_eq!(
            first.assertion_catalog_status,
            CatalogValidationStatus::FatalConflict
        );

        let valid = make_strict_detail(AssertionKind::Always, "passed", 1, 1, 0);
        let later = aggregate_reports(
            vec![
                (
                    FIRST_TEST_SEED,
                    make_report(Vec::new(), vec![valid]),
                    SINGLE_REPORT_SECONDS,
                ),
                (
                    SECOND_TEST_SEED,
                    make_report(Vec::new(), vec![spoof]),
                    SINGLE_REPORT_SECONDS,
                ),
            ],
            TWO_REPORT_SECONDS,
        );
        assert_eq!(
            later.assertion_catalog_status,
            CatalogValidationStatus::FatalConflict
        );
        assert!(!later.collision_safe_assertion_evidence);
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
                    assertion_identity: Some(crate::test_support::assertion_identity(100)),
                    fallback_scope: None,
                    assertion_location: "test.rs:1".into(),
                    schedule: SerializableSchedule { faults: Vec::new() },
                    tick: 500,
                    replay_parent_depth: 0,
                    replay_parent_snapshot_ref: None,
                    dedup_key: Some(0xAAAA),
                    schedule_variant: None,
                    scenario_config: None,
                    scenario_summary: None,
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
                    scenario_summary: None,
                },
                SeedSummary {
                    seed: 43,
                    rounds: 10,
                    total_branches: 80,
                    total_edges: 80,
                    bugs_found: 0,
                    wall_clock_seconds: 21.1,
                    scenario_summary: None,
                },
            ],
            assertion_details: Vec::new(),
            assertion_stats: AssertionStats::default(),
            assertion_identity_conflicts: Vec::new(),
            assertion_catalog_status:
                chaoscontrol_protocol::admission::CatalogValidationStatus::LegacyAmbiguous,
            collision_safe_assertion_evidence: false,
            wall_clock_seconds: 25.0,
            failed_seeds: Vec::new(),
            scenario_config: None,
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
        let s = format_memory_estimate(5, 3, 256).expect("memory estimate");
        assert!(s.contains("3.8 GB"));
        assert!(s.contains("5 seeds"));
        assert!(s.contains("3 VMs"));
        assert!(s.contains("256 MB"));
    }

    #[test]
    fn memory_estimate_small() {
        let s = format_memory_estimate(1, 1, 256).expect("memory estimate");
        assert!(s.contains("0.3 GB") || s.contains("0.2 GB"));
        assert!(s.contains("1 seeds"));
        assert!(format_memory_estimate(usize::MAX, usize::MAX, 1).is_err());
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
                scenario: None,
            },
            output_dir: "results/".into(),
            completed: std::collections::BTreeMap::from([(
                42,
                SeedSummary {
                    seed: 42,
                    rounds: 10,
                    total_branches: 80,
                    total_edges: 256,
                    bugs_found: 1,
                    wall_clock_seconds: 23.0,
                    scenario_summary: None,
                },
            )]),
            failed: std::collections::BTreeMap::new(),
        };

        let json = serde_json::to_string_pretty(&progress).unwrap();
        let loaded: CampaignProgress = serde_json::from_str(&json).unwrap();
        let mut unknown = serde_json::to_value(&progress).expect("campaign progress JSON");
        unknown["unreviewed_authority"] = serde_json::Value::Bool(true);

        assert!(serde_json::from_value::<CampaignProgress>(unknown).is_err());
        assert_eq!(loaded.seeds, vec![42, 43, 44]);
        assert_eq!(loaded.completed.len(), 1);
        assert!(loaded.completed.contains_key(&42));
        assert_eq!(loaded.config.num_vms, 3);
        assert_eq!(loaded.config.exploration_mode, "hybrid");
    }

    #[test]
    fn campaign_progress_save_load() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let dir = temporary.path();

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
                scenario: None,
            },
            output_dir: dir.to_string_lossy().into_owned(),
            completed: std::collections::BTreeMap::new(),
            failed: std::collections::BTreeMap::new(),
        };

        let dir_text = dir.to_string_lossy();
        save_campaign_progress(&progress, &dir_text).unwrap();
        let loaded = load_campaign_progress(&dir_text).unwrap();
        let path = dir.join("campaign_progress.json");
        let before = std::fs::read(&path).expect("campaign progress bytes");
        let mut duplicate = progress.clone();
        duplicate.seeds = vec![10, 10];

        assert!(save_campaign_progress(&duplicate, &dir_text).is_err());
        assert_eq!(std::fs::read(&path).expect("retained progress"), before);
        assert_eq!(loaded.seeds, vec![10, 20]);
        assert!(loaded.completed.is_empty());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let target = dir.join("retained-progress.json");
            std::fs::rename(&path, &target).expect("move progress fixture");
            symlink(&target, &path).expect("progress symlink");
            assert!(load_campaign_progress(&dir_text).is_err());
        }
    }
}
