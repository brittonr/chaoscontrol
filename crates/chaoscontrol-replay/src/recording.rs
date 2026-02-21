//! Execution recording — captures checkpoints during a run.

use crate::checkpoint::{Checkpoint, CheckpointStore};
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::controller::SimulationSnapshot;
use serde::{Deserialize, Serialize};

/// A recorded execution session.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recording {
    /// Unique session ID.
    pub session_id: String,
    /// When the recording was made (Unix timestamp).
    pub timestamp: u64,
    /// The initial simulation config.
    pub config: RecordingConfig,
    /// Checkpoints taken during execution, ordered by tick.
    #[serde(skip)] // Snapshots too large for JSON
    pub checkpoints: CheckpointStore,
    /// The fault schedule that was executed.
    #[serde(skip)] // FaultSchedule doesn't implement Serialize
    pub schedule: FaultSchedule,
    /// Master seed.
    pub seed: u64,
    /// Events that occurred (faults fired, assertions hit, etc).
    pub events: Vec<RecordedEvent>,
    /// Final oracle report.
    #[serde(skip)] // OracleReport doesn't implement Serialize
    pub oracle_report: Option<OracleReport>,
    /// Total ticks executed.
    pub total_ticks: u64,
}

/// Configuration for a recording session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    /// Number of VMs in the simulation.
    pub num_vms: usize,
    /// VM memory size in bytes.
    pub vm_memory_size: usize,
    /// TSC frequency in kHz.
    pub tsc_khz: u32,
    /// Kernel path.
    pub kernel_path: String,
    /// Optional initrd path.
    pub initrd_path: Option<String>,
    /// Exits per VM per scheduling round.
    pub quantum: u64,
    /// Take a checkpoint every N ticks.
    pub checkpoint_interval: u64,
    /// Optional disk image path for virtio-blk devices.
    #[serde(default)]
    pub disk_image_path: Option<String>,
}

/// An event recorded during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RecordedEvent {
    /// A fault was fired at this tick.
    FaultFired { tick: u64, fault: String },
    /// An SDK assertion was hit.
    AssertionHit {
        tick: u64,
        vm_index: usize,
        assertion_id: u64,
        location: String,
        passed: bool,
    },
    /// A VM changed status.
    VmStatusChange {
        tick: u64,
        vm_index: usize,
        old_status: String,
        new_status: String,
    },
    /// Serial output from a VM.
    SerialOutput {
        tick: u64,
        vm_index: usize,
        data: String,
    },
    /// Bug detected.
    BugDetected {
        tick: u64,
        bug_id: u64,
        description: String,
        checkpoint_id: Option<u64>,
    },
}

/// A recording session that captures checkpoints during execution.
pub struct Recorder {
    config: RecordingConfig,
    recording: Recording,
    next_checkpoint_tick: u64,
    next_checkpoint_id: u64,
}

impl Recorder {
    /// Create a new recorder.
    pub fn new(config: RecordingConfig, schedule: FaultSchedule, seed: u64) -> Self {
        let session_id = format!("rec_{}", uuid_like_id());
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let next_checkpoint_tick = config.checkpoint_interval;

        Self {
            next_checkpoint_tick,
            next_checkpoint_id: 0,
            recording: Recording {
                session_id,
                timestamp,
                config: config.clone(),
                checkpoints: CheckpointStore::new(),
                schedule,
                seed,
                events: Vec::new(),
                oracle_report: None,
                total_ticks: 0,
            },
            config,
        }
    }

