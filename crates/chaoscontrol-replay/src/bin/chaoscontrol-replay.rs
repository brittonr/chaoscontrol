//! ChaosControl Replay CLI — time-travel debugging and bug triage

use chaoscontrol_replay::recording::RecordedEvent;
use chaoscontrol_replay::replay::{RealSimulationRunner, ReplayEngine, ReplayError};
use chaoscontrol_replay::serialize::{
    load_recording, save_triage_json, save_triage_report, SerializeError,
};
use chaoscontrol_replay::triage::TriageEngine;
use clap::{Parser, Subcommand};
use snafu::Snafu;
use std::path::PathBuf;

/// CLI errors for the replay binary.
#[derive(Debug, Snafu)]
enum CliError {
    #[snafu(display("Replay error"), context(false))]
    Replay { source: ReplayError },
    #[snafu(display("Serialization error"), context(false))]
    Serialize { source: SerializeError },
    #[snafu(display("I/O error"), context(false))]
    Io { source: std::io::Error },
    #[snafu(display("JSON error"), context(false))]
    Json { source: serde_json::Error },
    #[snafu(display("{message}"))]
    Other { message: String },
}

#[derive(Parser)]
#[command(name = "chaoscontrol-replay")]
#[command(about = "Time-travel debugging and replay for ChaosControl recordings")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Replay a recorded session
    Replay {
        /// Path to recording file
        #[arg(short, long)]
        recording: PathBuf,

        /// Checkpoint ID to start from (default: beginning)
        #[arg(short, long)]
        checkpoint: Option<u64>,

        /// Number of ticks to replay (default: all)
        #[arg(short, long)]
        ticks: Option<u64>,
    },

    /// Generate bug report from recording
    Triage {
        /// Path to recording file
        #[arg(short, long)]
        recording: PathBuf,

        /// Bug ID to triage
        #[arg(short, long)]
        bug_id: u64,

        /// Output format
        #[arg(short, long, value_enum, default_value = "markdown")]
        format: OutputFormat,

        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Show recording metadata
    Info {
        /// Path to recording file
        #[arg(short, long)]
        recording: PathBuf,
    },

    /// List events from recording
    Events {
        /// Path to recording file
        #[arg(short, long)]
        recording: PathBuf,

        /// Filter events by type
        #[arg(short, long, value_enum, default_value = "all")]
        filter: EventFilterType,

        /// Start tick (inclusive)
        #[arg(long)]
        from: Option<u64>,

        /// End tick (inclusive)
        #[arg(long)]
        to: Option<u64>,
    },

    /// Debug session at a specific tick
    Debug {
        /// Path to recording file
        #[arg(short, long)]
        recording: PathBuf,

        /// Tick to examine
        #[arg(short, long)]
        tick: u64,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
    Markdown,
    Json,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum EventFilterType {
    All,
    Faults,
    Assertions,
    Bugs,
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Replay {
            recording,
            checkpoint,
            ticks,
        } => cmd_replay(recording, checkpoint, ticks),
        Commands::Triage {
            recording,
            bug_id,
            format,
            output,
        } => cmd_triage(recording, bug_id, format, output),
        Commands::Info { recording } => cmd_info(recording),
        Commands::Events {
            recording,
            filter,
            from,
            to,
        } => cmd_events(recording, filter, from, to),
        Commands::Debug { recording, tick } => cmd_debug(recording, tick),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn cmd_replay(
    recording_path: PathBuf,
    checkpoint_id: Option<u64>,
    ticks: Option<u64>,
) -> Result<(), CliError> {
    println!("Loading recording from {:?}...", recording_path);
    let recording = load_recording(&recording_path)?;

    let ticks_to_run = ticks.unwrap_or(recording.total_ticks);

    println!(
        "Replaying {} ticks from {}...",
        ticks_to_run,
        if let Some(cp) = checkpoint_id {
            format!("checkpoint {}", cp)
        } else {
            "beginning".to_string()
        }
    );

    let engine = ReplayEngine::<RealSimulationRunner>::new(recording);
    let result = engine.replay_from(checkpoint_id, ticks_to_run)?;

    println!("\n=== Replay Results ===");
    println!("Ticks executed: {}", result.ticks_executed);
    println!(
        "Oracle: {} passed, {} failed, {} unexercised",
        result.oracle_report.passed, result.oracle_report.failed, result.oracle_report.unexercised
    );

    if result.oracle_report.failed > 0 {
        println!("\n⚠️  FAILURES DETECTED:");
        for (id, info) in &result.oracle_report.assertions {
            if info.verdict() == chaoscontrol_fault::oracle::Verdict::Failed {
                println!("  - Assertion {} ({:?}): {}", id, info.kind, info.message);
            }
        }
    }

    println!("\n=== Serial Output ===");
    for (i, output) in result.serial_output.iter().enumerate() {
        if !output.is_empty() {
            println!("--- VM{} ---", i);
            let tail = if output.len() > 500 {
                &output[output.len() - 500..]
            } else {
                output
            };
            println!("{}", tail);
        }
    }

    Ok(())
}

fn cmd_triage(
    recording_path: PathBuf,
    bug_id: u64,
    format: OutputFormat,
    output: Option<PathBuf>,
) -> Result<(), CliError> {
    println!("Loading recording from {:?}...", recording_path);
    let recording = load_recording(&recording_path)?;

    println!("Triaging bug #{}...", bug_id);
    let report = TriageEngine::triage(&recording, bug_id).ok_or_else(|| {
        OtherSnafu {
            message: format!("Bug #{} not found in recording", bug_id),
        }
        .build()
    })?;

    match (format, output) {
        (OutputFormat::Markdown, Some(path)) => {
            save_triage_report(&report, &path)?;
            println!("Saved markdown report to {:?}", path);
        }
        (OutputFormat::Json, Some(path)) => {
            save_triage_json(&report, &path)?;
            println!("Saved JSON report to {:?}", path);
        }
        (OutputFormat::Markdown, None) => {
            // Print to stdout
            println!("\n{}", format_triage_markdown(&report));
        }
        (OutputFormat::Json, None) => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }

    Ok(())
}

fn cmd_info(recording_path: PathBuf) -> Result<(), CliError> {
    let recording = load_recording(&recording_path)?;

    println!("=== Recording Info ===");
    println!("Session ID: {}", recording.session_id);
    println!("Timestamp: {}", recording.timestamp);
    println!("Seed: {}", recording.seed);

    println!("\n=== Config ===");
    println!("VMs: {}", recording.config.num_vms);
    println!(
        "Memory: {} MB",
        recording.config.vm_memory_size / (1024 * 1024)
    );
    println!("TSC: {} kHz", recording.config.tsc_khz);
    println!("Kernel: {}", recording.config.kernel_path);
    if let Some(ref initrd) = recording.config.initrd_path {
        println!("Initrd: {}", initrd);
    }
    println!("Quantum: {}", recording.config.quantum);
    println!(
        "Checkpoint interval: {} ticks",
        recording.config.checkpoint_interval
    );

    println!("\n=== Execution ===");
    println!("Total ticks: {}", recording.total_ticks);
    println!("Total events: {}", recording.events.len());
    println!("Checkpoints: {}", recording.checkpoints.len());

    // Event summary by type
    let mut fault_count = 0;
    let mut assertion_count = 0;
    let mut bug_count = 0;
    let mut status_change_count = 0;
    let mut serial_count = 0;

    for event in &recording.events {
        match event {
            RecordedEvent::FaultFired { .. } => fault_count += 1,
            RecordedEvent::AssertionHit { .. } => assertion_count += 1,
            RecordedEvent::BugDetected { .. } => bug_count += 1,
            RecordedEvent::VmStatusChange { .. } => status_change_count += 1,
            RecordedEvent::SerialOutput { .. } => serial_count += 1,
        }
    }

    println!("\n=== Event Summary ===");
    println!("Faults fired: {}", fault_count);
    println!("Assertions: {}", assertion_count);
    println!("Bugs detected: {}", bug_count);
    println!("VM status changes: {}", status_change_count);
    println!("Serial outputs: {}", serial_count);

    Ok(())
}

fn cmd_events(
    recording_path: PathBuf,
    filter: EventFilterType,
    from: Option<u64>,
    to: Option<u64>,
) -> Result<(), CliError> {
    let recording = load_recording(&recording_path)?;

    let from_tick = from.unwrap_or(0);
    let to_tick = to.unwrap_or(u64::MAX);

    println!(
        "Events (filter: {:?}, ticks: {}-{}):",
        filter, from_tick, to_tick
    );
    println!();

    for event in &recording.events {
        let tick = event_tick(event);
        if tick < from_tick || tick > to_tick {
            continue;
        }

        let matches = match filter {
            EventFilterType::All => true,
            EventFilterType::Faults => matches!(event, RecordedEvent::FaultFired { .. }),
            EventFilterType::Assertions => matches!(event, RecordedEvent::AssertionHit { .. }),
            EventFilterType::Bugs => matches!(event, RecordedEvent::BugDetected { .. }),
        };

        if matches {
            println!("[{:>10}] {}", tick, format_event(event));
        }
    }

    Ok(())
}

fn cmd_debug(recording_path: PathBuf, tick: u64) -> Result<(), CliError> {
    let recording = load_recording(&recording_path)?;

    println!("=== Debug State at Tick {} ===", tick);

    // Find nearest checkpoint
    let checkpoint = recording.checkpoints.at_or_before(tick);
    match checkpoint {
        Some(cp) => {
            println!(
                "\nNearest checkpoint: #{} at tick {} ({} ticks before target)",
                cp.id,
                cp.tick,
                tick.saturating_sub(cp.tick)
            );
        }
        None => {
            println!("\nNo checkpoint before tick {}", tick);
        }
    }

    // Show events around this tick
    let window = 50;
    let start_tick = tick.saturating_sub(window);
    let end_tick = tick + window;

    println!("\nEvents around tick {} (±{} tick window):", tick, window);
    let mut found_any = false;
    for event in &recording.events {
        let event_tick = event_tick(event);
        if event_tick >= start_tick && event_tick <= end_tick {
            found_any = true;
            let marker = if event_tick == tick { " <--" } else { "" };
            println!("  [{:>10}] {}{}", event_tick, format_event(event), marker);
        }
    }
    if !found_any {
        println!("  (no events in this window)");
    }

    // Show serial output snippet
    if let Some(cp) = checkpoint {
        println!("\nSerial output (from checkpoint):");
        for (i, output) in cp.serial_output.iter().enumerate() {
            if !output.is_empty() {
                let tail = if output.len() > 200 {
                    format!("...{}", &output[output.len() - 200..])
                } else {
                    output.clone()
                };
                println!(
                    "  VM{}: {}",
                    i,
                    tail.lines().collect::<Vec<_>>().join("\n       ")
                );
            }
        }
    }

    Ok(())
}

// Helper functions

fn event_tick(event: &RecordedEvent) -> u64 {
    match event {
        RecordedEvent::FaultFired { tick, .. } => *tick,
        RecordedEvent::AssertionHit { tick, .. } => *tick,
        RecordedEvent::VmStatusChange { tick, .. } => *tick,
        RecordedEvent::SerialOutput { tick, .. } => *tick,
        RecordedEvent::BugDetected { tick, .. } => *tick,
    }
}

fn format_event(event: &RecordedEvent) -> String {
    match event {
        RecordedEvent::FaultFired { fault, .. } => format!("⚡ Fault: {}", fault),
        RecordedEvent::AssertionHit {
            vm_index,
            assertion_id,
            location,
            passed,
            ..
        } => format!(
            "{} VM{} Assertion #{} at {}: {}",
            if *passed { "✓" } else { "✗" },
            vm_index,
            assertion_id,
            location,
            if *passed { "PASS" } else { "FAIL" }
        ),
        RecordedEvent::VmStatusChange {
            vm_index,
            old_status,
            new_status,
            ..
        } => format!("🔄 VM{} status: {} → {}", vm_index, old_status, new_status),
        RecordedEvent::SerialOutput { vm_index, data, .. } => {
            let preview = if data.len() > 50 {
                format!("{}...", &data[..50])
            } else {
                data.clone()
            };
            format!("📝 VM{} serial: {}", vm_index, preview)
        }
        RecordedEvent::BugDetected {
            bug_id,
            description,
            ..
        } => format!("🐛 BUG #{}: {}", bug_id, description),
    }
}

fn format_triage_markdown(report: &chaoscontrol_replay::triage::TriageReport) -> String {
    let mut md = String::new();

    md.push_str(&format!("# Bug Report #{}\n\n", report.bug_id));
    md.push_str(&format!("**Severity:** {:?}\n\n", report.severity));
    md.push_str(&format!("{}\n\n", report.summary));

    md.push_str("## Failed Assertion\n\n");
    md.push_str(&format!("- **ID:** {}\n", report.assertion.id));
    md.push_str(&format!("- **Type:** {}\n", report.assertion.kind));
    md.push_str(&format!("- **Location:** {}\n", report.assertion.location));
    md.push_str(&format!(
        "- **Description:** {}\n\n",
        report.assertion.description
    ));

    md.push_str("## Timeline\n\n");
    md.push_str("Events leading up to the bug:\n\n");
    md.push_str("| Tick | VM | Event |\n");
    md.push_str("|------|-------|-------|\n");
    for entry in &report.timeline {
        let vm = entry
            .vm_index
            .map(|i| format!("VM{}", i))
            .unwrap_or_else(|| "-".to_string());
        md.push_str(&format!("| {} | {} | {} |\n", entry.tick, vm, entry.event));
    }
    md.push_str("\n");

    md.push_str("## VM States at Failure\n\n");
    for vm_state in &report.vm_states {
        md.push_str(&format!("### VM{}\n\n", vm_state.vm_index));
        md.push_str(&format!("- **Status:** {}\n", vm_state.status));
        md.push_str(&format!("- **RIP:** {:#x}\n", vm_state.rip));
        if !vm_state.serial_tail.is_empty() {
            md.push_str("\n**Serial output (tail):**\n\n");
            md.push_str("```\n");
            md.push_str(&vm_state.serial_tail);
            md.push_str("\n```\n\n");
        }
    }

    md.push_str("## Fault Schedule\n\n");
    md.push_str(&format!("{}\n\n", report.schedule_description));

    md.push_str("## How to Reproduce\n\n");
    md.push_str(&format!("1. **Seed:** {}\n", report.reproduction.seed));
    if let Some(cp_id) = report.reproduction.start_checkpoint_id {
        md.push_str(&format!("2. **Start from checkpoint:** {}\n", cp_id));
    } else {
        md.push_str("2. **Start from:** beginning\n");
    }
    md.push_str(&format!(
        "3. **Run for:** {} ticks\n\n",
        report.reproduction.ticks_to_bug
    ));

    md.push_str("**Fault schedule (JSON):**\n\n");
    md.push_str("```json\n");
    md.push_str(&report.reproduction.schedule_json);
    md.push_str("\n```\n\n");

    md
}
