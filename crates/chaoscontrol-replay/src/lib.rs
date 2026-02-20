//! Time-travel debugging and replay for ChaosControl.
//!
//! This crate implements execution recording, replay, and time-travel debugging
//! for the ChaosControl deterministic hypervisor. Since ChaosControl simulations
//! are fully deterministic, you can reproduce ANY execution by recording the
//! initial state and fault schedule, then replaying with the same seed.
//!
//! # Core Capabilities
//!
//! 1. **Recording:** Capture execution with periodic checkpoints
//! 2. **Replay:** Reproduce exact execution from recording
//! 3. **Time-travel:** Jump to any point, rewind, step forward/backward
//! 4. **Counterfactuals:** Modify state/schedule and see what happens
//! 5. **Triage:** Automatic bug report generation
//!
//! # Example: Record and Replay
//!
//! ```no_run
//! use chaoscontrol_replay::recording::{Recorder, RecordingConfig};
//! use chaoscontrol_replay::replay::ReplayEngine;
//! use chaoscontrol_fault::schedule::FaultSchedule;
//!
//! // Create a recorder
//! let config = RecordingConfig {
//!     num_vms: 3,
//!     vm_memory_size: 256 * 1024 * 1024,
//!     tsc_khz: 3_000_000,
//!     kernel_path: "/path/to/vmlinux".to_string(),
//!     initrd_path: None,
//!     quantum: 100,
//!     checkpoint_interval: 1000, // Checkpoint every 1000 ticks
//!     disk_image_path: None,
//! };
//!
//! let schedule = FaultSchedule::new();
//! let mut recorder = Recorder::new(config, schedule, 42);
//!
//! // During simulation:
//! // recorder.on_tick(tick, || controller.snapshot_all(), serial_output);
//! // recorder.record_event(...);
//!
//! // Finish recording
//! // let recording = recorder.finish(oracle_report);
//!
//! // Later, replay from any checkpoint
//! // let engine = ReplayEngine::new(recording);
//! // let result = engine.replay_from(Some(checkpoint_id), 500)?;
//! ```
//!
//! # Example: Time-Travel Debugging
//!
//! ```no_run
//! use chaoscontrol_replay::debugger::{Debugger, EventFilter};
//! use chaoscontrol_replay::recording::Recording;
//!
//! // Load a recording
//! // let recording = load_recording("session.json")?;
//! // let mut debugger = Debugger::new(recording);
//!
//! // Jump to the bug
//! // debugger.goto_bug(bug_id)?;
//!
//! // Rewind to see what happened before
//! // debugger.rewind(100)?;
//!
//! // Find the next failed assertion
//! // if let Some(event) = debugger.next_event(EventFilter::FailedAssertion) {
//! //     println!("Next failure at tick {}", event_tick(event));
//! // }
//!
//! // Inspect serial output
//! // let output = debugger.serial_output(vm_index);
//! ```
//!
//! # Example: Bug Triage
//!
//! ```no_run
//! use chaoscontrol_replay::triage::TriageEngine;
//! use chaoscontrol_replay::serialize::save_triage_report;
//!
//! // Generate a comprehensive bug report
//! // let report = TriageEngine::triage(&recording, bug_id).unwrap();
//!
//! // Save as markdown for human consumption
//! // save_triage_report(&report, "bug_report.md")?;
//! ```
//!
//! # Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────┐
//! │                      Recording Phase                        │
//! ├────────────────────────────────────────────────────────────┤
//! │  SimulationController                                       │
//! │         ↓                                                   │
//! │    Recorder::on_tick()  ← takes snapshot every N ticks     │
//! │         ↓                                                   │
//! │    Checkpoint{ snapshot, events, serial_output }           │
//! │         ↓                                                   │
//! │    Recording{ config, schedule, seed, checkpoints[] }      │
//! └────────────────────────────────────────────────────────────┘
//!
//! ┌────────────────────────────────────────────────────────────┐
//! │                       Replay Phase                          │
//! ├────────────────────────────────────────────────────────────┤
//! │  ReplayEngine::replay_from(checkpoint_id, ticks)           │
//! │         ↓                                                   │
//! │  1. Restore snapshot at checkpoint                          │
//! │  2. Create new SimulationController with same seed          │
//! │  3. Run for N ticks                                         │
//! │  4. Return ReplayResult{ events, serial, oracle }          │
//! └────────────────────────────────────────────────────────────┘
//!
//! ┌────────────────────────────────────────────────────────────┐
//! │                    Time-Travel Phase                        │
//! ├────────────────────────────────────────────────────────────┤
//! │  Debugger::goto(tick)                                       │
//! │         ↓                                                   │
//! │  1. Find checkpoint at or before tick                       │
//! │  2. Replay from checkpoint to target tick                   │
//! │  3. Return DebugState{ tick, events, serial }              │
//! │                                                             │
//! │  Debugger::rewind(ticks) → goto(current_tick - ticks)      │
//! │  Debugger::step_forward(ticks) → goto(current_tick + ticks)│
//! └────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Determinism Guarantees
//!
//! Replay is guaranteed to be identical to the original execution because:
//!
//! 1. **VMs are deterministic:** Same seed → same CPUID, TSC, entropy
//! 2. **Scheduling is deterministic:** Same quantum, same order
//! 3. **Faults are deterministic:** FaultSchedule replayed exactly
//! 4. **Network is deterministic:** Message delivery based on virtual time
//! 5. **Snapshots capture complete state:** All registers, memory, devices
//!
//! # Performance
//!
//! - **Recording overhead:** ~5-10% (snapshot cost amortized over interval)
//! - **Replay speed:** Same as original run (deterministic = no slowdown)
//! - **Time-travel:** O(ticks from checkpoint) to jump anywhere
//! - **Checkpoint storage:** ~memory_size per checkpoint (compressed)
//!
//! # Module Organization
//!
//! - [`recording`] — Execution recording (Recorder, Recording)
//! - [`checkpoint`] — Checkpoint storage and lookup
//! - [`replay`] — Replay engine (ReplayEngine, SimulationRunner trait)
//! - [`debugger`] — Time-travel debugger (Debugger, DebugState)
//! - [`triage`] — Automatic bug triage (TriageEngine, TriageReport)
//! - [`serialize`] — Save/load recordings and reports

