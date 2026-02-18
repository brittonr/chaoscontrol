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
//! # Save results to directory
//! chaoscontrol-explore run --kernel vmlinux --output results/
//! ```

use chaoscontrol_explore::explorer::{Explorer, ExplorerConfig};
use chaoscontrol_explore::mutator::MutationConfig;
use chaoscontrol_explore::report::format_report;
use chaoscontrol_protocol::COVERAGE_BITMAP_ADDR;
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

        /// Max frontier size.
        #[arg(short, long, default_value = "50")]
        max_frontier: usize,

        /// Output directory for reports and bug artifacts.
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Resume from saved corpus (not yet implemented).
    Resume {
        /// Path to corpus directory.
        #[arg(short, long)]
        corpus: String,
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
            max_frontier,
            output,
        } => cmd_run(
            kernel,
            initrd,
            seed,
            vms,
            rounds,
            branches,
            ticks,
            quantum,
            max_frontier,
            output,
        ),
        Commands::Resume { corpus } => cmd_resume(corpus),
    }
}

fn cmd_run(
    kernel: String,
    initrd: Option<String>,
    seed: u64,
    vms: usize,
    rounds: u64,
    branches: usize,
    ticks: u64,
    quantum: u64,
    max_frontier: usize,
    output: Option<String>,
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

    // Create output directory if specified
    if let Some(ref output_dir) = output {
        if let Err(e) = fs::create_dir_all(output_dir) {
            eprintln!("Error: failed to create output directory: {}", e);
            std::process::exit(1);
        }
    }

    // Build configuration
    let config = ExplorerConfig {
        num_vms: vms,
        vm_config: VmConfig::default(),
        kernel_path: kernel.clone(),
        initrd_path: initrd.clone(),
        seed,
        branch_factor: branches,
        ticks_per_branch: ticks,
        max_rounds: rounds,
        max_frontier,
        quantum,
        mutation: MutationConfig::default(),
        coverage_gpa: COVERAGE_BITMAP_ADDR,
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
    eprintln!("  Max frontier:   {}", max_frontier);
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

fn cmd_resume(corpus: String) {
    eprintln!("Resume command is not yet implemented.");
    eprintln!("Corpus directory: {}", corpus);
    eprintln!();
    eprintln!("Future functionality:");
    eprintln!("  - Load saved corpus from {}", corpus);
    eprintln!("  - Resume exploration from previous session");
    eprintln!("  - Continue building on discovered coverage");
    std::process::exit(1);
}
