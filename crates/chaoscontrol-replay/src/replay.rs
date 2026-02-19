//! Replay engine — restores and replays from recording.

use crate::recording::{RecordedEvent, Recording, RecordingConfig};
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::controller::{
    RoundResult, SimulationConfig, SimulationController, SimulationSnapshot,
};
use chaoscontrol_vmm::vm::VmError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during replay.
#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("VM error: {0}")]
    Vm(#[from] VmError),
    #[error("Checkpoint not found: {0}")]
    CheckpointNotFound(u64),
    #[error("Invalid replay state: {0}")]
    InvalidState(String),
    #[error("Simulation runner error: {0}")]
    Runner(String),
}

/// Trait for running simulations (allows mocking in tests).
pub trait SimulationRunner {
    /// Create a new simulation from config.
    fn create(
        config: &RecordingConfig,
        schedule: FaultSchedule,
        seed: u64,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized;

    /// Step the simulation by one round.
    fn step_round(&mut self) -> Result<RoundResult, Box<dyn std::error::Error>>;

    /// Take a snapshot of all VMs.
    fn snapshot_all(&self) -> Result<SimulationSnapshot, Box<dyn std::error::Error>>;

    /// Restore all VMs from a snapshot.
    fn restore_all(
        &mut self,
        snapshot: &SimulationSnapshot,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Get current simulation tick.
    fn tick(&self) -> u64;

    /// Get the oracle report.
    fn report(&self) -> OracleReport;

    /// Get serial output for a specific VM.
    fn serial_output(&self, vm_index: usize) -> String;
}

/// Real implementation using SimulationController.
pub struct RealSimulationRunner {
    controller: SimulationController,
}

impl SimulationRunner for RealSimulationRunner {
    fn create(
        config: &RecordingConfig,
        schedule: FaultSchedule,
        seed: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let sim_config = SimulationConfig {
            num_vms: config.num_vms,
            vm_config: chaoscontrol_vmm::vm::VmConfig {
                memory_size: config.vm_memory_size,
                cpu: chaoscontrol_vmm::cpu::CpuConfig {
                    tsc_khz: config.tsc_khz,
                    seed,
                    ..Default::default()
                },
                ..Default::default()
            },
            kernel_path: config.kernel_path.clone(),
            initrd_path: config.initrd_path.clone(),
            seed,
            quantum: config.quantum,
            schedule,
        };

        let controller = SimulationController::new(sim_config)?;
        Ok(Self { controller })
    }

    fn step_round(&mut self) -> Result<RoundResult, Box<dyn std::error::Error>> {
        Ok(self.controller.step_round()?)
    }

    fn snapshot_all(&self) -> Result<SimulationSnapshot, Box<dyn std::error::Error>> {
        Ok(self.controller.snapshot_all()?)
    }

    fn restore_all(
        &mut self,
        snapshot: &SimulationSnapshot,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(self.controller.restore_all(snapshot)?)
    }

    fn tick(&self) -> u64 {
        self.controller.tick()
    }

    fn report(&self) -> OracleReport {
        self.controller.report()
    }

    fn serial_output(&self, _vm_index: usize) -> String {
        // Serial output collection would need mutable access to controller
        // For now, return empty string as serial output is captured in checkpoints
        String::new()
    }
}

/// Replays a recording from any checkpoint.
pub struct ReplayEngine<R: SimulationRunner = RealSimulationRunner> {
    /// The recording to replay.
    recording: Recording,
    /// Phantom data for the runner type.
    _runner: std::marker::PhantomData<R>,
}

impl<R: SimulationRunner> ReplayEngine<R> {
    /// Create a new replay engine from a recording.
    pub fn new(recording: Recording) -> Self {
        Self {
            recording,
            _runner: std::marker::PhantomData,
        }
    }

    /// Replay from the beginning (or a specific checkpoint) for N ticks.
    pub fn replay_from(
        &self,
        checkpoint_id: Option<u64>,
        ticks: u64,
    ) -> Result<ReplayResult, ReplayError> {
        let mut runner = R::create(
            &self.recording.config,
            self.recording.schedule.clone(),
            self.recording.seed,
        )
        .map_err(|e| ReplayError::Runner(e.to_string()))?;

        let start_tick = if let Some(cp_id) = checkpoint_id {
            let checkpoint = self
                .recording
                .checkpoints
                .get(cp_id)
                .ok_or(ReplayError::CheckpointNotFound(cp_id))?;

            if let Some(snapshot) = &checkpoint.snapshot {
                runner
                    .restore_all(snapshot)
                    .map_err(|e| ReplayError::Runner(e.to_string()))?;
                checkpoint.tick
            } else {
                return Err(ReplayError::InvalidState(format!(
                    "Checkpoint {} has no snapshot",
                    cp_id
                )));
            }
        } else {
            0
        };

        let target_tick = start_tick + ticks;
        let mut events = Vec::new();

        while runner.tick() < target_tick {
            let result = runner
                .step_round()
                .map_err(|e| ReplayError::Runner(e.to_string()))?;

            // Collect events that occurred this tick
            for event in &self.recording.events {
                if event_tick(event) == runner.tick() {
                    events.push(event.clone());
                }
            }

            if result.vms_running == 0 {
                break;
            }
        }

        let oracle_report = runner.report();
        let serial_output: Vec<String> = (0..self.recording.config.num_vms)
            .map(|i| runner.serial_output(i))
            .collect();

        let final_snapshot = runner.snapshot_all().ok();

        Ok(ReplayResult {
            ticks_executed: runner.tick() - start_tick,
            oracle_report,
            serial_output,
            events,
            final_snapshot,
        })
    }

    /// Replay with a modified fault schedule (counterfactual).
    pub fn replay_with_schedule(
        &self,
        checkpoint_id: u64,
        modified_schedule: FaultSchedule,
        ticks: u64,
    ) -> Result<ReplayResult, ReplayError> {
        let checkpoint = self
            .recording
            .checkpoints
            .get(checkpoint_id)
            .ok_or(ReplayError::CheckpointNotFound(checkpoint_id))?;

        let snapshot = checkpoint.snapshot.as_ref().ok_or_else(|| {
            ReplayError::InvalidState(format!("Checkpoint {} has no snapshot", checkpoint_id))
        })?;

        let mut runner = R::create(
            &self.recording.config,
            modified_schedule,
            self.recording.seed,
        )
        .map_err(|e| ReplayError::Runner(e.to_string()))?;

        runner
            .restore_all(snapshot)
            .map_err(|e| ReplayError::Runner(e.to_string()))?;

        let start_tick = checkpoint.tick;
        let target_tick = start_tick + ticks;
        let events = Vec::new(); // Events not collected in counterfactual replay

        while runner.tick() < target_tick {
            let result = runner
                .step_round()
                .map_err(|e| ReplayError::Runner(e.to_string()))?;

            if result.vms_running == 0 {
                break;
            }
        }

        let oracle_report = runner.report();
        let serial_output: Vec<String> = (0..self.recording.config.num_vms)
            .map(|i| runner.serial_output(i))
            .collect();

        let final_snapshot = runner.snapshot_all().ok();

        Ok(ReplayResult {
            ticks_executed: runner.tick() - start_tick,
            oracle_report,
            serial_output,
            events,
            final_snapshot,
        })
    }

    /// Replay with a memory modification at the checkpoint.
    pub fn replay_with_modification(
        &self,
        checkpoint_id: u64,
        _modifications: Vec<MemoryModification>,
        ticks: u64,
    ) -> Result<ReplayResult, ReplayError> {
        // For now, just replay without modifications (would need VM memory access)
        log::warn!("Memory modifications not yet implemented, replaying without changes");
        self.replay_from(Some(checkpoint_id), ticks)
    }

    /// Get the recording info.
    pub fn recording(&self) -> &Recording {
        &self.recording
    }
}

/// Memory modification for counterfactual replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryModification {
    /// Which VM to modify.
    pub vm_index: usize,
    /// Guest physical address.
    pub address: u64,
    /// New bytes to write.
    pub data: Vec<u8>,
}

/// Result of a replay operation.
#[derive(Debug, Clone)]
pub struct ReplayResult {
    /// Number of ticks executed during replay.
    pub ticks_executed: u64,
    /// Oracle report at the end.
    pub oracle_report: OracleReport,
    /// Serial output captured during replay.
    pub serial_output: Vec<String>,
    /// Events that occurred during replay.
    pub events: Vec<RecordedEvent>,
    /// Final snapshot (if available).
    pub final_snapshot: Option<SimulationSnapshot>,
}

/// Helper to extract tick from an event.
fn event_tick(event: &RecordedEvent) -> u64 {
    match event {
        RecordedEvent::FaultFired { tick, .. } => *tick,
        RecordedEvent::AssertionHit { tick, .. } => *tick,
        RecordedEvent::VmStatusChange { tick, .. } => *tick,
        RecordedEvent::SerialOutput { tick, .. } => *tick,
        RecordedEvent::BugDetected { tick, .. } => *tick,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::{Checkpoint, CheckpointStore};
    use chaoscontrol_fault::engine::EngineSnapshot;
    use chaoscontrol_vmm::controller::NetworkFabric;
    use rand::SeedableRng;

    // Mock simulation runner for testing
    struct MockRunner {
        tick: u64,
        max_ticks: u64,
    }

    impl SimulationRunner for MockRunner {
        fn create(
            _config: &RecordingConfig,
            _schedule: FaultSchedule,
            _seed: u64,
        ) -> Result<Self, Box<dyn std::error::Error>> {
            Ok(Self {
                tick: 0,
                max_ticks: 1000,
            })
        }

        fn step_round(&mut self) -> Result<RoundResult, Box<dyn std::error::Error>> {
            self.tick += 1;
            Ok(RoundResult {
                tick: self.tick,
                vms_running: if self.tick < self.max_ticks { 2 } else { 0 },
                vms_halted: 0,
                faults_fired: vec![],
                messages_delivered: 0,
            })
        }

        fn snapshot_all(&self) -> Result<SimulationSnapshot, Box<dyn std::error::Error>> {
            use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};

            let engine = FaultEngine::new(EngineConfig::default());

            Ok(SimulationSnapshot {
                tick: self.tick,
                vm_snapshots: vec![],
                network_state: NetworkFabric {
                    partitions: vec![],
                    latency: vec![0; 2],
                    jitter: vec![0; 2],
                    bandwidth_bps: vec![0; 2],
                    next_free_tick: vec![0; 2],
                    in_flight: vec![],
                    loss_rate_ppm: vec![],
                    corruption_rate_ppm: vec![],
                    reorder_window: vec![],
                    duplicate_rate_ppm: vec![],
                    rng: rand_chacha::ChaCha20Rng::seed_from_u64(42),
                },
                fault_engine_snapshot: engine.snapshot(),
            })
        }

        fn restore_all(
            &mut self,
            snapshot: &SimulationSnapshot,
        ) -> Result<(), Box<dyn std::error::Error>> {
            self.tick = snapshot.tick;
            Ok(())
        }

        fn tick(&self) -> u64 {
            self.tick
        }

        fn report(&self) -> OracleReport {
            OracleReport {
                assertions: std::collections::BTreeMap::new(),
                total_runs: 1,
                passed: 0,
                failed: 0,
                unexercised: 0,
                events: vec![],
            }
        }

        fn serial_output(&self, _vm_index: usize) -> String {
            String::new()
        }
    }

    fn test_recording() -> Recording {
        use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};

        let mut checkpoints = CheckpointStore::new();
        let engine = FaultEngine::new(EngineConfig::default());

        // Add checkpoint at tick 500
        checkpoints.push(Checkpoint {
            id: 0,
            tick: 500,
            snapshot: Some(SimulationSnapshot {
                tick: 500,
                vm_snapshots: vec![],
                network_state: NetworkFabric {
                    partitions: vec![],
                    latency: vec![0; 2],
                    jitter: vec![0; 2],
                    bandwidth_bps: vec![0; 2],
                    next_free_tick: vec![0; 2],
                    in_flight: vec![],
                    loss_rate_ppm: vec![],
                    corruption_rate_ppm: vec![],
                    reorder_window: vec![],
                    duplicate_rate_ppm: vec![],
                    rng: rand_chacha::ChaCha20Rng::seed_from_u64(42),
                },
                fault_engine_snapshot: engine.snapshot(),
            }),
            serial_output: vec![],
            events_since_last: vec![],
        });

        Recording {
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
            checkpoints,
            schedule: FaultSchedule::new(),
            seed: 42,
            events: vec![],
            oracle_report: None,
            total_ticks: 1000,
        }
    }