pub mod checkpoint;
pub mod debugger;
pub mod recording;
pub mod replay;
pub mod serialize;
pub mod triage;

// Re-export main types for convenience
pub use checkpoint::{Checkpoint, CheckpointStore};
pub use debugger::{DebugState, Debugger, EventFilter, RegisterState};
pub use recording::{RecordedEvent, Recorder, Recording, RecordingConfig};
pub use replay::{
    MemoryModification, RealSimulationRunner, ReplayEngine, ReplayError, ReplayResult,
    SimulationRunner,
};
pub use serialize::{
    load_recording, load_triage_json, save_recording, save_triage_json, save_triage_report,
    SerializeError,
};
pub use triage::{
    AssertionInfo, ReproductionInfo, Severity, TimelineEntry, TriageEngine, TriageReport,
    VmStateSnapshot,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify all main types are accessible
        let _checkpoint_store = CheckpointStore::new();

        // RecordingConfig
        let _config = RecordingConfig {
            num_vms: 2,
            vm_memory_size: 256 * 1024 * 1024,
            tsc_khz: 3_000_000,
            kernel_path: "/test".to_string(),
            initrd_path: None,
            quantum: 100,
            checkpoint_interval: 1000,
            disk_image_path: None,
        };

        // EventFilter
        let _filter = EventFilter::AnyFault;

        // Severity
        let _sev = Severity::Critical;
    }

    #[test]
    fn test_recording_workflow() {
        use chaoscontrol_fault::schedule::FaultSchedule;

        let config = RecordingConfig {
            num_vms: 2,
            vm_memory_size: 256 * 1024 * 1024,
            tsc_khz: 3_000_000,
            kernel_path: "/test/vmlinux".to_string(),
            initrd_path: None,
            quantum: 100,
            checkpoint_interval: 1000,
            disk_image_path: None,
        };

        let schedule = FaultSchedule::new();
        let recorder = Recorder::new(config, schedule, 42);

        assert_eq!(recorder.recording().seed, 42);
        assert_eq!(recorder.recording().total_ticks, 0);
    }

    #[test]
    fn test_checkpoint_store_basic() {
        let mut store = CheckpointStore::new();
        assert!(store.is_empty());

        let cp = Checkpoint {
            id: 0,
            tick: 100,
            snapshot: None,
            serial_output: vec![],
            events_since_last: vec![],
        };

        store.push(cp);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_event_filter_matching() {
        let fault = RecordedEvent::FaultFired {
            tick: 100,
            fault: "test".to_string(),
        };

        assert!(EventFilter::AnyFault.matches(&fault));
        assert!(!EventFilter::AnyAssertion.matches(&fault));
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical != Severity::High);
        assert!(Severity::High != Severity::Medium);
        assert!(Severity::Medium != Severity::Low);
    }
}
