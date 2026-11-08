//! ChaosControl Replay CLI — time-travel debugging and bug triage

use chaoscontrol_replay::debugger::Debugger;
use chaoscontrol_replay::recording::{RecordedEvent, Recording};
use chaoscontrol_replay::replay::{RealSimulationRunner, ReplayEngine, ReplayError};
use chaoscontrol_replay::serialize::{
    format_triage_markdown, load_recording, save_triage_json, save_triage_report, SerializeError,
};
use chaoscontrol_replay::session::{load_session, ExactSession, SessionError, SessionLimits};
use chaoscontrol_replay::triage::TriageEngine;
use chaoscontrol_vmm::scheduler::core::{ProgressBudget, ProgressMode};
use clap::{Parser, Subcommand};
use snafu::Snafu;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Default, clap::Args)]
struct ExecutionArgs {
    /// Explicit execution mode; a retained coarse manifest is not an opt-in.
    #[arg(long, value_parser = ["exact-single-step", "kvm-debug-boundary"])]
    smp_progress_mode: Option<String>,
    /// Optional equality check against the retained boundary budget, never an override.
    #[arg(long, requires = "smp_progress_mode", conflicts_with = "smp_instruction_quantum", value_parser = clap::value_parser!(u64).range(1..))]
    smp_boundary_quantum: Option<u64>,
    /// Match the retained immutable boundary table; never changes the profile.
    #[arg(long, requires = "smp_progress_mode", conflicts_with = "smp_instruction_quantum", value_parser = ["kvm-debug-exit-boundary-v1"])]
    smp_boundary_profile: Option<String>,
    /// Strict instruction budget equality check; never selects coarse execution.
    #[arg(long, conflicts_with = "smp_boundary_quantum", value_parser = clap::value_parser!(u64).range(1..))]
    smp_instruction_quantum: Option<u64>,
}

// r[impl chaoscontrol.kvm_debug_boundary.persistence]
impl ExecutionArgs {
    fn selection(
        &self,
        retained: &chaoscontrol_vmm::vm::VmConfig,
    ) -> Result<chaoscontrol_replay::session::ExecutionSelection, CliError> {
        let mode = match self.smp_progress_mode.as_deref() {
            Some("kvm-debug-boundary") => ProgressMode::KvmDebugBoundary,
            Some("exact-single-step") | None => ProgressMode::ExactSingleStep,
            Some(_) => {
                return Err(CliError::Other {
                    message: "unsupported SMP progress mode".into(),
                })
            }
        };
        let budget = retained.smp_progress_budget;
        let matches = match (mode, budget) {
            (
                ProgressMode::KvmDebugBoundary,
                ProgressBudget::KvmBoundaries { count, profile_id },
            ) => {
                self.smp_instruction_quantum.is_none()
                    && self
                        .smp_boundary_profile
                        .as_deref()
                        .is_none_or(|name| name == profile_id.name())
                    && self
                        .smp_boundary_quantum
                        .is_none_or(|requested| requested == count)
            }
            (ProgressMode::ExactSingleStep, ProgressBudget::Instructions(count)) => {
                self.smp_boundary_quantum.is_none()
                    && self.smp_boundary_profile.is_none()
                    && self
                        .smp_instruction_quantum
                        .is_none_or(|requested| requested == count)
            }
            _ => false,
        };
        if !matches || mode != retained.smp_progress_mode {
            return Err(CliError::Other { message: "explicit progress mode/units/budget do not match the retained session; overrides are forbidden".into() });
        }
        Ok(chaoscontrol_replay::session::ExecutionSelection {
            progress_mode: mode,
            progress_budget: budget,
        })
    }

    fn authorize(&self, session: &mut ExactSession) -> Result<(), CliError> {
        let selection = self.selection(&session.manifest().configuration.vm_config)?;
        session.authorize_execution(selection)?;
        Ok(())
    }
}

/// CLI errors for the replay binary.
#[derive(Debug, Snafu)]
enum CliError {
    #[snafu(display("Replay error: {source}"), context(false))]
    Replay { source: ReplayError },
    #[snafu(display("Serialization error"), context(false))]
    Serialize { source: SerializeError },
    #[snafu(display("Exact session admission: {source}"), context(false))]
    Session { source: SessionError },
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
        /// Directory containing an exact persisted session (not recording JSON)
        #[arg(short, long)]
        recording: PathBuf,

        /// Checkpoint ID to start from (default: retained session start)
        #[arg(short, long)]
        checkpoint: Option<u64>,

        /// Number of ticks to replay (default: all)
        #[arg(short, long)]
        ticks: Option<u64>,

