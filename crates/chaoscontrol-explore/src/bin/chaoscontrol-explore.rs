//! CLI binary for ChaosControl exploration engine.
//!
//! Runs coverage-guided exploration campaigns to discover bugs in distributed
//! systems running under ChaosControl VMM.
//!
//! # Usage
//!
//! ```bash
//! # Run exploration campaign
//! chaoscontrol-explore run --kernel vmlinux --initrd initrd.gz
//!
//! # Run with custom parameters
//! chaoscontrol-explore run --kernel vmlinux --vms 3 --rounds 200 --branches 16
//!
//! # Save results to directory (enables checkpointing)
//! chaoscontrol-explore run --kernel vmlinux --output results/
//!
//! # Resume from a previous session
//! chaoscontrol-explore resume --corpus results/
//!
//! # Resume with different kernel or more rounds
//! chaoscontrol-explore resume --corpus results/ --kernel vmlinux.new --rounds 500
//! ```
//!
//! # Checkpointing
//!
//! When an `--output` directory is specified for the `run` command, the explorer
//! automatically saves a checkpoint after each round to `{output}/checkpoint.json`.
//! This allows exploration campaigns to be interrupted and resumed later.
//!
//! The checkpoint contains:
//! - Configuration (VMs, seed, rounds, etc.)
//! - Global coverage bitmap (64KB)
//! - Bugs found so far
//! - Progress counters (rounds completed, branches run)
//!
//! Note: The frontier (VM snapshots) is NOT saved, as it contains complex KVM state.
//! On resume, we re-bootstrap the VMs but carry forward the global coverage map,
//! so we don't re-explore known territory.

use chaoscontrol_explore::campaign::{
    generate_seeds, load_campaign_progress, CampaignConfig, CampaignRunner,
};
use chaoscontrol_explore::checkpoint::{export_checkpoint_bugs, load_checkpoint};
use chaoscontrol_explore::corpus::BugReport;
use chaoscontrol_explore::explorer::{ExplorationMode, Explorer, ExplorerConfig};
use chaoscontrol_explore::minimizer::{MinimizeConfig, Minimizer};
use chaoscontrol_explore::mutator::MutationConfig;
use chaoscontrol_explore::report::{
    format_campaign_report, format_report, min_assertion_exercise_failures,
};
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_protocol::COVERAGE_BITMAP_ADDR;
use chaoscontrol_vmm::scheduler::SchedulingStrategy;
use chaoscontrol_vmm::vm::VmConfig;
use clap::{Parser, Subcommand};
use std::fs;
use std::path::Path;