    /// Call after each simulation round. Takes checkpoint if interval reached.
    pub fn on_tick(
        &mut self,
        tick: u64,
        snapshot_fn: impl FnOnce() -> SimulationSnapshot,
        serial_output: Vec<String>,
    ) {
        self.recording.total_ticks = tick;

        // Check if it's time for a checkpoint
        if tick >= self.next_checkpoint_tick {
            let snapshot = snapshot_fn();

            // Collect events since last checkpoint
            let last_cp_tick = self
                .recording
                .checkpoints
                .all()
                .last()
                .map(|cp| cp.tick)
                .unwrap_or(0);

            let events_since_last: Vec<_> = self
                .recording
                .events
                .iter()
                .filter(|e| event_tick(e) > last_cp_tick && event_tick(e) <= tick)
                .cloned()
                .collect();

            let checkpoint = Checkpoint {
                id: self.next_checkpoint_id,
                tick,
                snapshot: Some(snapshot),
                serial_output,
                events_since_last,
            };

            self.recording.checkpoints.push(checkpoint);
            self.next_checkpoint_id += 1;
            self.next_checkpoint_tick += self.config.checkpoint_interval;
        }
    }

    /// Record an event.
    pub fn record_event(&mut self, event: RecordedEvent) {
        self.recording.events.push(event);
    }

    /// Finalize the recording.
    pub fn finish(mut self, oracle_report: OracleReport) -> Recording {
        self.recording.oracle_report = Some(oracle_report);
        self.recording
    }

    /// Get the current recording (for inspection).
    pub fn recording(&self) -> &Recording {
        &self.recording
    }
}

/// Helper to extract tick from any event.
fn event_tick(event: &RecordedEvent) -> u64 {
    match event {
        RecordedEvent::FaultFired { tick, .. } => *tick,
        RecordedEvent::AssertionHit { tick, .. } => *tick,
        RecordedEvent::VmStatusChange { tick, .. } => *tick,
        RecordedEvent::SerialOutput { tick, .. } => *tick,
        RecordedEvent::BugDetected { tick, .. } => *tick,
    }
}

