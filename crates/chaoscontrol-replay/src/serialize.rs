//! Serialization for recordings and triage reports.

use crate::recording::{validate_recording, Recording};
use crate::triage::TriageReport;
use snafu::Snafu;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

pub const MAX_RECORDING_JSON_BYTES: usize = 16 * 1024 * 1024;

/// Maximum bytes accepted for a recording or triage JSON file.
///
/// Recordings and triage reports are metadata-only (snapshots are
/// recreated during replay), so 64 MiB is generous. The cap prevents
/// memory exhaustion from hostile or corrupt files.
const MAX_EVIDENCE_JSON_BYTES: u64 = 64 * 1024 * 1024;

/// Extra byte read past the limit to detect oversize content.
const READ_LIMIT_SENTINEL_BYTES: u64 = 1;

/// Open an evidence JSON file with admission checks: no symlinks,
/// regular files only, and a hard byte cap.
fn open_bounded_evidence_file(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "evidence JSON is not a regular file",
        ));
    }
    if metadata.len() > MAX_EVIDENCE_JSON_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "evidence JSON exceeds the byte limit",
        ));
    }
    Ok(file)
}

/// Load bounded JSON from a path. The byte cap is enforced twice: file
/// metadata before reading, and a sentinel-limited reader during parse.
fn read_bounded_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, SerializeError> {
    let file = open_bounded_evidence_file(path)?;
    let limited = file.take(MAX_EVIDENCE_JSON_BYTES + READ_LIMIT_SENTINEL_BYTES);
    let value = serde_json::from_reader(limited)?;
    Ok(value)
}

/// Errors that can occur during serialization.
#[derive(Debug, Snafu)]
pub enum SerializeError {
    #[snafu(display("IO error"), context(false))]
    Io { source: std::io::Error },
    #[snafu(display("JSON error"), context(false))]
    Json { source: serde_json::Error },
    #[snafu(display("Invalid recording: {message}"))]
    InvalidRecording { message: String },
    #[snafu(display("Recording JSON exceeds {limit} bytes"))]
    RecordingTooLarge { limit: usize },
}

/// Save a recording to a JSON file.
///
/// Note: This saves only the metadata (config, schedule, seed, events).
/// Snapshots are too large for JSON and are recreated during replay.
pub fn save_recording(recording: &Recording, path: &Path) -> Result<(), SerializeError> {
    validate_recording(recording).map_err(|error| SerializeError::InvalidRecording {
        message: format!("{error:?}"),
    })?;
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, recording)?;
    Ok(())
}

/// Load a recording from a JSON file.
///
/// Note: Loaded recordings will have empty checkpoints (no snapshots).
/// Snapshots are recreated by replaying the simulation.
pub fn load_recording(path: &Path) -> Result<Recording, SerializeError> {
    let file = open_bounded_evidence_file(path)?;
    let metadata = file.metadata()?;
    if metadata.len() > MAX_RECORDING_JSON_BYTES as u64 {
        return Err(SerializeError::RecordingTooLarge {
            limit: MAX_RECORDING_JSON_BYTES,
        });
    }
    let read_limit =
        u64::try_from(MAX_RECORDING_JSON_BYTES).expect("recording byte bound fits u64") + 1;
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len())
            .unwrap_or(MAX_RECORDING_JSON_BYTES)
            .min(MAX_RECORDING_JSON_BYTES),
    );
    file.take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_RECORDING_JSON_BYTES {
        return Err(SerializeError::RecordingTooLarge {
            limit: MAX_RECORDING_JSON_BYTES,
        });
    }
    let recording = serde_json::from_slice(&bytes)?;
    validate_recording(&recording).map_err(|error| SerializeError::InvalidRecording {
        message: format!("{error:?}"),
    })?;
    Ok(recording)
}

/// Save a triage report as a human-readable markdown file.
pub fn save_triage_report(report: &TriageReport, path: &Path) -> Result<(), SerializeError> {
    let markdown = format_triage_markdown(report);
    let mut file = File::create(path)?;
    file.write_all(markdown.as_bytes())?;
    Ok(())
}

