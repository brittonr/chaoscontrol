//! Replay engine — restores and replays from recording.

use crate::recording::{RecordedEvent, Recording, RecordingConfig};
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::controller::{
    RoundResult, SimulationConfig, SimulationController, SimulationSnapshot,
};
pub use chaoscontrol_vmm::registers::{RegisterModification, RegisterState};
use chaoscontrol_vmm::vm::VmError;
use serde::{Deserialize, Serialize};
use snafu::Snafu;

/// Errors that can occur during replay.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum ReplayError {
    #[snafu(display("VM error"), context(false))]
    Vm { source: VmError },
    #[snafu(display("Checkpoint not found: {id}"))]
    CheckpointNotFound { id: u64 },
    #[snafu(display("Invalid replay state: {message}"))]
    InvalidState { message: String },
    #[snafu(display("Simulation runner error: {message}"))]
    Runner { message: String },
}

/// Trait for running simulations (allows mocking in tests).
pub trait SimulationRunner {
    /// Create a new simulation from config.
    fn create(
        config: &RecordingConfig,
        schedule: FaultSchedule,
        seed: u64,
    ) -> Result<Self, ReplayError>
    where
        Self: Sized;

    /// Step the simulation by one round.
    fn step_round(&mut self) -> Result<RoundResult, ReplayError>;

    /// Take a snapshot of all VMs.
    fn snapshot_all(&self) -> Result<SimulationSnapshot, ReplayError>;

    /// Restore all VMs from a snapshot.
    fn restore_all(&mut self, snapshot: &SimulationSnapshot) -> Result<(), ReplayError>;

    /// Get current simulation tick.
    fn tick(&self) -> u64;

    /// Get the oracle report.
    fn report(&self) -> OracleReport;

    /// Get serial output for a specific VM.
    fn serial_output(&self, vm_index: usize) -> String;

    /// Read bytes from guest physical memory.
    fn read_memory(&self, vm_index: usize, addr: u64, size: usize) -> Result<Vec<u8>, ReplayError>;

    /// Write bytes to guest physical memory.
    fn write_memory(&mut self, vm_index: usize, addr: u64, data: &[u8]) -> Result<(), ReplayError>;

    /// Read a vCPU's register state.
    fn read_registers(&self, vm_index: usize, vcpu: usize) -> Result<RegisterState, ReplayError>;