        #[command(flatten)]
        execution: ExecutionArgs,
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
        /// Directory containing an exact persisted session
        #[arg(short, long)]
        recording: PathBuf,

        /// Tick to examine
        #[arg(short, long)]
        tick: u64,

        /// Output format (text or bounded JSON)
        #[arg(short, long, value_enum, default_value = "text")]
        format: DebugFormat,

        #[command(flatten)]
        execution: ExecutionArgs,
    },

    /// Interactive debugger with memory/register inspection and modification
    Inspect {
        /// Directory containing an exact persisted session
        #[arg(short, long)]
        recording: PathBuf,

        /// Read deterministic debugger commands from a bounded file; stop on error
        #[arg(long)]
        script: Option<PathBuf>,

        #[command(flatten)]
        execution: ExecutionArgs,
    },

    /// Determinism log tools (dump, diff, stats)
    Dlog {
        #[command(subcommand)]
        cmd: DlogCommands,
    },
}

#[derive(Subcommand)]
enum DlogCommands {
    /// Dump determinism log records as human-readable text
    Dump {
        /// Dlog file to dump
        #[arg(long)]
        file: PathBuf,
        /// First record index to show (default: 0)
        #[arg(long, default_value = "0")]
        from: u64,
        /// Number of records to show (default: all)
        #[arg(long, default_value = "18446744073709551615")]
        count: u64,
    },
    /// Compare two determinism log files and find divergence
    Diff {
        /// First dlog file
        #[arg(long)]
        file_a: PathBuf,
        /// Second dlog file
        #[arg(long)]
        file_b: PathBuf,
        /// Include RIP in comparison (catches KASLR differences)
        #[arg(long)]
        strict: bool,
    },
    /// Show per-tag record counts and totals
    Stats {
        /// Dlog file to analyze
        #[arg(long)]
        file: PathBuf,
    },
}