/// Format a triage report as markdown.
fn format_triage_markdown(report: &TriageReport) -> String {
    let mut md = String::new();

    // Header
    md.push_str(&format!("# Bug Report #{}\n\n", report.bug_id));
    md.push_str(&format!("**Severity:** {:?}\n\n", report.severity));
    md.push_str(&format!("{}\n\n", report.summary));

    // Assertion details
    md.push_str("## Failed Assertion\n\n");
    md.push_str(&format!("- **ID:** {}\n", report.assertion.id));
    md.push_str(&format!("- **Type:** {}\n", report.assertion.kind));
    md.push_str(&format!("- **Location:** {}\n", report.assertion.location));
    md.push_str(&format!(
        "- **Description:** {}\n\n",
        report.assertion.description
    ));

    // Timeline
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
    md.push('\n');

    // VM States
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

    // Fault schedule
    md.push_str("## Fault Schedule\n\n");
    md.push_str(&format!("{}\n\n", report.schedule_description));

    // Reproduction
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

/// Save a triage report as JSON (for programmatic consumption).
pub fn save_triage_json(report: &TriageReport, path: &Path) -> Result<(), SerializeError> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, report)?;
    Ok(())
}

/// Load a triage report from JSON.
pub fn load_triage_json(path: &Path) -> Result<TriageReport, SerializeError> {
    read_bounded_json(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::CheckpointStore;
    use crate::recording::RecordingConfig;
    use crate::triage::{
        AssertionInfo, ReproductionInfo, Severity, TimelineEntry, VmStateSnapshot,
    };
    use chaoscontrol_fault::outcomes::{
        fault_run_id, FaultAttemptId, FaultStageEvent, FaultStageKind,
    };
    use chaoscontrol_fault::schedule::FaultSchedule;
    use tempfile::TempDir;

    fn test_recording() -> Recording {
        Recording {
            session_id: "test_session".to_string(),
            timestamp: 1234567890,
            config: RecordingConfig {
                num_vms: 2,
                vm_memory_size: 256 * 1024 * 1024,
                tsc_khz: 3_000_000,
                kernel_path: "/test/vmlinux".to_string(),
                initrd_path: Some("/test/initrd".to_string()),
                quantum: 100,
                checkpoint_interval: 1000,
                disk_image_path: None,
            },
            checkpoints: CheckpointStore::new(),
            schedule: FaultSchedule::new(),
            seed: 42,
            fault_run_sequence: 1,
            fault_run_id: fault_run_id(42, 1, FaultSchedule::new().identity()),
            events: vec![],
            fault_stage_events: vec![],
            fault_round_deltas: vec![],
            fault_outcome_ledger: Default::default(),
            oracle_report: None,
            total_ticks: 5000,
        }
    }

    fn test_triage_report() -> TriageReport {
        TriageReport {
            bug_id: 1,
            summary: "Test bug summary".to_string(),
            assertion: AssertionInfo {
                id: 10,
                location: "test.rs:123".to_string(),
                kind: "always".to_string(),
                description: "Leader must be valid".to_string(),
                details: None,
            },
            timeline: vec![
                TimelineEntry {
                    tick: 100,
                    event: "Network partition".to_string(),
                    vm_index: None,
                },
                TimelineEntry {
                    tick: 200,
                    event: "Assertion failed".to_string(),
                    vm_index: Some(0),
                },
            ],
            schedule_description: "2 faults over 5000 ticks".to_string(),
            vm_states: vec![VmStateSnapshot {
                vm_index: 0,
                status: "Running".to_string(),
                rip: 0x1000,
                serial_tail: "last output...".to_string(),
            }],
            reproduction: ReproductionInfo {
                seed: 42,
                schedule_json: "{}".to_string(),
                start_checkpoint_id: Some(0),
                ticks_to_bug: 200,
            },
            severity: Severity::Critical,
        }
    }

    #[test]
    fn test_save_and_load_recording() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("recording.json");

        let recording = test_recording();
        save_recording(&recording, &path).unwrap();

        let loaded = load_recording(&path).unwrap();
        assert_eq!(loaded.session_id, "test_session");
        assert_eq!(loaded.seed, 42);
        assert_eq!(loaded.total_ticks, 5000);
        assert!(loaded.events.is_empty());
    }

    #[test]
    fn load_rejects_trace_that_does_not_match_ledger() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("tampered-recording.json");
        let mut recording = test_recording();
        recording.fault_stage_events.push(FaultStageEvent {
            sequence: 0,
            attempt_id: FaultAttemptId([0; 32]),
            kind: FaultStageKind::Selected,
        });
        let file = File::create(&path).unwrap();
        serde_json::to_writer(file, &recording).unwrap();

        assert!(matches!(
            load_recording(&path),
            Err(SerializeError::InvalidRecording { .. })
        ));
    }

    #[test]
    fn load_rejects_oversized_recording_before_json_parse() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("oversized-recording.json");
        let file = File::create(&path).unwrap();
        file.set_len((MAX_RECORDING_JSON_BYTES as u64) + 1).unwrap();

        assert!(matches!(
            load_recording(&path),
            Err(SerializeError::RecordingTooLarge { .. })
        ));
    }

    #[test]
    fn test_save_triage_markdown() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("report.md");

        let report = test_triage_report();
        save_triage_report(&report, &path).unwrap();

        // Read back and verify content
        let mut file = File::open(&path).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();

        assert!(content.contains("# Bug Report #1"));
        assert!(content.contains("**Severity:** Critical"));
        assert!(content.contains("Leader must be valid"));
        assert!(content.contains("## Timeline"));
        assert!(content.contains("## VM States at Failure"));
        assert!(content.contains("## How to Reproduce"));
    }

    #[test]
    fn test_save_and_load_triage_json() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("report.json");

        let report = test_triage_report();
        save_triage_json(&report, &path).unwrap();

        let loaded = load_triage_json(&path).unwrap();
        assert_eq!(loaded.bug_id, 1);
        assert_eq!(loaded.assertion.id, 10);
        assert_eq!(loaded.timeline.len(), 2);
        assert_eq!(loaded.vm_states.len(), 1);
    }

    #[test]
    fn test_format_triage_markdown() {
        let report = test_triage_report();
        let md = format_triage_markdown(&report);

        // Check structure
        assert!(md.contains("# Bug Report #1"));
        assert!(md.contains("## Failed Assertion"));
        assert!(md.contains("## Timeline"));
        assert!(md.contains("## VM States at Failure"));
        assert!(md.contains("## Fault Schedule"));
        assert!(md.contains("## How to Reproduce"));

        // Check content
        assert!(md.contains("test.rs:123"));
        assert!(md.contains("Network partition"));
        assert!(md.contains("VM0"));
        assert!(md.contains("0x1000"));
        assert!(md.contains("Seed:** 42"));
    }

    #[test]
    fn test_markdown_timeline_table() {
        let report = test_triage_report();
        let md = format_triage_markdown(&report);

        // Verify table format
        assert!(md.contains("| Tick | VM | Event |"));
        assert!(md.contains("|------|-------|-------|"));
        assert!(md.contains("| 100 | - | Network partition |"));
        assert!(md.contains("| 200 | VM0 | Assertion failed |"));
    }

    #[test]
    fn test_markdown_serial_output() {
        let report = test_triage_report();
        let md = format_triage_markdown(&report);

        assert!(md.contains("**Serial output (tail):**"));
        assert!(md.contains("```"));
        assert!(md.contains("last output..."));
    }

    #[test]
    fn test_save_recording_io_error() {
        let path = Path::new("/nonexistent/path/recording.json");
        let recording = test_recording();
        let result = save_recording(&recording, path);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_recording_not_found() {
        let path = Path::new("/nonexistent/recording.json");
        let result = load_recording(path);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_recording_rejects_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("recording.json");
        save_recording(&test_recording(), &target).unwrap();
        let link = temp_dir.path().join("link.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(load_recording(&link).is_err());
    }

    #[test]
    fn test_load_recording_rejects_oversized_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("huge.json");
        let file = File::create(&path).unwrap();
        file.set_len(MAX_EVIDENCE_JSON_BYTES + 1).unwrap();

        assert!(load_recording(&path).is_err());
    }

    #[test]
    fn test_load_recording_rejects_directory() {
        let temp_dir = TempDir::new().unwrap();
        assert!(load_recording(temp_dir.path()).is_err());
    }

    #[test]
    fn test_load_triage_json_rejects_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("report.json");
        save_triage_json(&test_triage_report(), &target).unwrap();
        let link = temp_dir.path().join("link.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(load_triage_json(&link).is_err());
    }
}
