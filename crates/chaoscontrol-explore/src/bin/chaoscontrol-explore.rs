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

use chaoscontrol_explore::checkpoint::load_checkpoint;
use chaoscontrol_explore::explorer::{ExplorationMode, Explorer, ExplorerConfig};
use chaoscontrol_explore::mutator::MutationConfig;
use chaoscontrol_explore::report::format_report;
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
        ),
        Commands::Resume {
            corpus,
            kernel,
            initrd,
            rounds,
        } => cmd_resume(corpus, kernel, initrd, rounds),
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

        // Save each bug as Debug-formatted text (JSON serialization would require
        // adding #[derive(Serialize)] to BugReport in library code)
        for bug in &report.bugs {
            let bug_path = format!("{}/bug_{}.txt", output_dir, bug.bug_id);
            let bug_text = format!("{:#?}", bug);
            if let Err(e) = fs::write(&bug_path, bug_text) {
                eprintln!("Warning: failed to save bug {}: {}", bug.bug_id, e);
            } else {
                eprintln!("Saved bug {} to: {}", bug.bug_id, bug_path);
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

fn cmd_resume(
    corpus: String,
    kernel_override: Option<String>,
    initrd_override: Option<String>,
    rounds_override: Option<u64>,
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

    // Save bugs
    for bug in &report.bugs {
        let bug_path = format!("{}/bug_{}.txt", corpus, bug.bug_id);
        let bug_text = format!("{:#?}", bug);
        if let Err(e) = fs::write(&bug_path, bug_text) {
            eprintln!("Warning: failed to save bug {}: {}", bug.bug_id, e);
        } else {
            eprintln!("Saved bug {} to: {}", bug.bug_id, bug_path);
        }
    }

    // Exit with error code if bugs found
    if !report.bugs.is_empty() {
        std::process::exit(1);
    }
}