#[derive(Clone, Copy, clap::ValueEnum, PartialEq, Eq)]
enum DebugFormat {
    Text,
    Json,
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
            execution,
        } => cmd_replay(recording, checkpoint, ticks, execution),
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
        Commands::Debug {
            recording,
            tick,
            format,
            execution,
        } => cmd_debug(recording, tick, format, execution),
        Commands::Inspect {
            recording,
            script,
            execution,
        } => cmd_inspect(recording, script, execution),
        Commands::Dlog { cmd } => match cmd {
            DlogCommands::Dump { file, from, count } => cmd_dlog_dump(file, from, count),
            DlogCommands::Diff {
                file_a,
                file_b,
                strict,
            } => cmd_dlog_diff(file_a, file_b, strict),
            DlogCommands::Stats { file } => cmd_dlog_stats(file),
        },
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
    execution: ExecutionArgs,
) -> Result<(), CliError> {
    let mut session = load_session(&recording_path, SessionLimits::default())?;
    execution.authorize(&mut session)?;
    let session = Arc::new(session);
    let start = match checkpoint_id {
        Some(id) => {
            session
                .recording()
                .checkpoints
                .get(id)
                .ok_or(ReplayError::CheckpointNotFound { id })?
                .tick
        }
        None => session.manifest().retained_start_tick,
    };
    let ticks_to_run = ticks.unwrap_or(session.manifest().retained_end_tick - start);
    println!(
        "Exact session {}: replaying {ticks_to_run} ticks from {start}",
        session.manifest().session_id
    );
    println!(
        "Progress mode: {:?}; budget: {:?}",
        session.manifest().configuration.vm_config.smp_progress_mode,
        session
            .manifest()
            .configuration
            .vm_config
            .smp_progress_budget
    );
    let engine = ReplayEngine::<RealSimulationRunner>::from_session(session);
    let result = engine.replay_from(checkpoint_id, ticks_to_run)?;

    println!("\n=== Replay Results ===");
    println!("Ticks executed: {}", result.ticks_executed);
    println!(
        "Oracle: {} passed, {} failed, {} unexercised",
        result.oracle_report.passed, result.oracle_report.failed, result.oracle_report.unexercised
    );

    if result.oracle_report.failed > 0 {
        println!("\n⚠️  FAILURES DETECTED:");
        for (identity, info) in result.oracle_report.all_records() {
            if info.verdict() == chaoscontrol_fault::oracle::Verdict::Failed {
                println!(
                    "  - Assertion {identity:?} ({:?}): {}",
                    info.kind, info.message
                );
            }
        }
    }

    println!("\n=== Serial Output ===");
    for (i, output) in result.serial_output.iter().enumerate() {
        if !output.is_empty() {
            println!("--- VM{} ---", i);
            let tail = if output.len() > 500 {
                chaoscontrol_replay::text::suffix_at_char_boundary(output, 500)
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
    println!("Faults selected (legacy projection): {}", fault_count);
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

// r[impl chaoscontrol.agent_investigation.debug_state_json]
fn debug_json(recording: &Recording, digest: &str, tick: u64) -> serde_json::Value {
    let checkpoint = recording.checkpoints.at_or_before(tick);
    let window = 50u64;
    let start_tick = tick.saturating_sub(window);
    let end_tick = tick.saturating_add(window);
    let events: Vec<serde_json::Value> = recording
        .events
        .iter()
        .filter(|event| {
            let event_tick = event_tick(event);
            event_tick >= start_tick && event_tick <= end_tick
        })
        .take(500)
        .map(|event| {
            serde_json::json!({
                "tick": event_tick(event),
                "description": format_event(event),
            })
        })
        .collect();
    serde_json::json!({
        "schema": "chaoscontrol.debug_state.v2",
        "recording_digest": digest,
        "session_id": recording.session_id,
        "seed": recording.seed,
        "fault_run_sequence": recording.fault_run_sequence,
        "total_ticks": recording.total_ticks,
        "config": {
            "num_vms": recording.config.num_vms,
            "vm_memory_size": recording.config.vm_memory_size,
            "tsc_khz": recording.config.tsc_khz,
            "kernel_path": recording.config.kernel_path,
            "initrd_path": recording.config.initrd_path,
            "quantum": recording.config.quantum,
            "smp_progress_mode": recording.config.smp_progress_mode,
            "smp_progress_budget": recording.config.smp_progress_budget,
            "checkpoint_interval": recording.config.checkpoint_interval,
            "random_faults": recording.config.random_faults,
        },
        "target_tick": tick,
        "window": { "start_tick": start_tick, "end_tick": end_tick },
        "nearest_checkpoint": checkpoint.map(|cp| serde_json::json!({
            "id": cp.id,
            "tick": cp.tick,
            "distance_ticks": tick.saturating_sub(cp.tick),
        })),
        "events": events,
        "serial_output": checkpoint.map(|cp| cp.serial_output.clone()).unwrap_or_default(),
        "non_claims": [
            "A verified read-only seek is not an accepted replay verdict or correctness proof.",
            "Boundary positions, simulation ticks, and retired instructions are distinct coordinates; coarse seeks are not instruction-accurate.",
            "Metadata is diagnostic; exact restore and continuation admission run before publishing a verified position.",
        ],
    })
}

fn cmd_debug(
    recording_path: PathBuf,
    tick: u64,
    format: DebugFormat,
    execution: ExecutionArgs,
) -> Result<(), CliError> {
    let mut session = load_session(&recording_path, SessionLimits::default())?;
    execution.authorize(&mut session)?;
    let session = Arc::new(session);
    let mut debugger = Debugger::<RealSimulationRunner>::from_session(session.clone());
    let state = debugger.goto(tick)?;
    let registers = debugger.read_registers(0, 0)?;
    if format == DebugFormat::Json {
        let mut json = debug_json(session.recording(), session.recording_identity(), tick);
        json["verified_original"] = serde_json::json!(state.verified_original);
        json["live_state_available"] = serde_json::json!(state.live_state_available);
        json["counterfactual"] = serde_json::json!(state.counterfactual);
        json["restored_checkpoint_id"] = serde_json::json!(state.checkpoint_id);
        json["progress_mode"] = serde_json::json!(state.progress_mode);
        json["progress_budget"] = serde_json::json!(state.progress_budget);
        json["verified_progress_positions"] = serde_json::json!(state.progress_positions);
        json["registers_vm0_vcpu0"] = serde_json::json!({
            "rip": registers.rip, "rsp": registers.rsp, "rax": registers.rax,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!(
            "Verified original session {} at tick {} (checkpoint #{})",
            session.manifest().session_id,
            state.tick,
            state.checkpoint_id
        );
        println!("VM0 vCPU0: {registers}");
        println!(
            "Progress mode: {:?}; budget: {:?}; verified positions: {:?}",
            state.progress_mode, state.progress_budget, state.progress_positions
        );
        println!("This read-only seek is not a replay verdict or correctness claim.");
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
                format!(
                    "{}...",
                    chaoscontrol_replay::text::prefix_at_char_boundary(data, 50)
                )
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

fn cmd_inspect(
    recording_path: PathBuf,
    script: Option<PathBuf>,
    execution: ExecutionArgs,
) -> Result<(), CliError> {
    let mut session = load_session(&recording_path, SessionLimits::default())?;
    execution.authorize(&mut session)?;
    let session = Arc::new(session);
    let debugger = Debugger::<RealSimulationRunner>::from_session(session.clone());
    if let Some(path) = script {
        let file = std::fs::File::open(path)?;
        if file.metadata()?.len() > session.limits().max_output_bytes {
            return Err(ReplayError::InvalidState {
                message: "inspection script exceeds admitted output/input byte bound".into(),
            }
            .into());
        }
        chaoscontrol_replay::inspect::run_script(debugger, std::io::BufReader::new(file))?;
    } else {
        chaoscontrol_replay::inspect::run_interactive(debugger)?;
    }
    Ok(())
}

fn cmd_dlog_diff(a: PathBuf, b: PathBuf, strict: bool) -> Result<(), CliError> {
    use chaoscontrol_vmm::dlog;

    let result = dlog::dlog_diff(&a, &b, strict).map_err(|e| CliError::Other {
        message: format!("dlog diff: {e}"),
    })?;

    println!("{result}");

    match result {
        dlog::DiffResult::Identical { .. } => std::process::exit(0),
        dlog::DiffResult::Diverged { .. } => std::process::exit(1),
        dlog::DiffResult::LengthMismatch { .. } => std::process::exit(2),
    }
}

fn cmd_dlog_stats(file: PathBuf) -> Result<(), CliError> {
    use chaoscontrol_vmm::dlog;

    let counts = dlog::dlog_stats(&file).map_err(|e| CliError::Other {
        message: format!("dlog stats: {e}"),
    })?;

    let total: u64 = counts.values().sum();
    println!("Tag                Count");
    println!("───────────────────────────");
    for (tag_byte, count) in &counts {
        let name = dlog::DlogTag::from_u8(*tag_byte)
            .map(|t| format!("{t}"))
            .unwrap_or_else(|| format!("Unknown({})", tag_byte));
        println!("{:<20} {:>8}", name, count);
    }
    println!("───────────────────────────");
    println!("{:<20} {:>8}", "TOTAL", total);

    Ok(())
}

fn cmd_dlog_dump(file: PathBuf, from: u64, count: u64) -> Result<(), CliError> {
    use chaoscontrol_vmm::dlog;

    let mut stdout = std::io::stdout().lock();
    let printed =
        dlog::dlog_dump(&file, from, count, &mut stdout).map_err(|e| CliError::Other {
            message: format!("dlog dump: {e}"),
        })?;

    eprintln!("{printed} records");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_fault::outcomes::fault_run_id;
    use chaoscontrol_fault::schedule::FaultSchedule;
    use chaoscontrol_replay::checkpoint::CheckpointStore;
    use chaoscontrol_replay::recording::RecordingConfig;

    fn test_recording() -> Recording {
        Recording {
            schema_version: chaoscontrol_replay::recording::RECORDING_SCHEMA_VERSION,
            trace_start_tick: 0,
            session_id: "test_session".to_string(),
            timestamp: 1_234_567_890,
            config: RecordingConfig {
                num_vms: 2,
                smp_progress_mode: ProgressMode::ExactSingleStep,
                smp_progress_budget: ProgressBudget::Instructions(10_000),
                vm_memory_size: 256 * 1024 * 1024,
                tsc_khz: 3_000_000,
                kernel_path: "/test/vmlinux".to_string(),
                initrd_path: Some("/test/initrd".to_string()),
                quantum: 100,
                checkpoint_interval: 1000,
                disk_image_path: None,
                random_faults: false,
            },
            checkpoints: CheckpointStore::new(),
            schedule: FaultSchedule::new(),
            seed: 42,
            fault_run_sequence: 1,
            fault_run_id: fault_run_id(42, 1, FaultSchedule::new().identity()),
            events: Vec::new(),
            fault_stage_events: Vec::new(),
            fault_round_deltas: Vec::new(),
            fault_outcome_ledger: Default::default(),
            schedule_rounds: Vec::new(),
            oracle_report: None,
            total_ticks: 5000,
        }
    }

    // r[verify chaoscontrol.replay_debugger.claim_boundary]
    #[test]
    fn historical_metadata_is_diagnostic_only_for_exact_commands() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("recording.json");
        chaoscontrol_replay::serialize::save_recording(&test_recording(), &path).unwrap();
        cmd_info(path.clone()).unwrap();
        cmd_events(path.clone(), EventFilterType::All, None, None).unwrap();
        assert!(matches!(
            cmd_replay(path.clone(), None, Some(1), ExecutionArgs::default()),
            Err(CliError::Session { .. })
        ));
        assert!(matches!(
            cmd_debug(path.clone(), 0, DebugFormat::Json, ExecutionArgs::default()),
            Err(CliError::Session { .. })
        ));
        assert!(matches!(
            cmd_inspect(path, None, ExecutionArgs::default()),
            Err(CliError::Session { .. })
        ));
    }

    #[test]
    fn inspect_script_is_an_explicit_cli_input_not_a_shell() {
        let parsed = Cli::try_parse_from([
            "chaoscontrol-replay",
            "inspect",
            "--recording",
            "retained-session",
            "--script",
            "navigation.commands",
        ])
        .unwrap();
        assert!(matches!(
            parsed.command,
            Commands::Inspect {
                script: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn debug_json_has_stable_schema_window_and_non_claims() {
        let value = debug_json(&test_recording(), "b3:00", 150);
        assert_eq!(value["schema"], "chaoscontrol.debug_state.v2");
        assert_eq!(value["target_tick"], 150);
        assert_eq!(value["window"]["start_tick"], 100);
        assert_eq!(value["window"]["end_tick"], 200);
        assert_eq!(value["session_id"], "test_session");
        assert_eq!(value["total_ticks"], 5000);
        assert!(value["nearest_checkpoint"].is_null());
        assert!(value["events"].as_array().expect("events array").is_empty());
        assert!(!value["non_claims"].as_array().expect("claims").is_empty());
        assert_eq!(value["config"]["num_vms"], 2);
    }

    // r[verify chaoscontrol.kvm_debug_boundary.persistence]
    #[test]
    fn persisted_coarse_execution_requires_explicit_mode_and_matching_optional_values() {
        use chaoscontrol_vmm::scheduler::core::BoundaryProfileId;
        let vm = chaoscontrol_vmm::vm::VmConfig {
            smp_progress_mode: ProgressMode::KvmDebugBoundary,
            smp_progress_budget: ProgressBudget::KvmBoundaries {
                profile_id: BoundaryProfileId::KvmDebugExitBoundaryV1,
                count: 37,
            },
            ..Default::default()
        };
        assert!(ExecutionArgs::default().selection(&vm).is_err());
        let mut explicit = ExecutionArgs {
            smp_progress_mode: Some("kvm-debug-boundary".into()),
            ..Default::default()
        };
        assert_eq!(
            explicit.selection(&vm).unwrap().progress_budget,
            vm.smp_progress_budget
        );
        explicit.smp_boundary_quantum = Some(37);
        explicit.smp_boundary_profile = Some("kvm-debug-exit-boundary-v1".into());
        assert!(explicit.selection(&vm).is_ok());
        explicit.smp_boundary_quantum = Some(38);
        assert!(explicit.selection(&vm).is_err());
        explicit.smp_boundary_quantum = Some(37);
        explicit.smp_instruction_quantum = Some(37);
        assert!(explicit.selection(&vm).is_err());
        explicit.smp_instruction_quantum = None;
        explicit.smp_boundary_profile = Some("unknown-profile".into());
        assert!(explicit.selection(&vm).is_err());
        explicit.smp_boundary_profile = None;
        explicit.smp_progress_mode = Some("exact-single-step".into());
        assert!(explicit.selection(&vm).is_err());
    }

    // r[verify chaoscontrol.kvm_debug_boundary.admission]
    #[test]
    fn persisted_execution_cli_rejects_unknown_modes_profiles_and_mixed_units() {
        for command in ["replay", "debug", "inspect"] {
            let mut base = vec!["chaoscontrol-replay", command, "--recording", "session"];
            if command == "debug" {
                base.extend(["--tick", "0"]);
            }
            for extra in [
                vec!["--smp-progress-mode", "automatic"],
                vec![
                    "--smp-progress-mode",
                    "kvm-debug-boundary",
                    "--smp-boundary-profile",
                    "unknown",
                ],
                vec!["--smp-boundary-quantum", "37"],
                vec![
                    "--smp-progress-mode",
                    "kvm-debug-boundary",
                    "--smp-boundary-quantum",
                    "0",
                ],
                vec![
                    "--smp-progress-mode",
                    "kvm-debug-boundary",
                    "--smp-boundary-quantum",
                    "37",
                    "--smp-instruction-quantum",
                    "37",
                ],
            ] {
                assert!(Cli::try_parse_from(base.iter().copied().chain(extra)).is_err());
            }
            assert!(
                Cli::try_parse_from(
                    base.iter()
                        .copied()
                        .chain(["--smp-progress-mode", "kvm-debug-boundary",])
                )
                .is_ok(),
                "retained budget/profile need not be respecified"
            );
        }
    }
}
