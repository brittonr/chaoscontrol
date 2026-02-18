//! Automatic bug triage and report generation.

use crate::recording::{Recording, RecordedEvent};
use serde::{Deserialize, Serialize};

/// Automatically generates triage reports for bugs.
pub struct TriageEngine;

impl TriageEngine {
    /// Generate a triage report from a recording and a bug event.
    pub fn triage(recording: &Recording, bug_id: u64) -> Option<TriageReport> {
        // Find the bug event
        let bug_event = recording.events.iter().find_map(|e| {
            if let RecordedEvent::BugDetected { tick, bug_id: id, description, checkpoint_id } = e {
                if *id == bug_id {
                    return Some((*tick, description.clone(), *checkpoint_id));
                }
            }
            None
        })?;

        let (bug_tick, description, checkpoint_id) = bug_event;

        // Find the related assertion
        let assertion = recording.events.iter().find_map(|e| {
            if let RecordedEvent::AssertionHit { tick, assertion_id, location, passed, .. } = e {
                if *tick <= bug_tick && !passed {
                    return Some(AssertionInfo {
                        id: *assertion_id,
                        location: location.clone(),
                        kind: "always".to_string(), // Would need to track assertion types
                        description: description.clone(),
                    });
                }
            }
            None
        }).unwrap_or_else(|| AssertionInfo {
            id: 0,
            location: "unknown".to_string(),
            kind: "unknown".to_string(),
            description: description.clone(),
        });

        // Build timeline: events leading up to the bug
        let timeline_start = bug_tick.saturating_sub(1000);
        let timeline: Vec<_> = recording.events
            .iter()
            .filter(|e| {
                let tick = event_tick(e);
                tick >= timeline_start && tick <= bug_tick
            })
            .map(|e| TimelineEntry {
                tick: event_tick(e),
                event: format_event(e),
                vm_index: event_vm_index(e),
            })
            .collect();

        // VM states (simplified - would need actual snapshot access)
        let vm_states: Vec<_> = (0..recording.config.num_vms)
            .map(|i| VmStateSnapshot {
                vm_index: i,
                status: "Running".to_string(),
                rip: 0, // Would need snapshot access
                serial_tail: get_serial_tail(recording, i, bug_tick),
            })
            .collect();

        // Determine severity
        let severity = if assertion.kind == "always" {
            Severity::Critical
        } else if assertion.kind == "reachable" {
            Severity::High
        } else if assertion.kind == "sometimes" {
            Severity::Medium
        } else {
            Severity::Low
        };

        // Build reproduction info
        let schedule_json = format!(
            "{{\"total_faults\": {}, \"remaining\": {}}}",
            recording.schedule.total(),
            recording.schedule.remaining()
        );

        let ticks_to_bug = if let Some(cp_id) = checkpoint_id {
            recording.checkpoints.get(cp_id)
                .map(|cp| bug_tick.saturating_sub(cp.tick))
                .unwrap_or(bug_tick)
        } else {
            bug_tick
        };

        let reproduction = ReproductionInfo {
            seed: recording.seed,
            schedule_json,
            start_checkpoint_id: checkpoint_id,
            ticks_to_bug,
        };

        let summary = format!(
            "Bug #{}: {} at tick {} ({})",
            bug_id,
            assertion.description,
            bug_tick,
            assertion.location
        );

        let schedule_description = format!(
            "{} faults scheduled over {} ticks",
            recording.schedule.total(),
            recording.total_ticks
        );

        Some(TriageReport {
            bug_id,
            summary,
            assertion,
            timeline,
            schedule_description,
            vm_states,
            reproduction,
            severity,
        })
    }
}

/// A comprehensive bug report for human consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageReport {
    /// Bug identifier.
    pub bug_id: u64,
    /// One-line summary.
    pub summary: String,
    /// The assertion that failed.
    pub assertion: AssertionInfo,
    /// Timeline of events leading up to the bug.
    pub timeline: Vec<TimelineEntry>,
    /// The fault schedule that triggered it.
    pub schedule_description: String,
    /// VM states at the time of failure.
    pub vm_states: Vec<VmStateSnapshot>,
    /// How to reproduce.
    pub reproduction: ReproductionInfo,
    /// Severity assessment.
    pub severity: Severity,
}

