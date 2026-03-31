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

use chaoscontrol_explore::campaign::{generate_seeds, CampaignConfig, CampaignRunner};
use chaoscontrol_explore::checkpoint::load_checkpoint;
use chaoscontrol_explore::corpus::BugReport;
use chaoscontrol_explore::explorer::{ExplorationMode, Explorer, ExplorerConfig};
use chaoscontrol_explore::minimizer::{MinimizeConfig, Minimizer};
use chaoscontrol_explore::mutator::MutationConfig;
use chaoscontrol_explore::report::{format_campaign_report, format_report};
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

        /// Enable the live web dashboard.
        #[arg(long)]
        dashboard: bool,

        /// Dashboard port (default: 8080).
        #[arg(long, default_value = "8080")]
        dashboard_port: u16,
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

        /// Bootstrap tick budget.
        #[arg(long, default_value = "10000")]
        bootstrap_budget: u64,

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

        /// Bootstrap tick budget.
        #[arg(long, default_value = "10000")]
        bootstrap_budget: u64,

        /// Show serial output from each VM.
        #[arg(long)]
        serial: bool,
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

        /// Parallel workers per seed (ignored in campaign mode, logged as warning).
        #[arg(short = 'w', long, default_value = "1")]
        workers: usize,
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
}

fn main() {
    env_logger::init();

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
            dashboard,
            dashboard_port,
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
            dashboard,
            dashboard_port,
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
            bootstrap_budget,
            serial,
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
            bootstrap_budget,
            serial,
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
        ),
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
            bootstrap_budget,
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
            bootstrap_budget,
            output,
        ),
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
    dashboard: bool,
    dashboard_port: u16,
) {
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
        num_vcpus: vcpus,
        scheduling_strategy,
        extra_cmdline: extra_cmdline.clone(),
        dlog_register_interval,
        dlog_memory_hash,
        ..Default::default()
    };

    // Build configuration
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
    eprintln!();
    eprintln!("Starting exploration...");
    eprintln!();

    // Create explorer and run
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
    if let Some(output_dir) = output {
        // Save formatted report
        let report_path = format!("{}/report.txt", output_dir);
        if let Err(e) = fs::write(&report_path, &formatted) {
            eprintln!("Warning: failed to save report: {}", e);
        } else {
            eprintln!("Saved report to: {}", report_path);
        }

        // Save bugs as JSON (consumable by `minimize` subcommand) + Debug text
        for bug in &report.bugs {
            // JSON format (for minimize subcommand)
            let serialized: chaoscontrol_explore::checkpoint::SerializableBug = bug.into();
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
) {
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
        eprintln!("Warning: --workers ignored in campaign mode (each seed runs sequentially)");
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
        num_workers: 1, // forced to 1 in campaign mode
    };

    let seed_list = generate_seeds(seed, campaign_seeds, seeds.as_deref());

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
    for bug in &report.bugs {
        let serialized: chaoscontrol_explore::checkpoint::SerializableBug = bug.into();
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
    bootstrap_budget: u64,
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

    // Convert serialized schedule back to FaultSchedule
    let schedule: FaultSchedule = (&serialized_bug.schedule).into();

    let bug = BugReport {
        bug_id: serialized_bug.bug_id,
        assertion_id: serialized_bug.assertion_id,
        assertion_location: serialized_bug.assertion_location.clone(),
        schedule,
        snapshot: None,
        tick: serialized_bug.tick,
        dedup_key: serialized_bug.dedup_key.unwrap_or(0),
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
        num_vcpus: vcpus,
        scheduling_strategy,
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
            dedup_key: None,
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
    bootstrap_budget: u64,
    show_serial: bool,
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
        num_vcpus: vcpus,
        scheduling_strategy,
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

    eprintln!("Bootstrapping...");

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

    // Bootstrap
    if let Err(e) = controller.run_until_setup_complete(bootstrap_budget) {
        eprintln!("Error: bootstrap failed: {}", e);
        std::process::exit(1);
    }

    let bootstrap_tick = controller.tick();
    eprintln!("Bootstrap complete at tick {}", bootstrap_tick);
    eprintln!();

    // Snapshot, then restore with fault schedule
    let snapshot = match controller.snapshot_all() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: snapshot failed: {}", e);
            std::process::exit(1);
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

    if target_failed {
        eprintln!("  ✗ BUG REPRODUCED — assertion {} failed", target_assertion);
    } else {
        eprintln!(
            "  ○ Bug NOT reproduced — assertion {} did not fail",
            target_assertion
        );
    }
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
    if target_failed {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}