    #[test]
    fn test_replay_engine_new() {
        let recording = test_recording();
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);
        assert_eq!(engine.recording.seed, 42);
    }

    #[test]
    fn test_replay_from_beginning() {
        let recording = test_recording();
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        let result = engine.replay_from(None, 100).unwrap();
        assert_eq!(result.ticks_executed, 100);
    }

    #[test]
    fn test_replay_from_checkpoint() {
        let recording = test_recording();
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        let result = engine.replay_from(Some(0), 100).unwrap();
        // Should start from checkpoint at tick 500, run for 100 more
        assert_eq!(result.ticks_executed, 100);
    }

    #[test]
    fn test_replay_checkpoint_not_found() {
        let recording = test_recording();
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        let result = engine.replay_from(Some(999), 100);
        assert!(matches!(result, Err(ReplayError::CheckpointNotFound(999))));
    }

    #[test]
    fn test_replay_with_schedule() {
        let recording = test_recording();
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        let new_schedule = FaultSchedule::new();
        let result = engine.replay_with_schedule(0, new_schedule, 50).unwrap();
        assert_eq!(result.ticks_executed, 50);
    }

    #[test]
    fn test_memory_modification() {
        let mod1 = MemoryModification {
            vm_index: 0,
            address: 0x1000,
            data: vec![0x42, 0x43, 0x44],
        };

        assert_eq!(mod1.vm_index, 0);
        assert_eq!(mod1.address, 0x1000);
        assert_eq!(mod1.data.len(), 3);
    }
}