/// Information about the failed assertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionInfo {
    pub id: u64,
    pub location: String,
    pub kind: String, // "always", "sometimes", "reachable"
    pub description: String,
}

/// A single entry in the bug timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub tick: u64,
    pub event: String,
    pub vm_index: Option<usize>,
}

/// Snapshot of VM state at bug time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmStateSnapshot {
    pub vm_index: usize,
    pub status: String,
    pub rip: u64,
    pub serial_tail: String, // last 500 chars of serial output
}

/// Instructions for reproducing the bug.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReproductionInfo {
    /// Seed to reproduce.
    pub seed: u64,
    /// Fault schedule as JSON.
    pub schedule_json: String,
    /// Checkpoint to start from.
    pub start_checkpoint_id: Option<u64>,
    /// Ticks from checkpoint to bug.
    pub ticks_to_bug: u64,
}

/// Severity classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// Assertion failed that should always hold — correctness bug.
    Critical,
    /// Reachability assertion failed — unreachable code was reached.
    High,
    /// Sometimes assertion never held — liveness concern.
    Medium,
    /// Informational finding.
    Low,
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

fn event_vm_index(event: &RecordedEvent) -> Option<usize> {
    match event {
        RecordedEvent::AssertionHit { vm_index, .. } => Some(*vm_index),
        RecordedEvent::VmStatusChange { vm_index, .. } => Some(*vm_index),
        RecordedEvent::SerialOutput { vm_index, .. } => Some(*vm_index),
        _ => None,
    }
}

fn format_event(event: &RecordedEvent) -> String {
    match event {
        RecordedEvent::FaultFired { fault, .. } => format!("Fault fired: {}", fault),
        RecordedEvent::AssertionHit { assertion_id, location, passed, .. } => {
            format!("Assertion {}: {} ({})", assertion_id, location, if *passed { "PASS" } else { "FAIL" })
        }
        RecordedEvent::VmStatusChange { old_status, new_status, .. } => {
            format!("VM status: {} → {}", old_status, new_status)
        }
        RecordedEvent::SerialOutput { data, .. } => {
            let preview = if data.len() > 50 {
                format!("{}...", &data[..50])
            } else {
                data.clone()
            };
            format!("Serial: {}", preview)
        }
        RecordedEvent::BugDetected { description, .. } => {
            format!("BUG DETECTED: {}", description)
        }
    }
}