/// Generate a unique ID (simple timestamp-based).
fn uuid_like_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn test_config() -> RecordingConfig {
        RecordingConfig {
            num_vms: 2,
            vm_memory_size: 256 * 1024 * 1024,
            tsc_khz: 3_000_000,
            kernel_path: "/test/vmlinux".to_string(),
            initrd_path: None,
            quantum: 100,
            checkpoint_interval: 1000,
            disk_image_path: None,
        }
    }

    fn dummy_snapshot() -> SimulationSnapshot {
        // Create a minimal dummy snapshot for testing
        use chaoscontrol_fault::engine::EngineConfig;
        use chaoscontrol_fault::engine::FaultEngine;
        use chaoscontrol_vmm::controller::{NetworkFabric, SimulationSnapshot};

        // Create a temporary engine just to get a snapshot
        let engine = FaultEngine::new(EngineConfig::default());

        SimulationSnapshot {
            tick: 0,
            vm_snapshots: vec![],
            network_state: NetworkFabric {
                partitions: vec![],
                latency: vec![],
                jitter: vec![],
                bandwidth_bps: vec![],
                next_free_tick: vec![],
                in_flight: vec![],
                packet_in_flight: vec![],
                loss_rate_ppm: vec![],
                corruption_rate_ppm: vec![],
                reorder_window: vec![],
                duplicate_rate_ppm: vec![],
                rng: rand_chacha::ChaCha20Rng::seed_from_u64(42),
                stats: Default::default(),
            },
            fault_engine_snapshot: engine.snapshot(),
        }
    }

    #[test]
    fn test_recorder_new() {
        let config = test_config();
        let schedule = FaultSchedule::new();
        let recorder = Recorder::new(config, schedule, 42);

        assert_eq!(recorder.recording.seed, 42);
        assert_eq!(recorder.recording.total_ticks, 0);
        assert!(recorder.recording.checkpoints.is_empty());
    }

    #[test]
    fn test_recorder_on_tick_no_checkpoint() {
        let config = test_config();
        let schedule = FaultSchedule::new();
        let mut recorder = Recorder::new(config, schedule, 42);

        // Tick 100 - no checkpoint yet (interval is 1000)
        recorder.on_tick(100, dummy_snapshot, vec![]);

        assert_eq!(recorder.recording.total_ticks, 100);
        assert_eq!(recorder.recording.checkpoints.len(), 0);
    }

    #[test]
    fn test_recorder_on_tick_checkpoint() {
        let config = test_config();
        let schedule = FaultSchedule::new();
        let mut recorder = Recorder::new(config, schedule, 42);

        // Tick 1000 - checkpoint
        recorder.on_tick(1000, dummy_snapshot, vec![String::from("output")]);

        assert_eq!(recorder.recording.total_ticks, 1000);
        assert_eq!(recorder.recording.checkpoints.len(), 1);

        let cp = recorder.recording.checkpoints.all()[0].clone();
        assert_eq!(cp.id, 0);
        assert_eq!(cp.tick, 1000);
        assert_eq!(cp.serial_output.len(), 1);
    }

    #[test]
    fn test_recorder_multiple_checkpoints() {
        let config = test_config();
        let schedule = FaultSchedule::new();
        let mut recorder = Recorder::new(config, schedule, 42);

        recorder.on_tick(1000, dummy_snapshot, vec![]);
        recorder.on_tick(2000, dummy_snapshot, vec![]);
        recorder.on_tick(3000, dummy_snapshot, vec![]);

        assert_eq!(recorder.recording.checkpoints.len(), 3);
        assert_eq!(recorder.recording.checkpoints.all()[0].id, 0);
        assert_eq!(recorder.recording.checkpoints.all()[1].id, 1);
        assert_eq!(recorder.recording.checkpoints.all()[2].id, 2);
    }

    #[test]
    fn test_recorder_record_event() {
        let config = test_config();
        let schedule = FaultSchedule::new();
        let mut recorder = Recorder::new(config, schedule, 42);

        recorder.record_event(RecordedEvent::FaultFired {
            tick: 100,
            fault: "NetworkPartition".to_string(),
        });

        assert_eq!(recorder.recording.events.len(), 1);
    }

    #[test]
    fn test_recorder_finish() {
        let config = test_config();
        let schedule = FaultSchedule::new();
        let recorder = Recorder::new(config, schedule, 42);

        let oracle_report = OracleReport {
            assertions: std::collections::BTreeMap::new(),
            total_runs: 1,
            passed: 0,
            failed: 0,
            unexercised: 0,
            events: vec![],
        };

        let recording = recorder.finish(oracle_report);
        assert!(recording.oracle_report.is_some());
    }

    #[test]
    fn test_event_tick_extraction() {
        let events = [
            RecordedEvent::FaultFired {
                tick: 100,
                fault: "test".to_string(),
            },
            RecordedEvent::AssertionHit {
                tick: 200,
                vm_index: 0,
                assertion_id: 1,
                location: "test".to_string(),
                passed: true,
            },
            RecordedEvent::BugDetected {
                tick: 300,
                bug_id: 1,
                description: "bug".to_string(),
                checkpoint_id: None,
            },
        ];

        assert_eq!(event_tick(&events[0]), 100);
        assert_eq!(event_tick(&events[1]), 200);
        assert_eq!(event_tick(&events[2]), 300);
    }

    #[test]
    fn test_checkpoint_events_since_last() {
        let config = test_config();
        let schedule = FaultSchedule::new();
        let mut recorder = Recorder::new(config, schedule, 42);

        // Add events at various ticks
        recorder.record_event(RecordedEvent::FaultFired {
            tick: 100,
            fault: "f1".to_string(),
        });
        recorder.record_event(RecordedEvent::FaultFired {
            tick: 500,
            fault: "f2".to_string(),
        });
        recorder.record_event(RecordedEvent::FaultFired {
            tick: 1500,
            fault: "f3".to_string(),
        });

        // Take checkpoint at 1000
        recorder.on_tick(1000, dummy_snapshot, vec![]);

        // First checkpoint should include events up to tick 1000
        let cp1 = &recorder.recording.checkpoints.all()[0];
        assert_eq!(cp1.events_since_last.len(), 2); // f1 and f2

        // Take checkpoint at 2000
        recorder.on_tick(2000, dummy_snapshot, vec![]);

        // Second checkpoint should include events after 1000
        let cp2 = &recorder.recording.checkpoints.all()[1];
        assert_eq!(cp2.events_since_last.len(), 1); // f3
    }
}