#[derive(Parser)]
#[command(name = "chaoscontrol-explore")]
#[command(about = "Coverage-guided exploration for ChaosControl VMM")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run an exploration campaign.
    Run {
        /// Path to kernel (vmlinux or bzImage).
        #[arg(short, long)]
        kernel: String,

        /// Path to initrd (optional).
        #[arg(short, long)]
        initrd: Option<String>,

        /// Random seed for reproducibility.
        #[arg(short, long, default_value = "42")]
        seed: u64,

        /// Number of VMs per simulation.
        #[arg(short, long, default_value = "2")]
        vms: usize,

        /// Total exploration rounds.
        #[arg(short, long, default_value = "100")]
        rounds: u64,

        /// Branch factor (variants per round).
        #[arg(short, long, default_value = "8")]
        branches: usize,

        /// Ticks per branch (ticks_per_branch).
        #[arg(short, long, default_value = "1000")]
        ticks: u64,

        /// Scheduling quantum (exits per VM per round).
        #[arg(short, long, default_value = "100")]
        quantum: u64,

        /// Number of vCPUs per VM (1 = single-CPU, 2+ = SMP).
        #[arg(long, default_value = "1")]
        vcpus: usize,

        /// Scheduling strategy for SMP: "round-robin" or "randomized".
        #[arg(long, default_value = "round-robin")]
        scheduling: String,

        /// Max frontier size.
        #[arg(short, long, default_value = "50")]
        max_frontier: usize,

        /// Output directory for reports and bug artifacts.
        #[arg(short, long)]
        output: Option<String>,

        /// Path to a disk image file for the virtio-blk device.
        ///
        /// When provided, each VM's block device is loaded from this file.
        /// The file is read once; copy-on-write makes snapshots cheap.
        #[arg(long)]
        disk_image: Option<String>,

        /// Extra kernel command line parameters (appended to defaults).
        ///
        /// Example: --extra-cmdline "raft_bug=fig8"
        #[arg(long)]
        extra_cmdline: Option<String>,

        /// Exploration mode: "fault-schedule", "input-tree", or "hybrid".
        ///
        /// fault-schedule: mutate fault schedules (original mode).
        /// input-tree:     branch at random_choice() decision points.
        /// hybrid:         alternate between both strategies.
        #[arg(long, default_value = "fault-schedule")]
        mode: String,

        /// Bootstrap tick budget (kernel boot + guest init).
        /// Exploration waits for setup_complete or this limit.
        #[arg(long, default_value = "10000")]
        bootstrap_budget: u64,

        /// Directory for determinism log files.
        ///
        /// When set, each VM writes a binary .dlog file per run.
        /// Use `chaoscontrol-replay dlog diff` to compare two runs.
        #[arg(long)]
        dlog: Option<String>,

        /// Emit a full RegisterDump dlog record every N exits.
        /// 0 (default) disables register dumps.
        #[arg(long, default_value = "0")]
        dlog_register_interval: u64,

        /// Hash guest memory pages at snapshot boundaries and emit
        /// MemoryHash dlog records.
        #[arg(long)]
        dlog_memory_hash: bool,

        /// Number of parallel worker threads for branch execution.
        ///
        /// 1 (default): sequential, identical to previous behavior.
        /// N > 1: N workers run branches in parallel.
        /// 0: auto-detect based on available cores.
        #[arg(short = 'w', long, default_value = "1")]
        workers: usize,

        /// Rare-edge threshold: global hit count at or below which an edge is "rare".
        #[arg(long, default_value = "3")]
        rare_edge_threshold: u8,

        /// Score multiplier per rare edge in frontier prioritization.
        #[arg(long, default_value = "5.0")]
        rare_edge_weight: f64,

        /// Stale rounds before havoc mutations activate (0 = auto: stale_round_limit/2).
        #[arg(long, default_value = "0")]
        havoc_after_stale: u64,

        /// Havoc mutation count range (min,max).
        #[arg(long, default_value = "4,16", value_delimiter = ',')]
        havoc_mutations: Vec<u32>,

        /// Stop after N consecutive rounds with no new edges or bugs (0 = never).
        #[arg(long, default_value = "10")]
        stale_round_limit: u64,

        /// Refuse to start if estimated VM memory exceeds 80% of available RAM.
        #[arg(long)]
        strict_memory: bool,

        /// Guest memory size per VM in MiB.
        #[arg(long, default_value = "256")]
        memory_mb: usize,

        /// Run delta-debugging minimizer on each bug after exploration.
        #[arg(long)]
        auto_minimize: bool,

        /// Enable the live web dashboard.
        #[arg(long)]
        dashboard: bool,

        /// Dashboard port (default: 8080).
        #[arg(long, default_value = "8080")]
        dashboard_port: u16,

        /// Named helical scenario family. Generates a rotating multi-phase
        /// fault schedule instead of random mutations.
        ///
        /// Built-in families:
        ///   network-ring         Rotating partitions and restarts
        ///   volatile-write-ring  Rotating DiskFsyncLie + kill/restart cycles
        ///   degraded-io-ring     Rotating DiskSlow/DiskPartialRead + restarts
        #[arg(long)]
        scenario: Option<String>,

        /// Phase duration in virtual nanoseconds for helical scenarios.
        #[arg(long, default_value = "1000")]
        scenario_phase_ticks: u64,

        /// Number of helical turns (one turn = one target rotation).
        #[arg(long, default_value = "6")]
        scenario_turns: usize,

        /// Minimum exercised assertions required per guest/category group.
        /// Exits with status 3 after writing artifacts when any group is below the floor.
        #[arg(long, default_value = "0")]
        min_assertion_exercise: usize,

        /// Emit one JSON metrics line after each completed round.
        #[arg(long)]
        emit_metrics: bool,

        /// Write JSON metrics lines to this file instead of stderr.
        #[arg(long)]
        metrics_file: Option<String>,
    },

    /// Minimize a bug-triggering fault schedule.
    ///
    /// Takes a bug report from a previous exploration run and produces
    /// the smallest fault schedule that still triggers the same failure.
    Minimize {
        /// Path to kernel (vmlinux).
        #[arg(short, long)]
        kernel: String,

        /// Path to initrd (optional).
        #[arg(short, long)]
        initrd: Option<String>,

        /// Path to the bug report file (from exploration output).
        #[arg(short, long)]
        bug: String,

        /// Random seed (must match exploration run).
        #[arg(short, long, default_value = "42")]
        seed: u64,

        /// Number of VMs.
        #[arg(short, long, default_value = "2")]
        vms: usize,

        /// Ticks per branch.
        #[arg(short, long, default_value = "1000")]
        ticks: u64,

        /// Scheduling quantum.
        #[arg(short, long, default_value = "100")]
        quantum: u64,

        /// Number of vCPUs per VM.
        #[arg(long, default_value = "1")]
        vcpus: usize,

        /// Scheduling strategy: "round-robin" or "randomized".
        #[arg(long, default_value = "round-robin")]
        scheduling: String,

        /// Path to disk image (optional).
        #[arg(long)]
        disk_image: Option<String>,

        /// Extra kernel command line parameters.
        #[arg(long)]
        extra_cmdline: Option<String>,

        /// Bootstrap tick budget.
        #[arg(long, default_value = "10000")]
        bootstrap_budget: u64,

        /// Guest memory size per VM in MiB.
        #[arg(long, default_value = "256")]
        memory_mb: usize,

        /// Output file for minimized schedule.
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Reproduce a bug from a bug report file.
    ///
    /// Boots the simulation, applies the fault schedule from the bug report,
    /// runs for the configured ticks, and reports whether the assertion fails.
    /// Use this to verify a bug after minimization or on a different host.
    Reproduce {
        /// Path to kernel (vmlinux).
        #[arg(short, long)]
        kernel: String,

        /// Path to initrd (optional).
        #[arg(short, long)]
        initrd: Option<String>,

        /// Path to the bug report file (JSON).
        #[arg(short, long)]
        bug: String,

        /// Random seed (must match exploration run).
        #[arg(short, long, default_value = "42")]
        seed: u64,

        /// Number of VMs.
        #[arg(short, long, default_value = "2")]
        vms: usize,

        /// Ticks to run after bootstrap.
        #[arg(short, long, default_value = "1000")]
        ticks: u64,

        /// Scheduling quantum.
        #[arg(short, long, default_value = "100")]
        quantum: u64,

        /// Number of vCPUs per VM.
        #[arg(long, default_value = "1")]
        vcpus: usize,

        /// Scheduling strategy: "round-robin" or "randomized".
        #[arg(long, default_value = "round-robin")]
        scheduling: String,

        /// Path to disk image (optional).
        #[arg(long)]
        disk_image: Option<String>,

        /// Extra kernel command line parameters.
        #[arg(long)]
        extra_cmdline: Option<String>,

        /// Bootstrap tick budget.
        #[arg(long, default_value = "10000")]
        bootstrap_budget: u64,

        /// Guest memory size per VM in MiB.
        #[arg(long, default_value = "256")]
        memory_mb: usize,

        /// Show serial output from each VM.
        #[arg(long)]
        serial: bool,

        /// Write machine-readable replay verdict JSON to this path.
        #[arg(long)]
        verdict_output: Option<String>,
    },

    /// Run a multi-seed campaign.
    ///
    /// Launches N independent explorations with different seeds in parallel,
    /// then aggregates bugs, coverage, and assertion verdicts into a unified
    /// report. Each seed runs in its own thread with its own KVM VMs.
    Campaign {
        /// Path to kernel (vmlinux or bzImage).
        #[arg(short, long)]
        kernel: String,

        /// Path to initrd (optional).
        #[arg(short, long)]
        initrd: Option<String>,

        /// Base random seed. Seeds are base, base+1, ..., base+N-1.
        #[arg(short, long, default_value = "42")]
        seed: u64,

        /// Number of VMs per simulation.
        #[arg(short, long, default_value = "2")]
        vms: usize,

        /// Total exploration rounds per seed.
        #[arg(short, long, default_value = "100")]
        rounds: u64,

        /// Branch factor (variants per round).
        #[arg(short, long, default_value = "8")]
        branches: usize,

        /// Ticks per branch.
        #[arg(short, long, default_value = "1000")]
        ticks: u64,

        /// Scheduling quantum (exits per VM per round).
        #[arg(short, long, default_value = "100")]
        quantum: u64,

        /// Number of vCPUs per VM.
        #[arg(long, default_value = "1")]
        vcpus: usize,

        /// Scheduling strategy: "round-robin" or "randomized".
        #[arg(long, default_value = "round-robin")]
        scheduling: String,

        /// Max frontier size.
        #[arg(short, long, default_value = "50")]
        max_frontier: usize,

        /// Output directory for reports and per-seed artifacts (required).
        #[arg(short, long)]
        output: String,

        /// Path to a disk image file for the virtio-blk device.
        #[arg(long)]
        disk_image: Option<String>,

        /// Extra kernel command line parameters.
        #[arg(long)]
        extra_cmdline: Option<String>,

        /// Exploration mode: "fault-schedule", "input-tree", or "hybrid".
        #[arg(long, default_value = "fault-schedule")]
        mode: String,

        /// Bootstrap tick budget.
        #[arg(long, default_value = "10000")]
        bootstrap_budget: u64,

        /// Number of seeds to run in parallel.
        #[arg(long, default_value = "4")]
        campaign_seeds: usize,

        /// Explicit comma-separated seed list (overrides --seed + --campaign-seeds).
        #[arg(long, value_delimiter = ',')]
        seeds: Option<Vec<u64>>,

        /// Parallel workers per seed (legacy, use --workers-per-seed).
        #[arg(short = 'w', long, default_value = "1")]
        workers: usize,

        /// Workers per seed (0 = auto: cores / (seeds × VMs)).
        #[arg(long, default_value = "0")]
        workers_per_seed: usize,

        /// Rare-edge threshold: global hit count at or below which an edge is "rare".
        #[arg(long, default_value = "3")]
        rare_edge_threshold: u8,

        /// Score multiplier per rare edge in frontier prioritization.
        #[arg(long, default_value = "5.0")]
        rare_edge_weight: f64,

        /// Stale rounds before havoc mutations activate (0 = auto: stale_round_limit/2).
        #[arg(long, default_value = "0")]
        havoc_after_stale: u64,

        /// Havoc mutation count range (min,max).
        #[arg(long, default_value = "4,16", value_delimiter = ',')]
        havoc_mutations: Vec<u32>,

        /// Stop after N consecutive rounds with no new edges or bugs (0 = never).
        #[arg(long, default_value = "10")]
        stale_round_limit: u64,

        /// Refuse to start if estimated VM memory exceeds 80% of available RAM.
        #[arg(long)]
        strict_memory: bool,

        /// Run delta-debugging minimizer on each bug after campaign.
        #[arg(long)]
        auto_minimize: bool,

        /// Enable the live web dashboard.
        #[arg(long)]
        dashboard: bool,

        /// Dashboard port (default: 8080).
        #[arg(long, default_value = "8080")]
        dashboard_port: u16,

        /// Named helical scenario family.
        #[arg(long)]
        scenario: Option<String>,

        /// Phase duration in virtual nanoseconds for helical scenarios.
        #[arg(long, default_value = "1000")]
        scenario_phase_ticks: u64,

        /// Number of helical turns.
        #[arg(long, default_value = "6")]
        scenario_turns: usize,

        /// Minimum exercised assertions required per guest/category group.
        /// Exits with status 3 after writing artifacts when any group is below the floor.
        #[arg(long, default_value = "0")]
        min_assertion_exercise: usize,
    },

    /// Resume a multi-seed campaign from checkpoint.
    ///
    /// Reads campaign_progress.json, skips completed seeds,
    /// runs remaining seeds, then aggregates a final report.
    CampaignResume {
        /// Path to campaign output directory (containing campaign_progress.json).
        #[arg(short, long)]
        corpus: String,

        /// Override max rounds per seed.
        #[arg(short, long)]
        rounds: Option<u64>,
    },

    /// Resume from saved checkpoint.
    Resume {
        /// Path to corpus directory (containing checkpoint.json).
        #[arg(short, long)]
        corpus: String,

        /// Override kernel path (if different from checkpoint).
        #[arg(short, long)]
        kernel: Option<String>,

        /// Override initrd path (if different from checkpoint).
        #[arg(short, long)]
        initrd: Option<String>,

        /// Override max rounds (continue for more rounds).
        #[arg(short, long)]
        rounds: Option<u64>,

        /// Enable the live web dashboard.
        #[arg(long)]
        dashboard: bool,

        /// Dashboard port (default: 8080).
        #[arg(long, default_value = "8080")]
        dashboard_port: u16,
    },

    /// Export checkpoint-held bugs as standalone bug_N.json artifacts.
    ///
    /// This finalizes interrupted runs whose `checkpoint.json` contains bugs
    /// before the normal end-of-run artifact writer emitted bug files.
    ExportBugs {
        /// Path to checkpoint.json.
        #[arg(short, long)]
        checkpoint: String,

        /// Output directory for bug_N.json artifacts (defaults to checkpoint parent).
        #[arg(short, long)]
        output: Option<String>,

        /// Refuse to overwrite existing bug_N.json files.
        #[arg(long)]
        no_overwrite: bool,
    },
}