fn get_serial_tail(recording: &Recording, vm_index: usize, _tick: u64) -> String {
    // Get the most recent checkpoint serial output
    recording.checkpoints.all()
        .last()
        .and_then(|cp| cp.serial_output.get(vm_index))
        .map(|s| {
            if s.len() > 500 {
                s[s.len() - 500..].to_string()
            } else {
                s.clone()
            }
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::CheckpointStore;
    use crate::recording::RecordingConfig;
    use chaoscontrol_fault::schedule::FaultSchedule;

    fn test_recording() -> Recording {
        let mut recording = Recording {
            session_id: "test".to_string(),
            timestamp: 0,
            config: RecordingConfig {
                num_vms: 2,
                vm_memory_size: 256 * 1024 * 1024,
                tsc_khz: 3_000_000,
                kernel_path: "/test/vmlinux".to_string(),
                initrd_path: None,
                quantum: 100,
                checkpoint_interval: 1000,
            },
            checkpoints: CheckpointStore::new(),
            schedule: FaultSchedule::new(),
            seed: 42,
            events: vec![],
            oracle_report: None,
            total_ticks: 5000,
        };

        // Add some events
        recording.events.push(RecordedEvent::FaultFired {
            tick: 1000,
            fault: "NetworkPartition".to_string(),
        });

        recording.events.push(RecordedEvent::AssertionHit {
            tick: 2000,
            vm_index: 0,
            assertion_id: 1,
            location: "raft.rs:123".to_string(),
            passed: false,
        });

        recording.events.push(RecordedEvent::BugDetected {
            tick: 2000,
            bug_id: 1,
            description: "Leader election failed".to_string(),
            checkpoint_id: Some(0),
        });

        recording
    }

    #[test]
    fn test_triage_engine_generates_report() {
        let recording = test_recording();
        let report = TriageEngine::triage(&recording, 1);
        
        assert!(report.is_some());
        let report = report.unwrap();
        
        assert_eq!(report.bug_id, 1);
        assert!(report.summary.contains("Leader election failed"));
        assert_eq!(report.severity, Severity::Critical);
    }

    #[test]
    fn test_triage_engine_missing_bug() {
        let recording = test_recording();
        let report = TriageEngine::triage(&recording, 999);
        
        assert!(report.is_none());
    }

    #[test]
    fn test_triage_report_timeline() {
        let recording = test_recording();
        let report = TriageEngine::triage(&recording, 1).unwrap();
        
        // Timeline should include events leading to bug
        assert!(report.timeline.len() >= 2);
        
        let has_fault = report.timeline.iter().any(|e| e.event.contains("NetworkPartition"));
        let has_assertion = report.timeline.iter().any(|e| e.event.contains("Assertion"));
        
        assert!(has_fault);
        assert!(has_assertion);
    }

    #[test]
    fn test_triage_report_vm_states() {
        let recording = test_recording();
        let report = TriageEngine::triage(&recording, 1).unwrap();
        
        assert_eq!(report.vm_states.len(), 2);
        assert_eq!(report.vm_states[0].vm_index, 0);
        assert_eq!(report.vm_states[1].vm_index, 1);
    }

    #[test]
    fn test_triage_report_reproduction() {
        let recording = test_recording();
        let report = TriageEngine::triage(&recording, 1).unwrap();
        
        assert_eq!(report.reproduction.seed, 42);
        assert_eq!(report.reproduction.start_checkpoint_id, Some(0));
        assert!(report.reproduction.schedule_json.contains("{")); // Valid JSON
    }

    #[test]
    fn test_severity_levels() {
        assert_eq!(Severity::Critical as u8, 0);
        assert!(Severity::Critical != Severity::High);
        assert!(Severity::High != Severity::Medium);
        assert!(Severity::Medium != Severity::Low);
    }

    #[test]
    fn test_format_event() {
        let fault = RecordedEvent::FaultFired { tick: 100, fault: "Test".to_string() };
        let formatted = format_event(&fault);
        assert!(formatted.contains("Fault fired"));
        assert!(formatted.contains("Test"));

        let assertion = RecordedEvent::AssertionHit {
            tick: 200,
            vm_index: 0,
            assertion_id: 5,
            location: "test.rs:10".to_string(),
            passed: false,
        };
        let formatted = format_event(&assertion);
        assert!(formatted.contains("FAIL"));
        assert!(formatted.contains("test.rs:10"));
    }

    #[test]
    fn test_event_tick_extraction() {
        let events = vec![
            RecordedEvent::FaultFired { tick: 100, fault: "test".to_string() },
            RecordedEvent::BugDetected { tick: 200, bug_id: 1, description: "bug".to_string(), checkpoint_id: None },
        ];

        assert_eq!(event_tick(&events[0]), 100);
        assert_eq!(event_tick(&events[1]), 200);
    }

    #[test]
    fn test_event_vm_index_extraction() {
        let fault = RecordedEvent::FaultFired { tick: 100, fault: "test".to_string() };
        assert_eq!(event_vm_index(&fault), None);

        let assertion = RecordedEvent::AssertionHit {
            tick: 200,
            vm_index: 3,
            assertion_id: 1,
            location: "test".to_string(),
            passed: true,
        };
        assert_eq!(event_vm_index(&assertion), Some(3));
    }
}
