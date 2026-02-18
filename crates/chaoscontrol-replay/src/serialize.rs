//! Serialization for recordings and triage reports.

use crate::recording::Recording;
use crate::triage::TriageReport;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use thiserror::Error;

#[cfg(test)]
use std::io::Read;

/// Errors that can occur during serialization.
#[derive(Debug, Error)]
pub enum SerializeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Save a recording to a JSON file.
///
/// Note: This saves only the metadata (config, schedule, seed, events).
/// Snapshots are too large for JSON and are recreated during replay.
pub fn save_recording(recording: &Recording, path: &Path) -> Result<(), SerializeError> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, recording)?;
    Ok(())
}

/// Load a recording from a JSON file.
///
/// Note: Loaded recordings will have empty checkpoints (no snapshots).
/// Snapshots are recreated by replaying the simulation.
pub fn load_recording(path: &Path) -> Result<Recording, SerializeError> {
    let file = File::open(path)?;
    let recording = serde_json::from_reader(file)?;
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
    md.push_str(&format!("- **Description:** {}\n\n", report.assertion.description));

    // Timeline
    md.push_str("## Timeline\n\n");
    md.push_str("Events leading up to the bug:\n\n");
    md.push_str("| Tick | VM | Event |\n");
    md.push_str("|------|-------|-------|\n");
    for entry in &report.timeline {
        let vm = entry.vm_index
            .map(|i| format!("VM{}", i))
            .unwrap_or_else(|| "-".to_string());
        md.push_str(&format!("| {} | {} | {} |\n", entry.tick, vm, entry.event));
    }
    md.push_str("\n");

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
    md.push_str(&format!("3. **Run for:** {} ticks\n\n", report.reproduction.ticks_to_bug));

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
    let file = File::open(path)?;
    let report = serde_json::from_reader(file)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::CheckpointStore;
    use crate::recording::{RecordingConfig, RecordedEvent};
    use crate::triage::{AssertionInfo, TimelineEntry, VmStateSnapshot, ReproductionInfo, Severity};
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
            },
            checkpoints: CheckpointStore::new(),
            schedule: FaultSchedule::new(),
            seed: 42,
            events: vec![
                RecordedEvent::FaultFired { tick: 100, fault: "Test".to_string() },
            ],
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
            vm_states: vec![
                VmStateSnapshot {
                    vm_index: 0,
                    status: "Running".to_string(),
                    rip: 0x1000,
                    serial_tail: "last output...".to_string(),
                },
            ],
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
        assert_eq!(loaded.events.len(), 1);
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
}