    /// Set a vCPU's register state.
    fn set_registers(
        &mut self,
        vm_index: usize,
        vcpu: usize,
        state: &RegisterState,
    ) -> Result<(), ReplayError>;
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
    ) -> Result<Self, ReplayError> {
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
            disk_image_path: config.disk_image_path.clone(),
            base_core: None,
            dlog_dir: None,
            bootstrap_budget: None,
        };

        let controller = SimulationController::new(sim_config)?;
        Ok(Self { controller })
    }

    fn step_round(&mut self) -> Result<RoundResult, ReplayError> {
        Ok(self.controller.step_round()?)
    }

    fn snapshot_all(&self) -> Result<SimulationSnapshot, ReplayError> {
        Ok(self.controller.snapshot_all()?)
    }

    fn restore_all(&mut self, snapshot: &SimulationSnapshot) -> Result<(), ReplayError> {
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

    fn read_memory(&self, vm_index: usize, addr: u64, size: usize) -> Result<Vec<u8>, ReplayError> {
        Ok(self.controller.vm(vm_index).read_guest_memory(addr, size)?)
    }

    fn write_memory(&mut self, vm_index: usize, addr: u64, data: &[u8]) -> Result<(), ReplayError> {
        Ok(self
            .controller
            .vm(vm_index)
            .write_guest_memory(addr, data)?)
    }

    fn read_registers(&self, vm_index: usize, vcpu: usize) -> Result<RegisterState, ReplayError> {
        Ok(self.controller.vm(vm_index).read_vcpu_registers(vcpu)?)
    }

    fn set_registers(
        &mut self,
        vm_index: usize,
        vcpu: usize,
        state: &RegisterState,
    ) -> Result<(), ReplayError> {
        Ok(self
            .controller
            .vm_mut(vm_index)
            .set_vcpu_registers(vcpu, state)?)
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
        .map_err(|e| {
            RunnerSnafu {
                message: e.to_string(),
            }
            .build()
        })?;

        let start_tick = if let Some(cp_id) = checkpoint_id {
            let checkpoint = self
                .recording
                .checkpoints
                .get(cp_id)
                .ok_or(CheckpointNotFoundSnafu { id: cp_id }.build())?;

            if let Some(snapshot) = &checkpoint.snapshot {
                runner.restore_all(snapshot).map_err(|e| {
                    RunnerSnafu {
                        message: e.to_string(),
                    }
                    .build()
                })?;
                checkpoint.tick
            } else {
                return InvalidStateSnafu {
                    message: format!("Checkpoint {} has no snapshot", cp_id),
                }
                .fail();
            }
        } else {
            0
        };

        let target_tick = start_tick + ticks;
        let mut events = Vec::new();

        while runner.tick() < target_tick {
            let result = runner.step_round().map_err(|e| {
                RunnerSnafu {
                    message: e.to_string(),
                }
                .build()
            })?;

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
            .ok_or(CheckpointNotFoundSnafu { id: checkpoint_id }.build())?;

        let snapshot = checkpoint.snapshot.as_ref().ok_or_else(|| {
            InvalidStateSnafu {
                message: format!("Checkpoint {} has no snapshot", checkpoint_id),
            }
            .build()
        })?;

        let mut runner = R::create(
            &self.recording.config,
            modified_schedule,
            self.recording.seed,
        )
        .map_err(|e| {
            RunnerSnafu {
                message: e.to_string(),
            }
            .build()
        })?;

        runner.restore_all(snapshot).map_err(|e| {
            RunnerSnafu {
                message: e.to_string(),
            }
            .build()
        })?;

        let start_tick = checkpoint.tick;
        let target_tick = start_tick + ticks;
        let events = Vec::new(); // Events not collected in counterfactual replay

        while runner.tick() < target_tick {
            let result = runner.step_round().map_err(|e| {
                RunnerSnafu {
                    message: e.to_string(),
                }
                .build()
            })?;

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

    /// Replay with memory and/or register modifications at the checkpoint.
    ///
    /// Restores the checkpoint, applies all modifications (memory first,
    /// then registers), and runs for `ticks` ticks.
    pub fn replay_with_modification(
        &self,
        checkpoint_id: u64,
        memory_mods: Vec<MemoryModification>,
        register_mods: Vec<RegisterModification>,
        ticks: u64,
    ) -> Result<ReplayResult, ReplayError> {
        let checkpoint = self
            .recording
            .checkpoints
            .get(checkpoint_id)
            .ok_or(CheckpointNotFoundSnafu { id: checkpoint_id }.build())?;

        let snapshot = checkpoint.snapshot.as_ref().ok_or_else(|| {
            InvalidStateSnafu {
                message: format!("Checkpoint {} has no snapshot", checkpoint_id),
            }
            .build()
        })?;

        let mut runner = R::create(
            &self.recording.config,
            self.recording.schedule.clone(),
            self.recording.seed,
        )?;

        runner.restore_all(snapshot)?;

        // Apply memory modifications first.
        for mem_mod in &memory_mods {
            runner.write_memory(mem_mod.vm_index, mem_mod.address, &mem_mod.data)?;
        }

        // Apply register modifications second.
        for reg_mod in &register_mods {
            let mut state = runner.read_registers(reg_mod.vm_index, reg_mod.vcpu)?;
            for (reg, value) in &reg_mod.changes {
                reg.set(&mut state, *value);
            }
            runner.set_registers(reg_mod.vm_index, reg_mod.vcpu, &state)?;
        }

        let start_tick = checkpoint.tick;
        let target_tick = start_tick + ticks;

        while runner.tick() < target_tick {
            let result = runner.step_round()?;
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
            events: Vec::new(),
            final_snapshot,
        })
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
        ) -> Result<Self, ReplayError> {
            Ok(Self {
                tick: 0,
                max_ticks: 1000,
            })
        }

        fn step_round(&mut self) -> Result<RoundResult, ReplayError> {
            self.tick += 1;
            Ok(RoundResult {
                tick: self.tick,
                vms_running: if self.tick < self.max_ticks { 2 } else { 0 },
                vms_halted: 0,
                faults_fired: vec![],
                messages_delivered: 0,
            })
        }

        fn snapshot_all(&self) -> Result<SimulationSnapshot, ReplayError> {
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
                    packet_in_flight: vec![],
                    loss_rate_ppm: vec![],
                    corruption_rate_ppm: vec![],
                    reorder_window: vec![],
                    duplicate_rate_ppm: vec![],
                    rng: rand_chacha::ChaCha20Rng::seed_from_u64(42),
                    stats: Default::default(),
                },
                fault_engine_snapshot: engine.snapshot(),
                vcpu_stall_until: vec![],
                clock_freeze: vec![],
                clock_jitter_bound: vec![],
            })
        }

        fn restore_all(&mut self, snapshot: &SimulationSnapshot) -> Result<(), ReplayError> {
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
                catalog_size: 0,
                events: vec![],
            }
        }

        fn serial_output(&self, _vm_index: usize) -> String {
            String::new()
        }

        fn read_memory(
            &self,
            _vm_index: usize,
            _addr: u64,
            size: usize,
        ) -> Result<Vec<u8>, ReplayError> {
            Ok(vec![0xBB; size])
        }

        fn write_memory(
            &mut self,
            _vm_index: usize,
            _addr: u64,
            _data: &[u8],
        ) -> Result<(), ReplayError> {
            Ok(())
        }

        fn read_registers(
            &self,
            _vm_index: usize,
            _vcpu: usize,
        ) -> Result<RegisterState, ReplayError> {
            Ok(RegisterState {
                rip: 0x1000,
                rsp: 0x2000,
                rax: 0,
                rbx: 0,
                rcx: 0,
                rdx: 0,
                rsi: 0,
                rdi: 0,
                rbp: 0,
                r8: 0,
                r9: 0,
                r10: 0,
                r11: 0,
                r12: 0,
                r13: 0,
                r14: 0,
                r15: 0,
                rflags: 0x202,
                cs: 0,
                ss: 0,
                ds: 0,
                es: 0,
                fs: 0,
                gs: 0,
                cr0: 0,
                cr3: 0,
                cr4: 0,
            })
        }

        fn set_registers(
            &mut self,
            _vm_index: usize,
            _vcpu: usize,
            _state: &RegisterState,
        ) -> Result<(), ReplayError> {
            Ok(())
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
                    packet_in_flight: vec![],
                    loss_rate_ppm: vec![],
                    corruption_rate_ppm: vec![],
                    reorder_window: vec![],
                    duplicate_rate_ppm: vec![],
                    rng: rand_chacha::ChaCha20Rng::seed_from_u64(42),
                    stats: Default::default(),
                },
                fault_engine_snapshot: engine.snapshot(),
                vcpu_stall_until: vec![],
                clock_freeze: vec![],
                clock_jitter_bound: vec![],
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
                disk_image_path: None,
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
        assert!(matches!(
            result,
            Err(ReplayError::CheckpointNotFound { id: 999 })
        ));
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