fn main() {
    env_logger::init();
    chaoscontrol_explore::signal::install_signal_handlers();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            kernel,
            initrd,
            seed,
            vms,
            rounds,
            branches,
            ticks,
            quantum,
            vcpus,
            scheduling,
            max_frontier,
            output,
            disk_image,
            extra_cmdline,
            mode,
            bootstrap_budget,
            dlog,
            dlog_register_interval,
            dlog_memory_hash,
            workers,
            rare_edge_threshold,
            rare_edge_weight,
            havoc_after_stale,
            havoc_mutations,
            stale_round_limit,
            strict_memory,
            memory_mb,
            auto_minimize,
            dashboard,
            dashboard_port,
            scenario,
            scenario_phase_ticks,
            scenario_turns,
            min_assertion_exercise,
            emit_metrics,
            metrics_file,
        } => cmd_run(
            kernel,
            initrd,
            seed,
            vms,
            rounds,
            branches,
            ticks,
            quantum,
            vcpus,
            scheduling,
            max_frontier,
            output,
            disk_image,
            extra_cmdline,
            mode,
            bootstrap_budget,
            dlog,
            dlog_register_interval,
            dlog_memory_hash,
            workers,
            rare_edge_threshold,
            rare_edge_weight,
            havoc_after_stale,
            havoc_mutations,
            stale_round_limit,
            strict_memory,
            memory_mb,
            auto_minimize,
            dashboard,
            dashboard_port,
            scenario,
            scenario_phase_ticks,
            scenario_turns,
            min_assertion_exercise,
            emit_metrics,
            metrics_file,
        ),
        Commands::Reproduce {
            kernel,
            initrd,
            bug,
            seed,
            vms,
            ticks,
            quantum,
            vcpus,
            scheduling,
            disk_image,
            extra_cmdline,
            bootstrap_budget,
            memory_mb,
            serial,
            verdict_output,
        } => cmd_reproduce(
            kernel,
            initrd,
            bug,
            seed,
            vms,
            ticks,
            quantum,
            vcpus,
            scheduling,
            disk_image,
            extra_cmdline,
            bootstrap_budget,
            memory_mb,
            serial,
            verdict_output,
        ),
        Commands::Campaign {
            kernel,
            initrd,
            seed,
            vms,
            rounds,
            branches,
            ticks,
            quantum,
            vcpus,
            scheduling,
            max_frontier,
            output,
            disk_image,
            extra_cmdline,
            mode,
            bootstrap_budget,
            campaign_seeds,
            seeds,
            workers,
            workers_per_seed,
            rare_edge_threshold,
            rare_edge_weight,
            havoc_after_stale,
            havoc_mutations,
            stale_round_limit,
            strict_memory,
            auto_minimize,
            dashboard,
            dashboard_port,
            scenario,
            scenario_phase_ticks,
            scenario_turns,
            min_assertion_exercise,
        } => cmd_campaign(
            kernel,
            initrd,
            seed,
            vms,
            rounds,
            branches,
            ticks,
            quantum,
            vcpus,
            scheduling,
            max_frontier,
            output,
            disk_image,
            extra_cmdline,
            mode,
            bootstrap_budget,
            campaign_seeds,
            seeds,
            workers,
            workers_per_seed,
            rare_edge_threshold,
            rare_edge_weight,
            havoc_after_stale,
            havoc_mutations,
            stale_round_limit,
            strict_memory,
            auto_minimize,
            dashboard,
            dashboard_port,
            scenario,
            scenario_phase_ticks,
            scenario_turns,
            min_assertion_exercise,
        ),
        Commands::CampaignResume { corpus, rounds } => cmd_campaign_resume(corpus, rounds),
        Commands::ExportBugs {
            checkpoint,
            output,
            no_overwrite,
        } => cmd_export_bugs(checkpoint, output, !no_overwrite),
        Commands::Resume {
            corpus,
            kernel,
            initrd,
            rounds,
            dashboard,
            dashboard_port,
        } => cmd_resume(corpus, kernel, initrd, rounds, dashboard, dashboard_port),
        Commands::Minimize {
            kernel,
            initrd,
            bug,
            seed,
            vms,
            ticks,
            quantum,
            vcpus,
            scheduling,
            disk_image,
            extra_cmdline,
            bootstrap_budget,
            memory_mb,
            output,
        } => cmd_minimize(
            kernel,
            initrd,
            bug,
            seed,
            vms,
            ticks,
            quantum,
            vcpus,
            scheduling,
            disk_image,
            extra_cmdline,
            bootstrap_budget,
            memory_mb,
            output,
        ),
    }
}

fn cmd_export_bugs(checkpoint: String, output: Option<String>, overwrite: bool) {
    let checkpoint_path = Path::new(&checkpoint);
    if !checkpoint_path.exists() {
        eprintln!("Error: checkpoint file not found: {}", checkpoint);
        std::process::exit(1);
    }

    let output_dir = output.unwrap_or_else(|| {
        checkpoint_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_string_lossy()
            .into_owned()
    });

    match export_checkpoint_bugs(&checkpoint, &output_dir, overwrite) {
        Ok(summary) => {
            eprintln!(
                "Exported {} checkpoint bug artifact(s) to {}",
                summary.bugs_written, output_dir
            );
            if summary.snapshot_refs_validated > 0 {
                eprintln!(
                    "Validated {} replay parent snapshot reference(s)",
                    summary.snapshot_refs_validated
                );
            }
        }
        Err(e) => {
            eprintln!("Error: failed to export checkpoint bugs: {}", e);
            std::process::exit(1);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_run(
    kernel: String,
    initrd: Option<String>,
    seed: u64,
    vms: usize,
    rounds: u64,
    branches: usize,
    ticks: u64,
    quantum: u64,
    vcpus: usize,
    scheduling: String,
    max_frontier: usize,
    output: Option<String>,
    disk_image: Option<String>,
    extra_cmdline: Option<String>,
    mode: String,
    bootstrap_budget: u64,
    dlog: Option<String>,
    dlog_register_interval: u64,
    dlog_memory_hash: bool,
    workers: usize,
    rare_edge_threshold: u8,
    rare_edge_weight: f64,
    havoc_after_stale: u64,
    havoc_mutations: Vec<u32>,
    stale_round_limit: u64,
    strict_memory: bool,
    memory_mb: usize,
    auto_minimize: bool,
    dashboard: bool,
    dashboard_port: u16,
    scenario: Option<String>,
    scenario_phase_ticks: u64,
    scenario_turns: usize,
    min_assertion_exercise: usize,
    emit_metrics: bool,
    metrics_file: Option<String>,
) {
    // Parse scenario config
    let scenario_config = scenario.map(|name| {
        let family = chaoscontrol_fault::scenario::ScenarioFamily::from_str_loose(&name)
            .unwrap_or_else(|| {
                eprintln!("Error: unknown scenario family '{}'. Available: network-ring, volatile-write-ring, degraded-io-ring", name);
                std::process::exit(1);
            });
        chaoscontrol_fault::scenario::ScenarioConfig::new(family, vms, scenario_phase_ticks, scenario_turns)
    });

    // Validate inputs
    if !Path::new(&kernel).exists() {
        eprintln!("Error: kernel file not found: {}", kernel);
        std::process::exit(1);
    }

    if let Some(ref initrd_path) = initrd {
        if !Path::new(initrd_path).exists() {
            eprintln!("Error: initrd file not found: {}", initrd_path);
            std::process::exit(1);
        }
    }

    if let Some(ref disk_image_path) = disk_image {
        if !Path::new(disk_image_path).exists() {
            eprintln!("Error: disk image file not found: {}", disk_image_path);
            std::process::exit(1);
        }
    }

    // Create output directory if specified
    if let Some(ref output_dir) = output {
        if let Err(e) = fs::create_dir_all(output_dir) {
            eprintln!("Error: failed to create output directory: {}", e);
            std::process::exit(1);
        }
    }

    // Memory check
    let vm_memory_mb = memory_mb;
    let estimated_mb = vms * vm_memory_mb;
    if let Err(e) = chaoscontrol_explore::memory::check_memory(estimated_mb, strict_memory) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    // Parse scheduling strategy
    let scheduling_strategy = match scheduling.as_str() {
        "round-robin" | "rr" => SchedulingStrategy::RoundRobin,
        "randomized" | "rand" => SchedulingStrategy::Randomized {
            min_quantum: 50,
            max_quantum: 200,
        },
        other => {
            eprintln!(
                "Error: unknown scheduling strategy '{}'. Use 'round-robin' or 'randomized'.",
                other
            );
            std::process::exit(1);
        }
    };

    // Parse exploration mode
    let exploration_mode = match mode.as_str() {
        "fault-schedule" | "faults" | "fs" => ExplorationMode::FaultSchedule,
        "input-tree" | "inputs" | "it" => ExplorationMode::InputTree,
        "hybrid" | "both" => ExplorationMode::Hybrid,
        other => {
            eprintln!(
                "Error: unknown exploration mode '{}'. Use 'fault-schedule', 'input-tree', or 'hybrid'.",
                other
            );
            std::process::exit(1);
        }
    };

    // Build VM config with SMP settings
    let vm_config = VmConfig {
        memory_size: memory_mb * 1024 * 1024,
        num_vcpus: vcpus,
        scheduling_strategy,
        extra_cmdline: extra_cmdline.clone(),
        dlog_register_interval,
        dlog_memory_hash,
        ..Default::default()
    };

    // Build configuration
    let smp = vm_config.num_vcpus > 1;
    let config = ExplorerConfig {
        num_vms: vms,
        vm_config,
        kernel_path: kernel.clone(),
        initrd_path: initrd.clone(),
        seed,
        branch_factor: branches,
        ticks_per_branch: ticks,
        max_rounds: rounds,
        max_frontier,
        quantum,
        scheduling_strategy,
        mutation: MutationConfig::default(),
        exploration_mode,
        coverage_gpa: COVERAGE_BITMAP_ADDR,
        output_dir: output.clone(),
        disk_image_path: disk_image.clone(),
        bootstrap_budget,
        dlog_dir: dlog.as_ref().map(std::path::PathBuf::from),
        dlog_register_interval,
        dlog_memory_hash,
        num_workers: workers,
        stale_round_limit,
        schedule_diversity: smp,
        rare_edge_threshold,
        rare_edge_weight,
        havoc_after_stale,
        havoc_mutations: [
            havoc_mutations.first().copied().unwrap_or(4),
            havoc_mutations.get(1).copied().unwrap_or(16),
        ],
        scenario: scenario_config.clone(),
        emit_metrics,
        metrics_file: metrics_file.clone().map(std::path::PathBuf::from),
    };

    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!("  ChaosControl Exploration");
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!();
    eprintln!("Configuration:");
    eprintln!("  Kernel:         {}", kernel);
    if let Some(ref initrd_path) = initrd {
        eprintln!("  Initrd:         {}", initrd_path);
    }
    eprintln!("  VMs:            {}", vms);
    eprintln!("  Seed:           {}", seed);
    eprintln!("  Rounds:         {}", rounds);
    eprintln!("  Branches/round: {}", branches);
    eprintln!("  Ticks/branch:   {}", ticks);
    eprintln!("  Quantum:        {}", quantum);
    eprintln!("  vCPUs/VM:       {}", vcpus);
    eprintln!("  Scheduling:     {}", scheduling);
    eprintln!("  Mode:           {}", mode);
    eprintln!("  Max frontier:   {}", max_frontier);
    eprintln!("  Bootstrap:      {} ticks", bootstrap_budget);
    if let Some(ref disk_image_path) = disk_image {
        eprintln!("  Disk image:     {}", disk_image_path);
    }
    if let Some(ref dlog_dir) = dlog {
        eprintln!("  Dlog dir:       {}", dlog_dir);
    }
    if let Some(ref extra) = extra_cmdline {
        eprintln!("  Extra cmdline:  {}", extra);
    }
    if let Some(ref output_dir) = output {
        eprintln!("  Output:         {}", output_dir);
    }
    if emit_metrics {
        eprintln!(
            "  Metrics:        {}",
            metrics_file.as_deref().unwrap_or("stderr")
        );
    }
    if let Some(ref sc) = scenario_config {
        eprintln!(
            "  Scenario:       {} ({} turns, {} ns/phase)",
            sc.family, sc.turns, sc.phase_ticks
        );
    }
    eprintln!();
    eprintln!("Starting exploration...");
    eprintln!();

    // Create explorer and run
    let config_for_minimize = config.clone();
    let mut explorer = Explorer::new(config);

    // Start dashboard if requested
    #[cfg(feature = "dashboard")]
    if dashboard {
        match chaoscontrol_explore::server::start(dashboard_port) {
            Some(sink) => {
                eprintln!("Dashboard: http://localhost:{}", dashboard_port);
                explorer.set_event_sink(sink);
            }
            None => {
                eprintln!(
                    "Warning: failed to start dashboard on port {}",
                    dashboard_port
                );
            }
        }
    }
    #[cfg(not(feature = "dashboard"))]
    if dashboard {
        eprintln!("Warning: dashboard feature not enabled. Rebuild with --features dashboard");
    }
    let _ = (dashboard, dashboard_port);

    // Run with progress tracking
    let report = match run_with_progress(&mut explorer) {
        Ok(r) => r,
        Err(e) => {
            eprintln!();
            eprintln!("Exploration failed: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!();
    eprintln!("Exploration complete!");
    eprintln!();

    // Format and print report
    let formatted = format_report(&report);
    println!("{}", formatted);

    // Save output if requested
    if let Some(ref output_dir) = output {
        // Save formatted report
        let report_path = format!("{}/report.txt", output_dir);
        if let Err(e) = fs::write(&report_path, &formatted) {
            eprintln!("Warning: failed to save report: {}", e);
        } else {
            eprintln!("Saved report to: {}", report_path);
        }

        // Save bugs as JSON (consumable by `minimize` subcommand) + Debug text
        let snapshot_store =
            chaoscontrol_explore::snapshot_store::FileSnapshotStore::new(output_dir);
        for bug in &report.bugs {
            // JSON format (for minimize subcommand)
            let mut serialized: chaoscontrol_explore::checkpoint::SerializableBug = bug.into();
            if let Some(snapshot) = bug.snapshot.as_ref() {
                match chaoscontrol_explore::snapshot_store::SnapshotStore::put_snapshot(
                    &snapshot_store,
                    snapshot,
                    bug.replay_parent_depth,
                ) {
                    Ok(reference) => {
                        serialized.replay_parent_snapshot_ref = Some(reference);
                    }
                    Err(e) if bug.replay_parent_depth > 0 => {
                        eprintln!(
                            "Error: failed to persist required replay parent snapshot for bug {}: {}",
                            bug.bug_id, e
                        );
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: failed to persist schedule-only replay parent metadata for bug {}: {}",
                            bug.bug_id, e
                        );
                    }
                }
            } else if bug.replay_parent_depth > 0 {
                eprintln!(
                    "Error: bug {} requires replay parent snapshot depth {} but no parent snapshot is available to persist",
                    bug.bug_id, bug.replay_parent_depth
                );
                std::process::exit(1);
            }
            let json_path = format!("{}/bug_{}.json", output_dir, bug.bug_id);
            match serde_json::to_string_pretty(&serialized) {
                Ok(json) => {
                    if let Err(e) = fs::write(&json_path, &json) {
                        eprintln!("Warning: failed to save bug {} JSON: {}", bug.bug_id, e);
                    } else {
                        eprintln!("Saved bug {} to: {}", bug.bug_id, json_path);
                    }
                }
                Err(e) => {
                    eprintln!("Warning: failed to serialize bug {}: {}", bug.bug_id, e);
                }
            }

            // Note: Debug format (.txt) not saved — BugReport's snapshot
            // field contains full guest memory, making .txt files ~12GB each.
            // Use the JSON format for bug reproduction.
        }

        // Save per-assertion detail as JSON
        if !report.assertion_details.is_empty() {
            let assertions_path = format!("{}/assertions.json", output_dir);
            match serde_json::to_string_pretty(&report.assertion_details) {
                Ok(json) => {
                    if let Err(e) = fs::write(&assertions_path, &json) {
                        eprintln!("Warning: failed to save assertions: {}", e);
                    } else {
                        eprintln!(
                            "Saved {} assertion details to: {}",
                            report.assertion_details.len(),
                            assertions_path
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Warning: failed to serialize assertions: {}", e);
                }
            }
        }
    }

    // Auto-minimize bugs if requested
    if auto_minimize && !report.bugs.is_empty() {
        if chaoscontrol_explore::signal::shutdown_requested() {
            eprintln!("Skipping auto-minimize: interrupted");
        } else if let Some(ref output_dir) = output {
            auto_minimize_bugs(&report.bugs, &config_for_minimize, output_dir);
        }
    }

    let floor_failures =
        min_assertion_exercise_failures(&report.assertion_details, min_assertion_exercise);
    if floor_failures > 0 {
        eprintln!(
            "Assertion exercise floor failed: {} guest/category group(s) below {} exercised assertions",
            floor_failures, min_assertion_exercise
        );
        std::process::exit(3);
    }

    // Exit with error code if bugs found
    if !report.bugs.is_empty() {
        std::process::exit(1);
    }
}

fn run_with_progress(
    explorer: &mut Explorer,
) -> Result<
    chaoscontrol_explore::explorer::ExplorationReport,
    chaoscontrol_explore::explorer::ExploreError,
> {
    // We need to manually run the exploration loop to inject progress output
    // Since Explorer::run() doesn't expose round-by-round progress, we'll
    // use the stats() method to track progress after each internal step.

    // For now, just call run() and poll stats periodically if we could.
    // But Explorer::run() is blocking, so we'll just run it and report at the end.
    // To get per-round progress, we'd need Explorer to have a callback or iterator.

    // Actually, looking at the Explorer implementation, it uses log::info! internally
    // for progress. So with env_logger, that will show progress automatically.

    // Let's just call run() - the internal logging will show progress
    explorer.run()
}

#[allow(clippy::too_many_arguments)]
fn cmd_campaign(
    kernel: String,
    initrd: Option<String>,
    seed: u64,
    vms: usize,
    rounds: u64,
    branches: usize,
    ticks: u64,
    quantum: u64,
    vcpus: usize,
    scheduling: String,
    max_frontier: usize,
    output: String,
    disk_image: Option<String>,
    extra_cmdline: Option<String>,
    mode: String,
    bootstrap_budget: u64,
    campaign_seeds: usize,
    seeds: Option<Vec<u64>>,
    workers: usize,
    workers_per_seed: usize,
    rare_edge_threshold: u8,
    rare_edge_weight: f64,
    havoc_after_stale: u64,
    havoc_mutations: Vec<u32>,
    stale_round_limit: u64,
    strict_memory: bool,
    auto_minimize: bool,
    dashboard: bool,
    #[cfg_attr(not(feature = "dashboard"), allow(unused_variables))] dashboard_port: u16,
    scenario: Option<String>,
    scenario_phase_ticks: u64,
    scenario_turns: usize,
    min_assertion_exercise: usize,
) {
    // Parse scenario config
    let scenario_config = scenario.map(|name| {
        let family = chaoscontrol_fault::scenario::ScenarioFamily::from_str_loose(&name)
            .unwrap_or_else(|| {
                eprintln!("Error: unknown scenario family '{}'. Available: network-ring, volatile-write-ring, degraded-io-ring", name);
                std::process::exit(1);
            });
        chaoscontrol_fault::scenario::ScenarioConfig::new(family, vms, scenario_phase_ticks, scenario_turns)
    });

    // Start dashboard if requested.
    #[cfg(feature = "dashboard")]
    let _dashboard_tx = if dashboard {
        chaoscontrol_explore::server::start(dashboard_port)
    } else {
        None
    };
    #[cfg(not(feature = "dashboard"))]
    if dashboard {
        eprintln!("Warning: dashboard feature not enabled, ignoring --dashboard");
    }

    // Validate inputs
    if !Path::new(&kernel).exists() {
        eprintln!("Error: kernel file not found: {}", kernel);
        std::process::exit(1);
    }

    if let Some(ref initrd_path) = initrd {
        if !Path::new(initrd_path).exists() {
            eprintln!("Error: initrd file not found: {}", initrd_path);
            std::process::exit(1);
        }
    }

    if let Some(ref disk_image_path) = disk_image {
        if !Path::new(disk_image_path).exists() {
            eprintln!("Error: disk image file not found: {}", disk_image_path);
            std::process::exit(1);
        }
    }

    if workers > 1 {
        eprintln!("Warning: --workers ignored in campaign mode, use --workers-per-seed instead");
    }

    // Compute effective workers per seed.
    let seed_list_len = seeds.as_ref().map_or(campaign_seeds, |s| s.len());
    let effective_workers_per_seed = if workers_per_seed == 0 {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let auto = (cores / (seed_list_len * vms).max(1)).max(1);
        eprintln!(
            "Auto workers-per-seed: {} ({} cores / ({} seeds × {} VMs))",
            auto, cores, seed_list_len, vms
        );
        auto
    } else {
        workers_per_seed
    };

    // Memory check
    let vm_memory_mb = VmConfig::default().memory_size / (1024 * 1024);
    let estimated_mb = seed_list_len * vms * vm_memory_mb;
    if let Err(e) = chaoscontrol_explore::memory::check_memory(estimated_mb, strict_memory) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    let scheduling_strategy = match scheduling.as_str() {
        "round-robin" => SchedulingStrategy::RoundRobin,
        "randomized" | "rand" => SchedulingStrategy::Randomized {
            min_quantum: 50,
            max_quantum: 200,
        },
        other => {
            eprintln!("Error: unknown scheduling strategy: {}", other);
            std::process::exit(1);
        }
    };

    let exploration_mode = match mode.as_str() {
        "fault-schedule" => ExplorationMode::FaultSchedule,
        "input-tree" => ExplorationMode::InputTree,
        "hybrid" => ExplorationMode::Hybrid,
        other => {
            eprintln!("Error: unknown exploration mode: {}", other);
            std::process::exit(1);
        }
    };

    let vm_config = VmConfig {
        num_vcpus: vcpus,
        scheduling_strategy,
        extra_cmdline,
        ..VmConfig::default()
    };

    let smp = vm_config.num_vcpus > 1;
    let base_config = ExplorerConfig {
        num_vms: vms,
        vm_config,
        kernel_path: kernel,
        initrd_path: initrd,
        seed, // overridden per-seed by CampaignRunner
        branch_factor: branches,
        ticks_per_branch: ticks,
        max_rounds: rounds,
        max_frontier,
        quantum,
        scheduling_strategy,
        mutation: MutationConfig::default(),
        exploration_mode,
        coverage_gpa: COVERAGE_BITMAP_ADDR,
        output_dir: None, // set per-seed by CampaignRunner
        disk_image_path: disk_image,
        bootstrap_budget,
        dlog_dir: None,
        dlog_register_interval: 0,
        dlog_memory_hash: false,
        num_workers: effective_workers_per_seed,
        stale_round_limit,
        schedule_diversity: smp,
        rare_edge_threshold,
        rare_edge_weight,
        havoc_after_stale,
        havoc_mutations: [
            havoc_mutations.first().copied().unwrap_or(4),
            havoc_mutations.get(1).copied().unwrap_or(16),
        ],
        scenario: scenario_config,
        emit_metrics: false,
        metrics_file: None,
    };

    let seed_list = generate_seeds(seed, campaign_seeds, seeds.as_deref());

    let base_config_for_minimize = base_config.clone();
    let campaign_config = CampaignConfig {
        seeds: seed_list,
        base_explorer_config: base_config,
        output_dir: output.clone(),
    };

    let runner = CampaignRunner::new(campaign_config);
    match runner.run() {
        Ok(report) => {
            // Write reports
            let formatted = format_campaign_report(&report);
            println!("{}\n", formatted);

            if let Err(e) = fs::create_dir_all(&output) {
                eprintln!("Error creating output directory: {}", e);
            }

            let report_path = format!("{}/campaign_report.txt", output);
            if let Err(e) = fs::write(&report_path, &formatted) {
                eprintln!("Error writing report: {}", e);
            } else {
                eprintln!("Report saved to {}", report_path);
            }

            let json_path = format!("{}/campaign_report.json", output);
            match serde_json::to_string_pretty(&report) {
                Ok(json) => {
                    if let Err(e) = fs::write(&json_path, &json) {
                        eprintln!("Error writing JSON report: {}", e);
                    } else {
                        eprintln!("JSON report saved to {}", json_path);
                    }
                }
                Err(e) => eprintln!("Error serializing report: {}", e),
            }

            let assertions_path = format!("{}/assertions.json", output);
            match serde_json::to_string_pretty(&report.assertion_details) {
                Ok(json) => {
                    if let Err(e) = fs::write(&assertions_path, &json) {
                        eprintln!("Error writing assertions: {}", e);
                    } else {
                        eprintln!("Assertions saved to {}", assertions_path);
                    }
                }
                Err(e) => eprintln!("Error serializing assertions: {}", e),
            }

            // Auto-minimize campaign bugs
            if auto_minimize && !report.bugs.is_empty() {
                if chaoscontrol_explore::signal::shutdown_requested() {
                    eprintln!("Skipping auto-minimize: interrupted");
                } else {
                    // Convert CampaignBugs to BugReports for the minimizer.
                    let bugs: Vec<chaoscontrol_explore::corpus::BugReport> = report
                        .bugs
                        .iter()
                        .map(|cb| chaoscontrol_explore::corpus::BugReport {
                            bug_id: cb.bug.bug_id,
                            assertion_id: cb.bug.assertion_id,
                            assertion_location: cb.bug.assertion_location.clone(),
                            schedule: (&cb.bug.schedule).into(),
                            snapshot: None,
                            tick: cb.bug.tick,
                            replay_parent_depth: cb.bug.replay_parent_depth,
                            replay_parent_snapshot_ref: cb.bug.replay_parent_snapshot_ref.clone(),
                            dedup_key: cb.dedup_key,
                            schedule_variant: None,
                            scenario_config: cb.bug.scenario_config.clone(),
                            scenario_summary: cb.bug.scenario_summary.clone(),
                        })
                        .collect();
                    let min_dir = format!("{}/minimized", output);
                    let _ = fs::create_dir_all(&min_dir);
                    auto_minimize_bugs(&bugs, &base_config_for_minimize, &min_dir);
                }
            }

            let floor_failures =
                min_assertion_exercise_failures(&report.assertion_details, min_assertion_exercise);
            if floor_failures > 0 {
                eprintln!(
                    "Assertion exercise floor failed: {} guest/category group(s) below {} exercised assertions",
                    floor_failures, min_assertion_exercise
                );
                std::process::exit(3);
            }

            // Exit code: 0 = bugs found, 1 = no bugs
            if report.bugs.is_empty() {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Campaign failed: {}", e);
            std::process::exit(2);
        }
    }
}

fn cmd_campaign_resume(corpus: String, rounds_override: Option<u64>) {
    if !Path::new(&corpus).is_dir() {
        eprintln!("Error: campaign directory not found: {}", corpus);
        std::process::exit(1);
    }

    let progress = match load_campaign_progress(&corpus) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "Error: failed to load campaign_progress.json from {}: {}",
                corpus, e
            );
            std::process::exit(1);
        }
    };

    let remaining: Vec<u64> = progress
        .seeds
        .iter()
        .filter(|s| !progress.completed.contains_key(s))
        .copied()
        .collect();

    if remaining.is_empty() {
        eprintln!("All {} seeds already complete.", progress.seeds.len());
        // Aggregate existing results and write final report.
        let mut reports: Vec<(u64, chaoscontrol_explore::explorer::ExplorationReport, f64)> =
            Vec::new();
        for (seed, summary) in &progress.completed {
            // Load per-seed checkpoint to reconstruct a minimal report.
            let seed_dir = format!("{}/seed_{}", corpus, seed);
            let cp_path = format!("{}/checkpoint.json", seed_dir);
            if let Ok(cp) = load_checkpoint(&cp_path) {
                let report = chaoscontrol_explore::explorer::ExplorationReport {
                    rounds: cp.rounds_completed,
                    total_branches: cp.total_branches_run,
                    total_edges: cp.total_edges,
                    bugs: Vec::new(), // bugs are in per-seed output
                    corpus_size: 0,
                    coverage_stats: chaoscontrol_explore::coverage::CoverageStats {
                        total_edges: cp.total_edges,
                        total_runs: cp.total_branches_run,
                        edges_per_run_avg: if cp.total_branches_run > 0 {
                            cp.total_edges as f64 / cp.total_branches_run as f64
                        } else {
                            0.0
                        },
                    },
                    network_stats: Default::default(),
                    assertion_stats: Default::default(),
                    assertion_details: Vec::new(),
                    round_history: cp.round_history.unwrap_or_default(),
                    wall_clock_seconds: summary.wall_clock_seconds,
                    branches_per_second: if summary.wall_clock_seconds > 0.0 {
                        cp.total_branches_run as f64 / summary.wall_clock_seconds
                    } else {
                        0.0
                    },
                    edges_per_second: if summary.wall_clock_seconds > 0.0 {
                        cp.total_edges as f64 / summary.wall_clock_seconds
                    } else {
                        0.0
                    },
                    scenario_config: cp.scenario.clone(),
                    scenario_summary: cp.scenario_summary.clone(),
                };
                reports.push((*seed, report, summary.wall_clock_seconds));
            }
        }
        let campaign_report = chaoscontrol_explore::campaign::aggregate_reports(reports, 0.0);
        let formatted = format_campaign_report(&campaign_report);
        println!("{}", formatted);

        // Write final reports
        if let Ok(json) = serde_json::to_string_pretty(&campaign_report) {
            let _ = fs::write(format!("{}/campaign_report.json", corpus), &json);
        }
        let _ = fs::write(format!("{}/campaign_report.txt", corpus), &formatted);
        return;
    }

    eprintln!(
        "Resuming campaign: {} of {} seeds remaining ({:?})",
        remaining.len(),
        progress.seeds.len(),
        remaining,
    );

    // Reconstruct ExplorerConfig from checkpoint.
    let cfg = &progress.config;
    let exploration_mode = match cfg.exploration_mode.as_str() {
        "input-tree" => ExplorationMode::InputTree,
        "hybrid" => ExplorationMode::Hybrid,
        _ => ExplorationMode::FaultSchedule,
    };

    let vm_config = VmConfig {
        num_vcpus: cfg.num_vcpus,
        ..VmConfig::default()
    };

    let base_config = ExplorerConfig {
        num_vms: cfg.num_vms,
        vm_config,
        kernel_path: cfg.kernel_path.clone(),
        initrd_path: cfg.initrd_path.clone(),
        seed: cfg.seed,
        branch_factor: cfg.branch_factor,
        ticks_per_branch: cfg.ticks_per_branch,
        max_rounds: rounds_override.unwrap_or(cfg.max_rounds),
        quantum: cfg.quantum,
        exploration_mode,
        disk_image_path: cfg.disk_image_path.clone(),
        bootstrap_budget: cfg.bootstrap_budget,
        stale_round_limit: cfg.stale_round_limit,
        num_workers: 1,
        output_dir: None,
        ..ExplorerConfig::default()
    };

    let campaign_config = CampaignConfig {
        seeds: remaining,
        base_explorer_config: base_config,
        output_dir: corpus.clone(),
    };

    let runner = CampaignRunner::new(campaign_config);
    match runner.run() {
        Ok(report) => {
            let formatted = format_campaign_report(&report);
            println!("{}", formatted);

            if report.bugs.is_empty() {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Campaign resume failed: {}", e);
            std::process::exit(2);
        }
    }
}

fn cmd_resume(
    corpus: String,
    kernel_override: Option<String>,
    initrd_override: Option<String>,
    rounds_override: Option<u64>,
    dashboard: bool,
    dashboard_port: u16,
) {
    // Validate corpus directory exists
    if !Path::new(&corpus).is_dir() {
        eprintln!("Error: corpus directory not found: {}", corpus);
        std::process::exit(1);
    }

    // Load checkpoint
    let checkpoint_path = format!("{}/checkpoint.json", corpus);
    if !Path::new(&checkpoint_path).exists() {
        eprintln!("Error: checkpoint file not found: {}", checkpoint_path);
        eprintln!("Expected: {}", checkpoint_path);
        std::process::exit(1);
    }

    let checkpoint = match load_checkpoint(&checkpoint_path) {
        Ok(cp) => cp,
        Err(e) => {
            eprintln!("Error: failed to load checkpoint: {}", e);
            std::process::exit(1);
        }
    };

    // Determine actual paths (overrides take precedence)
    let kernel_path = kernel_override
        .clone()
        .unwrap_or_else(|| checkpoint.config.kernel_path.clone());
    let initrd_path = initrd_override
        .clone()
        .or(checkpoint.config.initrd_path.clone());

    // Validate kernel/initrd paths
    if !Path::new(&kernel_path).exists() {
        eprintln!("Error: kernel file not found: {}", kernel_path);
        std::process::exit(1);
    }

    if let Some(ref initrd) = initrd_path {
        if !Path::new(initrd).exists() {
            eprintln!("Error: initrd file not found: {}", initrd);
            std::process::exit(1);
        }
    }

    // Calculate remaining rounds
    let max_rounds = rounds_override.unwrap_or(checkpoint.config.max_rounds);
    let rounds_to_run = max_rounds.saturating_sub(checkpoint.rounds_completed);

    if rounds_to_run == 0 {
        eprintln!(
            "Error: checkpoint already completed {} rounds (max: {})",
            checkpoint.rounds_completed, max_rounds
        );
        eprintln!("Use --rounds to increase the round limit");
        std::process::exit(1);
    }

    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!("  ChaosControl Exploration (RESUME)");
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!();
    eprintln!("Checkpoint loaded from: {}", checkpoint_path);
    eprintln!();
    eprintln!("Previous progress:");
    eprintln!("  Rounds completed:  {}", checkpoint.rounds_completed);
    eprintln!("  Branches run:      {}", checkpoint.total_branches_run);
    eprintln!("  Edges discovered:  {}", checkpoint.total_edges);
    eprintln!("  Bugs found:        {}", checkpoint.bugs.len());
    eprintln!();
    eprintln!("Configuration:");
    eprintln!("  Kernel:            {}", kernel_path);
    if let Some(ref initrd) = initrd_path {
        eprintln!("  Initrd:            {}", initrd);
    }
    eprintln!("  VMs:               {}", checkpoint.config.num_vms);
    eprintln!("  Seed:              {}", checkpoint.config.seed);
    eprintln!("  Max rounds:        {}", max_rounds);
    eprintln!("  Remaining rounds:  {}", rounds_to_run);
    eprintln!("  Branches/round:    {}", checkpoint.config.branch_factor);
    eprintln!(
        "  Ticks/branch:      {}",
        checkpoint.config.ticks_per_branch
    );
    eprintln!("  Output:            {}", corpus);
    eprintln!();
    eprintln!("Resuming exploration...");
    eprintln!();

    // Create explorer from checkpoint
    let mut explorer = Explorer::from_checkpoint(
        checkpoint,
        kernel_override,
        initrd_override,
        Some(max_rounds),
    );

    // Update output directory to continue saving checkpoints
    explorer.config_mut().output_dir = Some(corpus.clone());

    // Start dashboard if requested
    #[cfg(feature = "dashboard")]
    if dashboard {
        match chaoscontrol_explore::server::start(dashboard_port) {
            Some(sink) => {
                eprintln!("Dashboard: http://localhost:{}", dashboard_port);
                explorer.set_event_sink(sink);
            }
            None => {
                eprintln!(
                    "Warning: failed to start dashboard on port {}",
                    dashboard_port
                );
            }
        }
    }
    #[cfg(not(feature = "dashboard"))]
    if dashboard {
        eprintln!("Warning: dashboard feature not enabled. Rebuild with --features dashboard");
    }
    let _ = (dashboard, dashboard_port);

    // Run exploration
    let report = match run_with_progress(&mut explorer) {
        Ok(r) => r,
        Err(e) => {
            eprintln!();
            eprintln!("Exploration failed: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!();
    eprintln!("Exploration complete!");
    eprintln!();

    // Format and print report
    let formatted = format_report(&report);
    println!("{}", formatted);

    // Save output
    let report_path = format!("{}/report.txt", corpus);
    if let Err(e) = fs::write(&report_path, &formatted) {
        eprintln!("Warning: failed to save report: {}", e);
    } else {
        eprintln!("Saved report to: {}", report_path);
    }

    // Save bugs as JSON (for minimize/reproduce subcommands)
    let snapshot_store = chaoscontrol_explore::snapshot_store::FileSnapshotStore::new(&corpus);
    for bug in &report.bugs {
        let mut serialized: chaoscontrol_explore::checkpoint::SerializableBug = bug.into();
        if serialized.replay_parent_snapshot_ref.is_none() {
            if let Some(snapshot) = bug.snapshot.as_ref() {
                match chaoscontrol_explore::snapshot_store::SnapshotStore::put_snapshot(
                    &snapshot_store,
                    snapshot,
                    bug.replay_parent_depth,
                ) {
                    Ok(reference) => serialized.replay_parent_snapshot_ref = Some(reference),
                    Err(e) if bug.replay_parent_depth > 0 => {
                        eprintln!(
                            "Error: failed to persist required replay parent snapshot for bug {}: {}",
                            bug.bug_id, e
                        );
                        std::process::exit(1);
                    }
                    Err(e) => eprintln!(
                        "Warning: failed to persist optional replay parent snapshot for bug {}: {}",
                        bug.bug_id, e
                    ),
                }
            } else if bug.replay_parent_depth > 0 {
                eprintln!(
                    "Error: bug {} requires replay parent snapshot context (depth {}) but no snapshot is available to persist",
                    bug.bug_id, bug.replay_parent_depth
                );
                std::process::exit(1);
            }
        }
        let json_path = format!("{}/bug_{}.json", corpus, bug.bug_id);
        match serde_json::to_string_pretty(&serialized) {
            Ok(json) => {
                if let Err(e) = fs::write(&json_path, &json) {
                    eprintln!("Warning: failed to save bug {} JSON: {}", bug.bug_id, e);
                } else {
                    eprintln!("Saved bug {} to: {}", bug.bug_id, json_path);
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to serialize bug {}: {}", bug.bug_id, e);
            }
        }
    }

    // Save per-assertion detail as JSON
    if !report.assertion_details.is_empty() {
        let assertions_path = format!("{}/assertions.json", corpus);
        match serde_json::to_string_pretty(&report.assertion_details) {
            Ok(json) => {
                if let Err(e) = fs::write(&assertions_path, &json) {
                    eprintln!("Warning: failed to save assertions: {}", e);
                } else {
                    eprintln!(
                        "Saved {} assertion details to: {}",
                        report.assertion_details.len(),
                        assertions_path
                    );
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to serialize assertions: {}", e);
            }
        }
    }

    // Exit with error code if bugs found
    if !report.bugs.is_empty() {
        std::process::exit(1);
    }
}

fn load_replay_parent_snapshot(
    bug_path: &str,
    serialized_bug: &chaoscontrol_explore::checkpoint::SerializableBug,
) -> Option<chaoscontrol_vmm::controller::SimulationSnapshot> {
    let (snapshot, validation) = load_replay_parent_snapshot_for_verdict(bug_path, serialized_bug);
    if validation.status != chaoscontrol_explore::replay_verdict::SnapshotValidationStatus::Valid
        && validation.status
            != chaoscontrol_explore::replay_verdict::SnapshotValidationStatus::NotRequired
    {
        if let Some(diagnostic) = validation.diagnostic {
            eprintln!("Error: {diagnostic}");
        }
        std::process::exit(1);
    }
    snapshot
}

fn load_replay_parent_snapshot_for_verdict(
    bug_path: &str,
    serialized_bug: &chaoscontrol_explore::checkpoint::SerializableBug,
) -> (
    Option<chaoscontrol_vmm::controller::SimulationSnapshot>,
    chaoscontrol_explore::replay_verdict::ReplaySnapshotValidation,
) {
    use chaoscontrol_explore::replay_verdict::ReplaySnapshotValidation;
    use chaoscontrol_explore::snapshot_store::{FileSnapshotStore, SnapshotStore};

    match serialized_bug.replay_parent_snapshot_ref.as_ref() {
        Some(reference) => {
            let root = Path::new(bug_path)
                .parent()
                .unwrap_or_else(|| Path::new("."));
            let store = FileSnapshotStore::new(root);
            match store.get_snapshot_artifact(reference) {
                Ok(artifact) => {
                    eprintln!(
                        "Loaded replay parent snapshot artifact: {} ({})",
                        reference.path, reference.digest
                    );
                    (
                        Some(artifact.snapshot),
                        ReplaySnapshotValidation::valid(reference.clone()),
                    )
                }
                Err(e) => (
                    None,
                    ReplaySnapshotValidation::from_error(reference.clone(), &e),
                ),
            }
        }
        None if serialized_bug.replay_parent_depth > 0 => (
            None,
            ReplaySnapshotValidation::missing_ref(format!(
                "bug requires replay parent snapshot context (depth {}) but has no replay_parent_snapshot_ref",
                serialized_bug.replay_parent_depth
            )),
        ),
        None => (None, ReplaySnapshotValidation::not_required()),
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_minimize(
    kernel: String,
    initrd: Option<String>,
    bug_path: String,
    seed: u64,
    vms: usize,
    ticks: u64,
    quantum: u64,
    vcpus: usize,
    scheduling: String,
    disk_image: Option<String>,
    extra_cmdline: Option<String>,
    bootstrap_budget: u64,
    memory_mb: usize,
    output: Option<String>,
) {
    // Validate inputs
    if !Path::new(&kernel).exists() {
        eprintln!("Error: kernel file not found: {}", kernel);
        std::process::exit(1);
    }
    if !Path::new(&bug_path).exists() {
        eprintln!("Error: bug file not found: {}", bug_path);
        std::process::exit(1);
    }

    // Load bug report (JSON with SerializableBug structure)
    let bug_json = match fs::read_to_string(&bug_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: failed to read bug file: {}", e);
            std::process::exit(1);
        }
    };

    let serialized_bug: chaoscontrol_explore::checkpoint::SerializableBug =
        match serde_json::from_str(&bug_json) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error: failed to parse bug file: {}", e);
                eprintln!("Expected JSON with fields: bug_id, assertion_id, assertion_location, schedule, tick");
                std::process::exit(1);
            }
        };

    let replay_parent_snapshot = load_replay_parent_snapshot(&bug_path, &serialized_bug);

    // Convert serialized schedule back to FaultSchedule
    let schedule: FaultSchedule = (&serialized_bug.schedule).into();

    let bug = BugReport {
        bug_id: serialized_bug.bug_id,
        assertion_id: serialized_bug.assertion_id,
        assertion_location: serialized_bug.assertion_location.clone(),
        schedule,
        snapshot: replay_parent_snapshot,
        tick: serialized_bug.tick,
        replay_parent_depth: serialized_bug.replay_parent_depth,
        replay_parent_snapshot_ref: serialized_bug.replay_parent_snapshot_ref.clone(),
        dedup_key: serialized_bug.dedup_key.unwrap_or(0),
        schedule_variant: None,
        scenario_config: serialized_bug.scenario_config.clone(),
        scenario_summary: serialized_bug.scenario_summary.clone(),
    };

    // Parse scheduling strategy
    let scheduling_strategy = match scheduling.as_str() {
        "round-robin" | "rr" => SchedulingStrategy::RoundRobin,
        "randomized" | "rand" => SchedulingStrategy::Randomized {
            min_quantum: 50,
            max_quantum: 200,
        },
        other => {
            eprintln!("Error: unknown scheduling strategy '{}'", other);
            std::process::exit(1);
        }
    };

    let vm_config = VmConfig {
        memory_size: memory_mb * 1024 * 1024,
        num_vcpus: vcpus,
        scheduling_strategy,
        extra_cmdline: extra_cmdline.clone(),
        ..Default::default()
    };

    let config = MinimizeConfig {
        num_vms: vms,
        vm_config,
        kernel_path: kernel.clone(),
        initrd_path: initrd.clone(),
        seed,
        quantum,
        scheduling_strategy,
        ticks_per_branch: ticks,
        disk_image_path: disk_image.clone(),
        bootstrap_budget,
        coverage_gpa: COVERAGE_BITMAP_ADDR,
    };

    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!("  ChaosControl Schedule Minimizer");
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!();
    eprintln!("Bug report:       {}", bug_path);
    eprintln!("Assertion ID:     {}", bug.assertion_id);
    eprintln!("Assertion:        {}", bug.assertion_location);
    eprintln!("Original faults:  {}", bug.schedule.total());
    eprintln!("Bug tick:         {}", bug.tick);
    eprintln!();
    eprintln!("Configuration:");
    eprintln!("  Kernel:         {}", kernel);
    if let Some(ref initrd_path) = initrd {
        eprintln!("  Initrd:         {}", initrd_path);
    }
    eprintln!("  VMs:            {}", vms);
    eprintln!("  Seed:           {}", seed);
    eprintln!("  Ticks/branch:   {}", ticks);
    eprintln!("  Quantum:        {}", quantum);
    eprintln!();
    eprintln!("Minimizing...");
    eprintln!();

    let mut minimizer = Minimizer::new(config, bug);

    let result = match minimizer.minimize() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Minimization failed: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!();
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!("  Minimization Result");
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!();
    eprintln!("  Original faults:   {}", result.original_faults);
    eprintln!("  Minimized faults:  {}", result.minimized_faults);
    let reduction = if result.original_faults > 0 {
        (1.0 - result.minimized_faults as f64 / result.original_faults as f64) * 100.0
    } else {
        0.0
    };
    eprintln!("  Reduction:         {:.1}%", reduction);
    eprintln!("  Candidates tested: {}", result.candidates_tested);
    eprintln!();

    // Print minimized schedule
    let faults = result.schedule.faults();
    if faults.is_empty() {
        eprintln!("  No faults needed (bug triggers without fault injection)");
    } else {
        eprintln!("  Minimized fault schedule:");
        for (i, fault) in faults.iter().enumerate() {
            eprintln!("    [{}] @ {}ns: {:?}", i + 1, fault.time_ns, fault.fault);
        }
    }
    eprintln!();

    // Save minimized bug report
    if let Some(ref output_path) = output {
        let minimized_bug = chaoscontrol_explore::checkpoint::SerializableBug {
            bug_id: result.assertion_id,
            assertion_id: result.assertion_id,
            assertion_location: String::new(),
            schedule: (&result.schedule).into(),
            tick: 0,
            replay_parent_depth: serialized_bug.replay_parent_depth,
            replay_parent_snapshot_ref: serialized_bug.replay_parent_snapshot_ref.clone(),
            dedup_key: None,
            schedule_variant: None,
            scenario_config: serialized_bug.scenario_config.clone(),
            scenario_summary: serialized_bug.scenario_summary.clone(),
        };

        match serde_json::to_string_pretty(&minimized_bug) {
            Ok(json) => {
                if let Err(e) = fs::write(output_path, &json) {
                    eprintln!("Warning: failed to save minimized schedule: {}", e);
                } else {
                    eprintln!("Saved minimized bug to: {}", output_path);
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to serialize: {}", e);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_reproduce(
    kernel: String,
    initrd: Option<String>,
    bug_path: String,
    seed: u64,
    vms: usize,
    ticks: u64,
    quantum: u64,
    vcpus: usize,
    scheduling: String,
    disk_image: Option<String>,
    extra_cmdline: Option<String>,
    bootstrap_budget: u64,
    memory_mb: usize,
    show_serial: bool,
    verdict_output: Option<String>,
) {
    use chaoscontrol_fault::oracle::Verdict;
    use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController};

    // Validate inputs
    if !Path::new(&kernel).exists() {
        eprintln!("Error: kernel file not found: {}", kernel);
        std::process::exit(1);
    }
    if !Path::new(&bug_path).exists() {
        eprintln!("Error: bug file not found: {}", bug_path);
        std::process::exit(1);
    }

    // Load bug report
    let bug_json = match fs::read_to_string(&bug_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: failed to read bug file: {}", e);
            std::process::exit(1);
        }
    };

    let serialized_bug: chaoscontrol_explore::checkpoint::SerializableBug =
        match serde_json::from_str(&bug_json) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error: failed to parse bug file: {}", e);
                std::process::exit(1);
            }
        };

    let command_context = std::env::args().collect::<Vec<_>>().join(" ");
    let (replay_parent_snapshot, snapshot_validation) =
        load_replay_parent_snapshot_for_verdict(&bug_path, &serialized_bug);
    if matches!(
        snapshot_validation.status,
        chaoscontrol_explore::replay_verdict::SnapshotValidationStatus::MissingRef
            | chaoscontrol_explore::replay_verdict::SnapshotValidationStatus::MissingArtifact
            | chaoscontrol_explore::replay_verdict::SnapshotValidationStatus::InvalidDigest
            | chaoscontrol_explore::replay_verdict::SnapshotValidationStatus::InvalidRef
    ) {
        let diagnostic = snapshot_validation
            .diagnostic
            .clone()
            .unwrap_or_else(|| "invalid replay parent snapshot evidence".to_string());
        let verdict = chaoscontrol_explore::replay_verdict::ReplayVerdict::from_reproduce(
            command_context,
            1,
            bug_path.clone(),
            &serialized_bug,
            snapshot_validation,
            false,
            diagnostic.clone(),
        );
        if let Some(path) = verdict_output.as_ref() {
            if let Err(e) = chaoscontrol_explore::replay_verdict::write_verdict(path, &verdict) {
                eprintln!("Warning: failed to write replay verdict {}: {}", path, e);
            } else {
                eprintln!("Replay verdict: {}", path);
            }
        }
        eprintln!("Error: {diagnostic}");
        std::process::exit(1);
    }

    let schedule: FaultSchedule = (&serialized_bug.schedule).into();
    let target_assertion = serialized_bug.assertion_id;

    // Parse scheduling strategy
    let scheduling_strategy = match scheduling.as_str() {
        "round-robin" | "rr" => SchedulingStrategy::RoundRobin,
        "randomized" | "rand" => SchedulingStrategy::Randomized {
            min_quantum: 50,
            max_quantum: 200,
        },
        other => {
            eprintln!("Error: unknown scheduling strategy '{}'", other);
            std::process::exit(1);
        }
    };

    let vm_config = VmConfig {
        memory_size: memory_mb * 1024 * 1024,
        num_vcpus: vcpus,
        scheduling_strategy,
        extra_cmdline,
        ..Default::default()
    };

    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!("  ChaosControl Bug Reproducer");
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!();
    eprintln!("Bug report:       {}", bug_path);
    eprintln!("Assertion ID:     {}", target_assertion);
    eprintln!("Assertion:        {}", serialized_bug.assertion_location);
    eprintln!("Faults:           {}", schedule.total());
    eprintln!();
    eprintln!("Configuration:");
    eprintln!("  Kernel:         {}", kernel);
    if let Some(ref initrd_path) = initrd {
        eprintln!("  Initrd:         {}", initrd_path);
    }
    eprintln!("  VMs:            {}", vms);
    eprintln!("  Seed:           {}", seed);
    eprintln!("  Ticks:          {}", ticks);
    eprintln!("  Quantum:        {}", quantum);
    if let Some(ref di) = disk_image {
        eprintln!("  Disk image:     {}", di);
    }
    eprintln!();

    // Print fault schedule
    let faults = schedule.faults();
    if !faults.is_empty() {
        eprintln!("Fault schedule:");
        for (i, fault) in faults.iter().enumerate() {
            eprintln!("  [{}] @ {}ns: {:?}", i + 1, fault.time_ns, fault.fault);
        }
        eprintln!();
    }

    if replay_parent_snapshot.is_some() {
        eprintln!("Loading persisted replay parent snapshot...");
    } else {
        eprintln!("Bootstrapping...");
    }

    // Create simulation controller
    let sim_config = SimulationConfig {
        num_vms: vms,
        vm_config,
        kernel_path: kernel.clone(),
        initrd_path: initrd.clone(),
        seed,
        quantum,
        schedule: FaultSchedule::new(), // empty during bootstrap
        disk_image_path: disk_image,
        bootstrap_budget: None,
        base_core: None,
        dlog_dir: None,
    };

    let mut controller = match SimulationController::new(sim_config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: failed to create simulation: {}", e);
            std::process::exit(1);
        }
    };

    let snapshot = if let Some(snapshot) = replay_parent_snapshot {
        eprintln!("Replay parent snapshot loaded at tick {}", snapshot.tick);
        eprintln!();
        snapshot
    } else {
        // Bootstrap
        if let Err(e) = controller.run_until_setup_complete(bootstrap_budget) {
            eprintln!("Error: bootstrap failed: {}", e);
            std::process::exit(1);
        }

        let bootstrap_tick = controller.tick();
        eprintln!("Bootstrap complete at tick {}", bootstrap_tick);
        eprintln!();

        // Snapshot, then restore with fault schedule
        match controller.snapshot_all() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error: snapshot failed: {}", e);
                std::process::exit(1);
            }
        }
    };

    if let Err(e) = controller.restore_all(&snapshot) {
        eprintln!("Error: restore failed: {}", e);
        std::process::exit(1);
    }
    controller.reset_vm_statuses();
    controller.set_schedule(schedule);
    controller.clear_all_coverage();

    eprintln!("Running {} ticks with fault schedule...", ticks);

    // Run
    if let Err(e) = controller.run(ticks) {
        eprintln!("Error: simulation failed: {}", e);
        std::process::exit(1);
    }

    eprintln!();
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!("  Reproduction Result");
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!();

    // Check assertion results across all VMs
    let mut target_failed = false;
    let mut all_assertions = Vec::new();

    for i in 0..controller.num_vms() {
        let oracle = controller.vm(i).fault_engine().oracle();
        for (id, record) in oracle.assertions() {
            let verdict = record.verdict();
            if *id == target_assertion as u32 && verdict == Verdict::Failed {
                target_failed = true;
            }
            // Deduplicate by id
            if !all_assertions.iter().any(|(aid, _, _, _)| aid == id) {
                all_assertions.push((*id, record.message.clone(), record.kind, verdict));
            }
        }
    }

    let diagnostic = if target_failed {
        let message = format!("BUG REPRODUCED — assertion {} failed", target_assertion);
        eprintln!("  ✗ {}", message);
        message
    } else {
        let message = format!(
            "Bug NOT reproduced — assertion {} did not fail",
            target_assertion
        );
        eprintln!("  ○ {}", message);
        message
    };
    eprintln!();

    // Show all assertion verdicts
    if !all_assertions.is_empty() {
        eprintln!("  Assertion results:");
        for (id, message, _kind, verdict) in &all_assertions {
            let icon = match verdict {
                Verdict::Failed => "✗",
                Verdict::Passed => "✓",
                Verdict::Unexercised => "○",
            };
            eprintln!("    {} [{}] {}", icon, id, message);
        }
        eprintln!();
    }

    // Show serial output if requested
    if show_serial {
        for i in 0..controller.num_vms() {
            let serial = controller.vm_mut(i).take_serial_output();
            if !serial.is_empty() {
                eprintln!(
                    "─── VM {} Serial Output ──────────────────────────────────────────────",
                    i
                );
                eprintln!("{}", serial);
                eprintln!();
            }
        }
    }

    // Exit code: 0 if bug reproduced, 1 if not
    let exit_status = if target_failed { 0 } else { 1 };
    if let Some(path) = verdict_output.as_ref() {
        let verdict = chaoscontrol_explore::replay_verdict::ReplayVerdict::from_reproduce(
            command_context,
            exit_status,
            bug_path.clone(),
            &serialized_bug,
            snapshot_validation,
            target_failed,
            diagnostic,
        );
        if let Err(e) = chaoscontrol_explore::replay_verdict::write_verdict(path, &verdict) {
            eprintln!("Warning: failed to write replay verdict {}: {}", path, e);
        } else {
            eprintln!("Replay verdict: {}", path);
        }
    }

    std::process::exit(exit_status);
}

/// Run delta-debugging minimization on each bug.
fn auto_minimize_bugs(
    bugs: &[chaoscontrol_explore::corpus::BugReport],
    config: &ExplorerConfig,
    output_dir: &str,
) {
    use chaoscontrol_explore::minimizer::{MinimizeConfig, Minimizer};
    use std::time::Instant;

    eprintln!(
        "\nAuto-minimizing {} bug{}...",
        bugs.len(),
        if bugs.len() == 1 { "" } else { "s" }
    );

    let min_config = MinimizeConfig {
        num_vms: config.num_vms,
        vm_config: config.vm_config.clone(),
        kernel_path: config.kernel_path.clone(),
        initrd_path: config.initrd_path.clone(),
        seed: config.seed,
        quantum: config.quantum,
        scheduling_strategy: config.scheduling_strategy,
        ticks_per_branch: config.ticks_per_branch,
        disk_image_path: config.disk_image_path.clone(),
        bootstrap_budget: config.bootstrap_budget,
        coverage_gpa: config.coverage_gpa,
    };

    for bug in bugs {
        if chaoscontrol_explore::signal::shutdown_requested() {
            eprintln!("Skipping remaining minimizations: interrupted");
            break;
        }

        let original_faults = bug.schedule.total();
        if original_faults == 0 {
            eprintln!("Bug {}: already minimal (0 faults)", bug.bug_id);
            continue;
        }

        eprintln!(
            "Minimizing bug {} ({} faults)...",
            bug.bug_id, original_faults
        );
        let start = Instant::now();

        let mut minimizer = Minimizer::new(min_config.clone(), bug.clone());
        match minimizer.minimize() {
            Ok(result) => {
                let elapsed = start.elapsed().as_secs_f64();
                eprintln!(
                    "  Bug {}: {} \u{2192} {} faults ({:.1}s)",
                    bug.bug_id, result.original_faults, result.minimized_faults, elapsed
                );

                // Save minimized schedule
                let min_bug = chaoscontrol_explore::checkpoint::SerializableBug {
                    bug_id: bug.bug_id,
                    assertion_id: bug.assertion_id,
                    assertion_location: bug.assertion_location.clone(),
                    schedule: (&result.schedule).into(),
                    tick: bug.tick,
                    replay_parent_depth: bug.replay_parent_depth,
                    replay_parent_snapshot_ref: bug.replay_parent_snapshot_ref.clone(),
                    dedup_key: Some(bug.dedup_key),
                    schedule_variant: None,
                    scenario_config: bug.scenario_config.clone(),
                    scenario_summary: bug.scenario_summary.clone(),
                };
                let path = format!("{}/bug_{}_min.json", output_dir, bug.bug_id);
                match serde_json::to_string_pretty(&min_bug) {
                    Ok(json) => {
                        if let Err(e) = fs::write(&path, &json) {
                            eprintln!("  Warning: failed to write {}: {}", path, e);
                        } else {
                            eprintln!("  Saved to {}", path);
                        }
                    }
                    Err(e) => eprintln!("  Warning: failed to serialize: {}", e),
                }
            }
            Err(e) => {
                eprintln!(
                    "  Bug {}: minimization failed ({}), keeping original",
                    bug.bug_id, e
                );
            }
        }
    }
}
