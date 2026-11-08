//! Replay engine — restores and replays from recording.

use crate::recording::{validate_recording, RecordedEvent, Recording, RecordingConfig};
use crate::session::ExactSession;
use chaoscontrol_fault::evidence::{FaultControlEvent, FaultControlKind};
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::outcomes::{
    transition_fault_outcome, validate_fault_outcome_ledger, FaultOutcomeLedger, FaultStageEvent,
    FaultStageKind,
};
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::controller::{
    RoundResult, SimulationController, SimulationSnapshot, VmScheduleTrace,
};
pub use chaoscontrol_vmm::registers::{RegisterModification, RegisterState};
use chaoscontrol_vmm::vm::VmError;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::sync::Arc;

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
        run_sequence: u64,
    ) -> Result<Self, ReplayError>
    where
        Self: Sized;

    /// Construct from a fully admitted session, never from partial metadata.
    fn create_from_session(session: &ExactSession, checkpoint_id: u64) -> Result<Self, ReplayError>
    where
        Self: Sized,
    {
        session
            .require_execution_authorization()
            .map_err(|error| ReplayError::InvalidState {
                message: error.to_string(),
            })?;
        let recording = session.recording();
        let checkpoint = recording
            .checkpoints
            .get(checkpoint_id)
            .ok_or(ReplayError::CheckpointNotFound { id: checkpoint_id })?;
        let snapshot = validated_checkpoint_snapshot(recording, checkpoint)?;
        let mut runner = Self::create(
            &recording.config,
            recording.schedule.clone(),
            recording.seed,
            recording.fault_run_sequence,
        )?;
        runner.restore_all(snapshot)?;
        Ok(runner)
    }

    /// Start one clean counterfactual fault run after a restored checkpoint.
    fn begin_counterfactual_fault_run(
        &mut self,
        schedule: FaultSchedule,
    ) -> Result<(), ReplayError>;

    /// Step the simulation by one round.
    fn step_round(&mut self) -> Result<RoundResult, ReplayError>;

    /// Finish pending host-exit bookkeeping at the snapshot observation boundary.
    /// Must not execute another guest instruction, advance ticks or SMP evidence,
    /// or allocate/copy a snapshot. Abstract runners must already be settled.
    fn settle_exact_observation(&mut self) -> Result<(), ReplayError>;

    /// Cumulative pause admission and suppression facts, including restored facts.
    fn fault_control_events(&self) -> &[FaultControlEvent];

    /// Replay one recorded external request through the normal admission path.
    fn replay_pause_request(
        &mut self,
        request: chaoscontrol_fault::mechanisms::PauseControlRequest,
    ) -> Result<(), ReplayError>;

    /// Take a snapshot of all VMs.
    fn snapshot_all(&mut self) -> Result<SimulationSnapshot, ReplayError>;

    /// Restore all VMs from a snapshot.
    fn restore_all(&mut self, snapshot: &SimulationSnapshot) -> Result<(), ReplayError>;

    /// Get current simulation tick.
    fn tick(&self) -> u64;

    /// Get the oracle report.
    fn report(&self) -> OracleReport;

    /// Get serial output for a specific VM.
    fn serial_output(&self, vm_index: usize) -> &str;

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

    /// Only an actual admitted runner can authorize the inaccessible-clock view.
    /// Retained descriptors and abstract mechanism fixtures cannot grant it.
    fn coarse_clock_projection_admitted(&self) -> bool {
        false
    }
}

/// Real implementation using SimulationController.
pub struct RealSimulationRunner {
    controller: SimulationController,
    /// Keep sealed guest-input capabilities alive for the complete attempt.
    _artifacts: Vec<std::fs::File>,
    deadline: Option<std::time::Instant>,
    serial_output: Vec<String>,
    serial_bytes: u64,
    serial_limit: u64,
}

#[allow(
    ambient_clock,
    reason = "host watchdog shell only; never deterministic guest state"
)]
fn host_watchdog_now() -> std::time::Instant {
    std::time::Instant::now()
}

impl RealSimulationRunner {
    pub(crate) fn from_controller(controller: SimulationController) -> Self {
        let serial_output = vec![String::new(); controller.num_vms()];
        Self {
            controller,
            _artifacts: Vec::new(),
            deadline: None,
            serial_output,
            serial_bytes: 0,
            serial_limit: crate::session::SessionLimits::default().max_output_bytes,
        }
    }
    pub(crate) fn into_controller(self) -> SimulationController {
        self.controller
    }
    pub(crate) fn from_restored_controller(
        controller: SimulationController,
        artifacts: Vec<std::fs::File>,
    ) -> Self {
        let serial_output = vec![String::new(); controller.num_vms()];
        Self {
            controller,
            _artifacts: artifacts,
            deadline: None,
            serial_output,
            serial_bytes: 0,
            serial_limit: crate::session::SessionLimits::default().max_output_bytes,
        }
    }
    pub(crate) fn set_output_limit(&mut self, bytes: u64) -> Result<(), ReplayError> {
        self.ensure_healthy()?;
        if bytes == 0
            || bytes > crate::session::SessionLimits::default().max_output_bytes
            || bytes < self.serial_bytes
        {
            return Err(ReplayError::InvalidState {
                message: "invalid actual output byte bound".into(),
            });
        }
        self.serial_limit = bytes;
        Ok(())
    }

    fn collect_serial_output(&mut self) -> Result<(), ReplayError> {
        for vm_index in 0..self.controller.num_vms() {
            let vm = self.controller.vm_mut(vm_index);
            if vm.serial_dropped_byte_count() != 0 {
                return Err(ReplayError::Runner {
                    message: "guest serial capture dropped bytes; continuation cannot be verified"
                        .into(),
                });
            }
            let text = vm.take_serial_output();
            self.serial_bytes = self
                .serial_bytes
                .checked_add(text.len() as u64)
                .ok_or_else(|| ReplayError::Runner {
                    message: "actual guest output byte accounting overflow".into(),
                })?;
            if self.serial_bytes > self.serial_limit {
                return Err(ReplayError::Runner {
                    message: "actual guest output exceeds admitted byte bound".into(),
                });
            }
            if self.serial_output[vm_index].is_empty() {
                self.serial_output[vm_index] = text;
            } else {
                self.serial_output[vm_index].push_str(&text);
            }
        }
        Ok(())
    }

    pub(crate) fn set_execution_deadline(&mut self, milliseconds: u64) -> Result<(), ReplayError> {
        self.ensure_healthy()?;
        self.deadline = Some(
            host_watchdog_now()
                .checked_add(std::time::Duration::from_millis(milliseconds))
                .ok_or_else(|| ReplayError::InvalidState {
                    message: "host execution deadline overflow".into(),
                })?,
        );
        Ok(())
    }

    fn check_deadline(&self) -> Result<(), ReplayError> {
        self.ensure_healthy()?;
        if self
            .deadline
            .is_some_and(|deadline| host_watchdog_now() >= deadline)
        {
            return Err(ReplayError::Runner {
                message: "exact replay host execution deadline exhausted (operational failure)"
                    .into(),
            });
        }
        Ok(())
    }

    fn validate_vm(&self, vm_index: usize, vcpu: Option<usize>) -> Result<(), ReplayError> {
        self.ensure_healthy()?;
        if vm_index >= self.controller.num_vms()
            || vcpu.is_some_and(|vcpu| vcpu >= self.controller.config().vm_config.num_vcpus)
        {
            return Err(ReplayError::InvalidState {
                message: "VM/vCPU observation index is outside the admitted topology".into(),
            });
        }
        Ok(())
    }

    fn ensure_healthy(&self) -> Result<(), ReplayError> {
        if self.controller.round_poison().is_some() {
            return Err(ReplayError::Runner { message: "replay runner is permanently poisoned; a fresh admitted checkpoint and runner are required".into() });
        }
        Ok(())
    }

    fn abort_attempt(&mut self, error: ReplayError) -> ReplayError {
        ReplayError::Vm {
            source: self.controller.poison_execution_exchange(error.to_string()),
        }
    }
}

impl SimulationRunner for RealSimulationRunner {
    fn create(
        _config: &RecordingConfig,
        _schedule: FaultSchedule,
        _seed: u64,
        _run_sequence: u64,
    ) -> Result<Self, ReplayError> {
        Err(ReplayError::InvalidState {
            message: "exact replay requires a persisted ExactSession; metadata cannot reconstruct deterministic inputs or boot a replacement".into(),
        })
    }

    fn create_from_session(
        session: &ExactSession,
        checkpoint_id: u64,
    ) -> Result<Self, ReplayError> {
        let mut runner = session
            .restore_controller(checkpoint_id)
            .map(|restored| restored.into_runner())
            .map_err(|error| ReplayError::InvalidState {
                message: error.to_string(),
            })?;
        runner.set_execution_deadline(session.host_deadline_ms())?;
        runner.set_output_limit(session.limits().max_output_bytes)?;
        Ok(runner)
    }

    fn begin_counterfactual_fault_run(
        &mut self,
        schedule: FaultSchedule,
    ) -> Result<(), ReplayError> {
        self.ensure_healthy()?;
        self.controller.begin_counterfactual_fault_run(schedule)?;
        Ok(())
    }

    fn step_round(&mut self) -> Result<RoundResult, ReplayError> {
        let attempt = (|| {
            self.check_deadline()?;
            let result = self.controller.step_round()?;
            self.check_deadline()?;
            self.collect_serial_output()?;
            Ok(result)
        })();
        attempt.map_err(|error| self.abort_attempt(error))
    }

    fn fault_control_events(&self) -> &[FaultControlEvent] {
        &self.controller.fault_outcomes().control_events
    }

    fn replay_pause_request(
        &mut self,
        request: chaoscontrol_fault::mechanisms::PauseControlRequest,
    ) -> Result<(), ReplayError> {
        self.ensure_healthy()?;
        // Both admission and typed rejection are retained in control_events and
        // compared with the recording immediately after dispatch.
        let _decision = self.controller.request_pause_control(request);
        Ok(())
    }

    fn settle_exact_observation(&mut self) -> Result<(), ReplayError> {
        let attempt = (|| {
            self.check_deadline()?;
            self.controller.settle_exact_observation()?;
            self.check_deadline()
        })();
        attempt.map_err(|error| self.abort_attempt(error))
    }

    fn snapshot_all(&mut self) -> Result<SimulationSnapshot, ReplayError> {
        self.ensure_healthy()?;
        self.controller
            .snapshot_all()
            .map_err(|error| self.abort_attempt(error.into()))
    }

    fn restore_all(&mut self, snapshot: &SimulationSnapshot) -> Result<(), ReplayError> {
        self.ensure_healthy()?;
        self.controller.restore_all(snapshot)?;
        for output in &mut self.serial_output {
            output.clear();
        }
        self.serial_bytes = 0;
        Ok(())
    }

    fn tick(&self) -> u64 {
        self.controller.tick()
    }

    fn report(&self) -> OracleReport {
        self.controller.report()
    }

    fn serial_output(&self, vm_index: usize) -> &str {
        self.serial_output.get(vm_index).map_or("", String::as_str)
    }

    fn coarse_clock_projection_admitted(&self) -> bool {
        self.controller.round_poison().is_none()
            && (0..self.controller.num_vms())
                .all(|vm| self.controller.vm(vm).coarse_clock_projection_admitted())
    }

    fn read_memory(&self, vm_index: usize, addr: u64, size: usize) -> Result<Vec<u8>, ReplayError> {
        self.validate_vm(vm_index, None)?;
        Ok(self.controller.vm(vm_index).read_guest_memory(addr, size)?)
    }

    fn write_memory(&mut self, vm_index: usize, addr: u64, data: &[u8]) -> Result<(), ReplayError> {
        self.validate_vm(vm_index, None)?;
        Ok(self
            .controller
            .vm(vm_index)
            .write_guest_memory(addr, data)?)
    }

    fn read_registers(&self, vm_index: usize, vcpu: usize) -> Result<RegisterState, ReplayError> {
        self.validate_vm(vm_index, Some(vcpu))?;
        Ok(self.controller.vm(vm_index).read_vcpu_registers(vcpu)?)
    }

    fn set_registers(
        &mut self,
        vm_index: usize,
        vcpu: usize,
        state: &RegisterState,
    ) -> Result<(), ReplayError> {
        self.validate_vm(vm_index, Some(vcpu))?;
        Ok(self
            .controller
            .vm_mut(vm_index)
            .set_vcpu_registers(vcpu, state)?)
    }
}

/// Share admitted RAM rather than cloning the checkpoint inventory per consumer.
#[derive(Clone)]
pub(crate) enum ReplayRecording {
    Memory(Arc<Recording>),
    Session(Arc<ExactSession>),
}
impl ReplayRecording {
    fn session(&self) -> Option<&ExactSession> {
        match self {
            Self::Session(session) => Some(session),
            Self::Memory(_) => None,
        }
    }
}
impl std::ops::Deref for ReplayRecording {
    type Target = Recording;
    fn deref(&self) -> &Recording {
        match self {
            Self::Memory(recording) => recording,
            Self::Session(session) => session.recording(),
        }
    }
}

/// Replays a recording from any checkpoint.
pub struct ReplayEngine<R: SimulationRunner = RealSimulationRunner> {
    /// The recording to replay.
    recording: ReplayRecording,
    /// Phantom data for the runner type.
    _runner: std::marker::PhantomData<R>,
}

impl<R: SimulationRunner> ReplayEngine<R> {
    /// Create a new replay engine from a recording.
    pub fn new(recording: Recording) -> Self {
        Self {
            recording: ReplayRecording::Memory(Arc::new(recording)),
            _runner: std::marker::PhantomData,
        }
    }

    /// The production intake: complete persisted deterministic inputs.
    pub fn from_session(session: Arc<ExactSession>) -> Self {
        Self {
            recording: ReplayRecording::Session(session),
            _runner: std::marker::PhantomData,
        }
    }

    pub(crate) fn validate_target(&self, tick: u64) -> Result<(), ReplayError> {
        checked_recorded_target_tick(&self.recording, 0, tick)?;
        if let Some(session) = self.recording.session() {
            session
                .validate_target(tick)
                .map_err(|error| ReplayError::InvalidState {
                    message: error.to_string(),
                })?;
        }
        Ok(())
    }

    pub(crate) fn start_tick(&self) -> u64 {
        self.recording
            .session()
            .map_or(0, |session| session.manifest().retained_start_tick)
    }

    fn restored_runner(&self, checkpoint_id: Option<u64>) -> Result<R, ReplayError> {
        if let Some(session) = self.recording.session() {
            session.require_execution_authorization().map_err(|error| {
                ReplayError::InvalidState {
                    message: error.to_string(),
                }
            })?;
            let id = checkpoint_id.ok_or_else(|| ReplayError::InvalidState {
                message: "exact session has no checkpoint at or before target".into(),
            })?;
            let runner = R::create_from_session(session, id)?;
            let expected = self
                .recording
                .checkpoints
                .get(id)
                .ok_or(ReplayError::CheckpointNotFound { id })?
                .tick;
            if runner.tick() != expected {
                return Err(ReplayError::InvalidState {
                    message: "restored runner tick differs from the admitted checkpoint".into(),
                });
            }
            return Ok(runner);
        }
        let snapshot = checkpoint_id
            .map(|id| {
                let checkpoint = self
                    .recording
                    .checkpoints
                    .get(id)
                    .ok_or(ReplayError::CheckpointNotFound { id })?;
                validated_checkpoint_snapshot(&self.recording, checkpoint)
            })
            .transpose()?;
        let mut runner = R::create(
            &self.recording.config,
            self.recording.schedule.clone(),
            self.recording.seed,
            self.recording.fault_run_sequence,
        )?;
        if let Some(snapshot) = snapshot {
            runner.restore_all(snapshot)?;
            if runner.tick() != snapshot.tick {
                return Err(ReplayError::InvalidState {
                    message: "restored runner tick differs from checkpoint".into(),
                });
            }
        }
        Ok(runner)
    }

    /// Failed attempts are dropped, never inspected or reused after poison.
    pub(crate) fn seek(&self, tick: u64) -> Result<(R, Option<u64>), ReplayError> {
        self.validate_replay_recording()?;
        self.validate_target(tick)?;
        let checkpoint = self
            .recording
            .checkpoints
            .all()
            .iter()
            .filter(|checkpoint| checkpoint.tick <= tick)
            .max_by_key(|checkpoint| (checkpoint.tick, checkpoint.id));
        let id = checkpoint.map(|checkpoint| checkpoint.id);
        let mut runner = self.restored_runner(id)?;
        advance_exact_runner(&self.recording, &mut runner, tick)?;
        Ok((runner, id))
    }

    /// Replay from the beginning (or a specific checkpoint) for N ticks.
    pub fn replay_from(
        &self,
        checkpoint_id: Option<u64>,
        ticks: u64,
    ) -> Result<ReplayResult, ReplayError> {
        self.validate_replay_recording()?;
        let checkpoint_id = checkpoint_id.or_else(|| {
            self.recording.session().and_then(|_| {
                self.recording
                    .checkpoints
                    .all()
                    .iter()
                    .filter(|checkpoint| checkpoint.tick == self.start_tick())
                    .max_by_key(|checkpoint| (checkpoint.tick, checkpoint.id))
                    .map(|checkpoint| checkpoint.id)
            })
        });
        let start_tick = checkpoint_id
            .map(|id| {
                self.recording
                    .checkpoints
                    .get(id)
                    .map(|checkpoint| checkpoint.tick)
                    .ok_or(ReplayError::CheckpointNotFound { id })
            })
            .transpose()?
            .unwrap_or(0);
        let target_tick = checked_recorded_target_tick(&self.recording, start_tick, ticks)?;
        self.validate_target(target_tick)?;
        let mut runner = self.restored_runner(checkpoint_id)?;
        let (events, fault_outcomes) =
            advance_exact_runner(&self.recording, &mut runner, target_tick)?;
        let oracle_report = runner.report();
        let serial_output: Vec<String> = (0..self.recording.config.num_vms)
            .map(|i| runner.serial_output(i).to_owned())
            .collect();

        let final_snapshot = Some(runner.snapshot_all()?);

        Ok(ReplayResult {
            ticks_executed: runner.tick() - start_tick,
            oracle_report,
            serial_output,
            events,
            fault_outcomes,
            final_snapshot,
            counterfactual: false,
        })
    }

    /// Replay with a modified fault schedule (counterfactual).
    pub fn replay_with_schedule(
        &self,
        checkpoint_id: u64,
        modified_schedule: FaultSchedule,
        ticks: u64,
    ) -> Result<ReplayResult, ReplayError> {
        self.validate_replay_recording()?;
        let checkpoint = self
            .recording
            .checkpoints
            .get(checkpoint_id)
            .ok_or(CheckpointNotFoundSnafu { id: checkpoint_id }.build())?;
        let start_tick = checkpoint.tick;
        let target_tick = checked_target_tick(start_tick, ticks)?;
        self.validate_target(target_tick)?;

        let mut runner = self.restored_runner(Some(checkpoint_id))?;
        runner.begin_counterfactual_fault_run(modified_schedule)?;

        let events = Vec::new(); // Events not collected in counterfactual replay
        let mut fault_outcomes = Vec::new();

        while runner.tick() < target_tick {
            let result = runner.step_round().map_err(|e| {
                RunnerSnafu {
                    message: e.to_string(),
                }
                .build()
            })?;
            fault_outcomes.extend(result.fault_outcomes.iter().cloned());

            if result.vms_running == 0 {
                break;
            }
        }

        let oracle_report = runner.report();
        let serial_output: Vec<String> = (0..self.recording.config.num_vms)
            .map(|i| runner.serial_output(i).to_owned())
            .collect();

        let final_snapshot = Some(runner.snapshot_all()?);

        Ok(ReplayResult {
            ticks_executed: runner.tick() - start_tick,
            oracle_report,
            serial_output,
            events,
            fault_outcomes,
            final_snapshot,
            counterfactual: true,
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
        self.validate_replay_recording()?;
        let checkpoint = self
            .recording
            .checkpoints
            .get(checkpoint_id)
            .ok_or(CheckpointNotFoundSnafu { id: checkpoint_id }.build())?;
        let start_tick = checkpoint.tick;
        let target_tick = checked_target_tick(start_tick, ticks)?;
        self.validate_target(target_tick)?;

        let mut runner = self.restored_runner(Some(checkpoint_id))?;
        runner.begin_counterfactual_fault_run(self.recording.schedule.clone())?;

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

        let mut fault_outcomes = Vec::new();

        while runner.tick() < target_tick {
            let result = runner.step_round()?;
            fault_outcomes.extend(result.fault_outcomes.iter().cloned());
            if result.vms_running == 0 {
                break;
            }
        }

        let oracle_report = runner.report();
        let serial_output: Vec<String> = (0..self.recording.config.num_vms)
            .map(|i| runner.serial_output(i).to_owned())
            .collect();

        let final_snapshot = Some(runner.snapshot_all()?);

        Ok(ReplayResult {
            ticks_executed: runner.tick() - start_tick,
            oracle_report,
            serial_output,
            events: Vec::new(),
            fault_outcomes,
            final_snapshot,
            counterfactual: true,
        })
    }

    /// Get the recording info.
    pub fn recording(&self) -> &Recording {
        &self.recording
    }

    pub(crate) fn recording_source(&self) -> ReplayRecording {
        self.recording.clone()
    }

    fn validate_replay_recording(&self) -> Result<(), ReplayError> {
        validate_recording(&self.recording).map_err(|error| ReplayError::InvalidState {
            message: format!("invalid recorded fault trace: {error:?}"),
        })
    }
}

/// Advance an already-restored runner using the same exact evidence checks as
/// replay_from. It never creates a runner or falls back to boot execution.
pub(crate) fn advance_exact_runner<R: SimulationRunner>(
    recording: &Recording,
    runner: &mut R,
    target_tick: u64,
) -> Result<(Vec<RecordedEvent>, Vec<FaultStageEvent>), ReplayError> {
    let start_tick = runner.tick();
    if target_tick < start_tick {
        return Err(ReplayError::InvalidState {
            message: "exact advancement target precedes restored checkpoint".into(),
        });
    }
    checked_recorded_target_tick(recording, start_tick, target_tick - start_tick)?;
    let mut events = Vec::new();
    let mut fault_outcomes = Vec::new();
    let mut stage_event_count = recording
        .fault_round_deltas
        .iter()
        .filter(|delta| delta.tick <= start_tick)
        .map(|delta| delta.event_end)
        .next_back()
        .unwrap_or(0);
    replay_pause_requests(recording, runner, stage_event_count)?;
    validate_replayed_fault_controls(
        recording,
        runner.tick(),
        stage_event_count,
        runner.fault_control_events(),
    )?;
    validate_actual_continuation(recording, runner)?;
    while runner.tick() < target_tick {
        let expected_tick = checked_target_tick(runner.tick(), 1)?;
        let result = runner.step_round()?;
        if result.tick != expected_tick || runner.tick() != expected_tick {
            return Err(ReplayError::InvalidState {
                message: format!("exact replay round did not arrive at tick {expected_tick}"),
            });
        }
        validate_replayed_fault_round(recording, result.tick, &result.fault_outcomes)?;
        validate_replayed_schedule_round(recording, result.tick, &result.schedule_traces)?;
        validate_replayed_injection_round(recording, result.tick, &result.injection_traces)?;
        stage_event_count = stage_event_count
            .checked_add(result.fault_outcomes.len() as u64)
            .ok_or_else(|| ReplayError::InvalidState {
                message: "fault-stage count overflow".into(),
            })?;
        fault_outcomes.extend(result.fault_outcomes);
        replay_pause_requests(recording, runner, stage_event_count)?;
        validate_replayed_fault_controls(
            recording,
            runner.tick(),
            stage_event_count,
            runner.fault_control_events(),
        )?;
        validate_actual_continuation(recording, runner)?;
        events.extend(
            recording
                .events
                .iter()
                .filter(|event| event_tick(event) == runner.tick())
                .cloned(),
        );
        if result.vms_running == 0 {
            break;
        }
    }
    validate_replayed_fault_interval(recording, start_tick, target_tick, &fault_outcomes)?;
    if runner.tick() != target_tick {
        return Err(ReplayError::InvalidState {
            message: format!("exact replay stopped before tick {target_tick}"),
        });
    }
    runner.settle_exact_observation()?;
    if runner.tick() != target_tick {
        return Err(ReplayError::InvalidState {
            message: "observation settlement changed the verified target coordinate".into(),
        });
    }
    if recording.config.smp_progress_mode
        == chaoscontrol_vmm::scheduler::core::ProgressMode::KvmDebugBoundary
    {
        for vm_index in 0..recording.config.num_vms {
            validate_replayed_serial_suffix(
                recording,
                start_tick,
                target_tick,
                vm_index,
                runner.serial_output(vm_index),
            )?;
        }
    }
    Ok((events, fault_outcomes))
}

/// One replay/debugger continuation check over actual settled guest state.
/// Schedule/fault suffix checks are performed per round by the same caller.
/// Full checkpoint bytes include memory, registers, pending devices, delivery
/// ledger, scheduler and assertion state; retained expectations are never echoed.
fn validate_actual_continuation<R: SimulationRunner>(
    recording: &Recording,
    runner: &mut R,
) -> Result<(), ReplayError> {
    if recording.config.smp_progress_mode
        != chaoscontrol_vmm::scheduler::core::ProgressMode::KvmDebugBoundary
    {
        return Ok(());
    }
    let Some(checkpoint) = recording
        .checkpoints
        .all()
        .iter()
        .find(|checkpoint| checkpoint.tick == runner.tick())
    else {
        return Ok(());
    };
    validated_checkpoint_snapshot(recording, checkpoint)?;
    runner.settle_exact_observation()?;
    let actual = runner.snapshot_all()?;
    if actual.tick != runner.tick() {
        return Err(ReplayError::InvalidState {
            message: "actual snapshot did not arrive at the runner's verified tick".into(),
        });
    }
    validate_actual_snapshot_continuation(
        recording,
        &actual,
        runner.coarse_clock_projection_admitted(),
    )
}

pub(crate) fn validate_actual_snapshot_continuation(
    recording: &Recording,
    actual: &SimulationSnapshot,
    projection_admitted: bool,
) -> Result<(), ReplayError> {
    let checkpoint = recording
        .checkpoints
        .all()
        .iter()
        .find(|checkpoint| checkpoint.tick == actual.tick)
        .ok_or_else(|| ReplayError::InvalidState {
            message: "actual observation has no retained checkpoint".into(),
        })?;
    let expected = validated_checkpoint_snapshot(recording, checkpoint)?;
    if actual.vm_snapshots.len() != expected.vm_snapshots.len() {
        return Err(ReplayError::InvalidState {
            message: "actual continuation topology mismatch".into(),
        });
    }
    let mut exclude_kernel_clock = projection_admitted && !actual.vm_snapshots.is_empty();
    for ((actual_vm, _), (expected_vm, _)) in actual.vm_snapshots.iter().zip(&expected.vm_snapshots)
    {
        let actual_descriptor =
            actual_vm
                .execution_descriptor()
                .map_err(|error| ReplayError::InvalidState {
                    message: format!("actual descriptor: {error:?}"),
                })?;
        let expected_descriptor =
            expected_vm
                .execution_descriptor()
                .map_err(|error| ReplayError::InvalidState {
                    message: format!("expected descriptor: {error:?}"),
                })?;
        if actual_descriptor != expected_descriptor {
            return Err(ReplayError::InvalidState {
                message: "actual continuation execution descriptor mismatch".into(),
            });
        }
        let permits = actual_descriptor.permits_coarse_clock_projection()
            && expected_descriptor.permits_coarse_clock_projection();
        exclude_kernel_clock &= permits;
    }
    if continuation_identity(actual, exclude_kernel_clock)?
        != continuation_identity(expected, exclude_kernel_clock)?
    {
        return Err(ReplayError::InvalidState {
            message: format!(
                "actual settled continuation differs from checkpoint {} at tick {}",
                checkpoint.id, checkpoint.tick
            ),
        });
    }
    Ok(())
}

fn continuation_identity(
    snapshot: &SimulationSnapshot,
    exclude_kernel_clock: bool,
) -> Result<[u8; 32], ReplayError> {
    struct CanonicalWriter(blake3::Hasher);
    impl std::io::Write for CanonicalWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = CanonicalWriter(blake3::Hasher::new());
    writer
        .0
        .update(b"chaoscontrol.replay.actual-continuation.v3\0");
    ciborium::ser::into_writer(
        &CanonicalSimulationContinuation {
            snapshot,
            exclude_kernel_clock,
        },
        &mut writer,
    )
    .map_err(|error| ReplayError::InvalidState {
        message: format!("canonical continuation serialization: {error}"),
    })?;
    Ok(*writer.0.finalize().as_bytes())
}

/// Exhaustive borrowed projection: adding controller state requires updating this
/// serializer. The only optional omission is the admitted inaccessible KVM clock.
struct CanonicalSimulationContinuation<'a> {
    snapshot: &'a SimulationSnapshot,
    exclude_kernel_clock: bool,
}

impl Serialize for CanonicalSimulationContinuation<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let SimulationSnapshot {
            tick,
            vm_snapshots,
            network_state,
            fault_engine_snapshot,
            vcpu_stall_until,
            cpu_throttle,
            clock_freeze,
            clock_jitter_bound,
            memory_pressure,
            process_fault_attempt,
            fault_operation_sequence,
            pending_process_observations,
        } = self.snapshot;
        let mut state = serializer.serialize_struct("CanonicalSimulationContinuation", 14)?;
        state.serialize_field("schema_version", &3_u16)?;
        state.serialize_field(
            "projection",
            &if self.exclude_kernel_clock {
                "exclude-inaccessible-kernel-clock-v1"
            } else {
                "full-state-v1"
            },
        )?;
        state.serialize_field("tick", tick)?;
        state.serialize_field(
            "vm_snapshots",
            &CanonicalVmContinuations {
                snapshots: vm_snapshots,
                exclude_kernel_clock: self.exclude_kernel_clock,
            },
        )?;
        state.serialize_field("network_state", network_state)?;
        state.serialize_field("fault_engine_snapshot", fault_engine_snapshot)?;
        state.serialize_field("vcpu_stall_until", vcpu_stall_until)?;
        state.serialize_field("cpu_throttle", cpu_throttle)?;
        state.serialize_field("clock_freeze", clock_freeze)?;
        state.serialize_field("clock_jitter_bound", clock_jitter_bound)?;
        state.serialize_field("memory_pressure", memory_pressure)?;
        state.serialize_field("process_fault_attempt", process_fault_attempt)?;
        state.serialize_field("fault_operation_sequence", fault_operation_sequence)?;
        state.serialize_field("pending_process_observations", pending_process_observations)?;
        state.end()
    }
}

struct CanonicalVmContinuations<'a> {
    snapshots: &'a [(
        chaoscontrol_vmm::snapshot::VmSnapshot,
        chaoscontrol_vmm::controller::VmStatus,
    )],
    exclude_kernel_clock: bool,
}

impl Serialize for CanonicalVmContinuations<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut sequence = serializer.serialize_seq(Some(self.snapshots.len()))?;
        for (snapshot, status) in self.snapshots {
            let view = snapshot
                .continuation_view(self.exclude_kernel_clock)
                .map_err(|error| {
                    serde::ser::Error::custom(format!("continuation view: {error:?}"))
                })?;
            sequence.serialize_element(&(view, status))?;
        }
        sequence.end()
    }
}

pub(crate) fn validated_checkpoint_snapshot<'a>(
    recording: &Recording,
    checkpoint: &'a crate::checkpoint::Checkpoint,
) -> Result<&'a SimulationSnapshot, ReplayError> {
    let snapshot = checkpoint
        .snapshot
        .as_ref()
        .ok_or_else(|| ReplayError::InvalidState {
            message: format!("Checkpoint {} has no snapshot", checkpoint.id),
        })?;
    if checkpoint.tick != snapshot.tick {
        return InvalidStateSnafu {
            message: format!(
                "checkpoint {} tick does not match its snapshot tick",
                checkpoint.id
            ),
        }
        .fail();
    }
    validate_fault_outcome_ledger(snapshot.fault_engine_snapshot.outcomes()).map_err(|error| {
        ReplayError::InvalidState {
            message: format!(
                "checkpoint {} has an invalid fault ledger: {error}",
                checkpoint.id
            ),
        }
    })?;
    let prefix_end = recording
        .fault_round_deltas
        .iter()
        .filter(|delta| delta.tick <= checkpoint.tick)
        .map(|delta| delta.event_end)
        .next_back()
        .unwrap_or(0);
    let prefix_end = usize::try_from(prefix_end).map_err(|_| ReplayError::InvalidState {
        message: "checkpoint fault-stage prefix exceeds platform bounds".to_string(),
    })?;
    let expected = replay_fault_ledger_prefix(recording, prefix_end, checkpoint.tick)?;
    if snapshot.fault_engine_snapshot.outcomes() != &expected {
        return InvalidStateSnafu {
            message: format!(
                "checkpoint {} fault ledger does not match the recorded trace prefix",
                checkpoint.id
            ),
        }
        .fail();
    }
    Ok(snapshot)
}

fn replay_fault_ledger_prefix(
    recording: &Recording,
    prefix_end: usize,
    tick: u64,
) -> Result<FaultOutcomeLedger, ReplayError> {
    let events = recording
        .fault_stage_events
        .get(..prefix_end)
        .ok_or_else(|| ReplayError::InvalidState {
            message: "checkpoint fault-stage prefix exceeds the recorded trace".to_string(),
        })?;
    let mut ledger = FaultOutcomeLedger::new();
    for event in events {
        let attempt = if event.kind == FaultStageKind::Selected {
            Some(
                &recording
                    .fault_outcome_ledger
                    .attempts
                    .get(&event.attempt_id)
                    .ok_or_else(|| ReplayError::InvalidState {
                        message: "recorded selected event has no attempt".to_string(),
                    })?
                    .attempt,
            )
        } else {
            None
        };
        ledger = transition_fault_outcome(&ledger, attempt, event.attempt_id, event.kind.clone())
            .map_err(|error| ReplayError::InvalidState {
            message: format!("recorded fault prefix is invalid: {error}"),
        })?;
    }
    ledger.control_events = fault_control_prefix(recording, tick, prefix_end as u64)
        .cloned()
        .collect();
    validate_fault_outcome_ledger(&ledger).map_err(|error| ReplayError::InvalidState {
        message: format!("recorded fault-control prefix is invalid: {error}"),
    })?;
    Ok(ledger)
}

fn fault_control_prefix(
    recording: &Recording,
    tick: u64,
    stage_event_count: u64,
) -> impl Iterator<Item = &FaultControlEvent> {
    recording
        .fault_outcome_ledger
        .control_events
        .iter()
        .filter(move |event| {
            event.stage_event_count <= stage_event_count
                && (event.run_id != recording.fault_run_id || event.tick <= tick)
        })
}

pub(crate) fn validate_replayed_fault_controls(
    recording: &Recording,
    tick: u64,
    stage_event_count: u64,
    replayed: &[FaultControlEvent],
) -> Result<(), ReplayError> {
    if !replayed
        .iter()
        .eq(fault_control_prefix(recording, tick, stage_event_count))
    {
        return Err(ReplayError::InvalidState {
            message: format!("fault-control trace mismatch at tick {tick}"),
        });
    }
    Ok(())
}

pub(crate) fn replay_pause_requests<R: SimulationRunner>(
    recording: &Recording,
    runner: &mut R,
    stage_event_count: u64,
) -> Result<(), ReplayError> {
    while let Some(event) = recording
        .fault_outcome_ledger
        .control_events
        .get(runner.fault_control_events().len())
    {
        if event.tick != runner.tick() || event.stage_event_count != stage_event_count {
            break;
        }
        let FaultControlKind::PauseRequested { request, .. } = &event.kind else {
            break;
        };
        if request.requester_bytes != request.requester.len() as u64 {
            return Err(ReplayError::InvalidState {
                message: "recorded rejected pause request has no retained raw identity".to_string(),
            });
        }
        let before = runner.fault_control_events().len();
        runner.replay_pause_request(chaoscontrol_fault::mechanisms::PauseControlRequest {
            requester: request.requester.clone(),
            start_tick: request.start_tick,
            end_tick: request.end_tick,
        })?;
        if runner.fault_control_events().len() != before + 1
            || runner.fault_control_events().last() != Some(event)
        {
            return Err(ReplayError::InvalidState {
                message: "pause-control admission diverged from the recorded decision".to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn checked_target_tick(start_tick: u64, ticks: u64) -> Result<u64, ReplayError> {
    start_tick
        .checked_add(ticks)
        .ok_or_else(|| ReplayError::InvalidState {
            message: "replay target tick overflow".to_string(),
        })
}

pub(crate) fn checked_recorded_target_tick(
    recording: &Recording,
    start_tick: u64,
    ticks: u64,
) -> Result<u64, ReplayError> {
    let target_tick = checked_target_tick(start_tick, ticks)?;
    if target_tick > recording.total_ticks {
        return InvalidStateSnafu {
            message: format!(
                "exact replay target tick {target_tick} exceeds recorded horizon {}",
                recording.total_ticks
            ),
        }
        .fail();
    }
    Ok(target_tick)
}

pub(crate) fn validate_replayed_fault_interval(
    recording: &Recording,
    start_tick: u64,
    target_tick: u64,
    replayed: &[FaultStageEvent],
) -> Result<(), ReplayError> {
    let mut expected = Vec::new();
    for delta in &recording.fault_round_deltas {
        if delta.tick > start_tick && delta.tick <= target_tick {
            let start =
                usize::try_from(delta.event_start).map_err(|_| ReplayError::InvalidState {
                    message: "recorded fault-stage delta start exceeds platform bounds".to_string(),
                })?;
            let end = usize::try_from(delta.event_end).map_err(|_| ReplayError::InvalidState {
                message: "recorded fault-stage delta end exceeds platform bounds".to_string(),
            })?;
            expected.extend_from_slice(&recording.fault_stage_events[start..end]);
        }
    }
    if replayed != expected {
        return InvalidStateSnafu {
            message: format!(
                "replay did not consume the exact fault-stage trace through tick {target_tick}"
            ),
        }
        .fail();
    }
    Ok(())
}

pub(crate) fn validate_replayed_schedule_round(
    recording: &Recording,
    tick: u64,
    replayed: &[VmScheduleTrace],
) -> Result<(), ReplayError> {
    let expected = recording
        .schedule_rounds
        .iter()
        .find(|round| round.tick == tick)
        .map_or(&[][..], |round| round.traces.as_slice());
    if replayed != expected {
        return InvalidStateSnafu {
            message: format!("deterministic SMP schedule trace mismatch at tick {tick}"),
        }
        .fail();
    }
    Ok(())
}

fn validate_replayed_injection_round(
    recording: &Recording,
    tick: u64,
    replayed: &[chaoscontrol_vmm::controller::VmInjectionTrace],
) -> Result<(), ReplayError> {
    let expected = recording
        .schedule_rounds
        .iter()
        .find(|round| round.tick == tick)
        .map_or(&[][..], |round| round.injection_traces.as_slice());
    if replayed != expected {
        return Err(ReplayError::InvalidState {
            message: format!(
                "canonical requested/accepted injection trace mismatch at tick {tick}"
            ),
        });
    }
    Ok(())
}

pub(crate) fn validate_replayed_serial_suffix(
    recording: &Recording,
    start_tick: u64,
    target_tick: u64,
    vm_index: usize,
    actual: &str,
) -> Result<(), ReplayError> {
    let expected = recording
        .events
        .iter()
        .filter_map(|event| match event {
            RecordedEvent::SerialOutput {
                tick,
                vm_index: index,
                data,
            } if *index == vm_index && *tick > start_tick && *tick <= target_tick => {
                Some(data.as_bytes())
            }
            _ => None,
        })
        .flatten()
        .copied();
    if !expected.eq(actual.bytes()) {
        return Err(ReplayError::InvalidState {
            message: format!(
                "actual guest serial suffix mismatch for VM {vm_index} through tick {target_tick}"
            ),
        });
    }
    Ok(())
}

pub(crate) fn validate_replayed_fault_round(
    recording: &Recording,
    tick: u64,
    replayed: &[FaultStageEvent],
) -> Result<(), ReplayError> {
    let expected = if let Some(delta) = recording
        .fault_round_deltas
        .iter()
        .find(|delta| delta.tick == tick)
    {
        let start = usize::try_from(delta.event_start).map_err(|_| ReplayError::InvalidState {
            message: "recorded fault-stage delta start exceeds platform bounds".to_string(),
        })?;
        let end = usize::try_from(delta.event_end).map_err(|_| ReplayError::InvalidState {
            message: "recorded fault-stage delta end exceeds platform bounds".to_string(),
        })?;
        &recording.fault_stage_events[start..end]
    } else {
        &[]
    };
    if replayed != expected {
        return InvalidStateSnafu {
            message: format!("fault-stage trace mismatch at tick {tick}"),
        }
        .fail();
    }
    Ok(())
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
    /// Recording events that occurred during replay.
    pub events: Vec<RecordedEvent>,
    /// Ordered fault-stage events produced by replay execution.
    pub fault_outcomes: Vec<FaultStageEvent>,
    /// Final snapshot (if available).
    pub final_snapshot: Option<SimulationSnapshot>,
    /// Explicit mutations never certify the original recorded continuation.
    pub counterfactual: bool,
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

    use crate::recording::{FaultRoundTraceDelta, RecordedScheduleRound};
    use chaoscontrol_fault::faults::Fault;
    use chaoscontrol_fault::outcomes::{
        fault_run_id, transition_fault_outcome, FaultAttempt, FaultAttemptId, FaultAttemptSource,
        FaultOutcomeLedger, FaultPlanEffect, FaultScheduleId, FaultStageKind,
        NANOSECONDS_PER_SIMULATION_TICK,
    };
    use chaoscontrol_vmm::controller::NetworkFabric;
    use chaoscontrol_vmm::scheduler::core::{
        plan_execution_observation, ExecutionProgressObservation, ProgressMode, ScheduleState,
        ScheduleTrace,
    };
    use chaoscontrol_vmm::scheduler::{SchedulerConfig, SchedulingStrategy};

    // r[verify chaoscontrol.system_runs.fault_injection.scenario.recorded]
    #[test]
    fn real_runner_rejects_partial_metadata_without_booting() {
        let recording = test_recording();
        assert!(matches!(RealSimulationRunner::create(&recording.config,
            recording.schedule.clone(), recording.seed, recording.fault_run_sequence),
            Err(ReplayError::InvalidState { message }) if message.contains("ExactSession")));
    }

    const SHORT_REPLAY_QUANTUM: u64 = 45;
    const CORRUPT_FAULT_TRACE_QUANTUM: u64 = 43;
    const DIVERGENT_SCHEDULE_TRACE_QUANTUM: u64 = 47;

    fn one_step_schedule_trace() -> VmScheduleTrace {
        let config = SchedulerConfig {
            num_vcpus: 2,
            progress_budget: chaoscontrol_vmm::scheduler::core::ProgressBudget::Instructions(2),
            strategy: SchedulingStrategy::RoundRobin,
            seed: 42,
        };
        let initial_state = ScheduleState::new(&config, ProgressMode::ExactSingleStep).unwrap();
        let planned = plan_execution_observation(
            &initial_state,
            ExecutionProgressObservation::ExactInstruction { vcpu: 0 },
        )
        .unwrap()
        .unwrap();
        VmScheduleTrace {
            vm_index: 0,
            trace: ScheduleTrace {
                schema_version: chaoscontrol_vmm::scheduler::core::SCHEDULE_TRACE_SCHEMA_VERSION,
                initial_state_id: initial_state.identity(),
                initial_state,
                records: vec![planned.record],
            },
        }
    }

    fn mock_attempt(tick: u64, seed: u64, schedule_id: FaultScheduleId) -> FaultAttempt {
        FaultAttempt::new_with_source(
            fault_run_id(seed, 1, schedule_id),
            1,
            schedule_id,
            tick - 1,
            tick * NANOSECONDS_PER_SIMULATION_TICK,
            FaultAttemptSource::Scheduled {
                entry_index: tick - 1,
                scheduled_at_ns: tick * NANOSECONDS_PER_SIMULATION_TICK,
            },
            Fault::ProcessKill { target: 0 },
        )
    }

    fn applied_mock_attempt(schedule_id: FaultScheduleId) -> FaultAttempt {
        const APPLIED_TRACE_SEED: u64 = 46;
        FaultAttempt::new_with_source(
            fault_run_id(APPLIED_TRACE_SEED, 1, schedule_id),
            1,
            schedule_id,
            0,
            NANOSECONDS_PER_SIMULATION_TICK,
            FaultAttemptSource::Scheduled {
                entry_index: 0,
                scheduled_at_ns: NANOSECONDS_PER_SIMULATION_TICK,
            },
            Fault::ProcessKill { target: 0 },
        )
    }

    // Mock simulation runner for testing
    struct MockRunner {
        tick: u64,
        max_ticks: u64,
        corrupt_trace: bool,
        emit_schedule_trace: bool,
        applied_trace: bool,
        scheduled_fault_count: usize,
        run_start_tick: u64,
        schedule: FaultSchedule,
        run_sequence: u64,
        seed: u64,
        control_engine: chaoscontrol_fault::engine::FaultEngine,
    }

    impl SimulationRunner for MockRunner {
        fn settle_exact_observation(&mut self) -> Result<(), ReplayError> {
            // Abstract rounds have no deferred KVM completion state.
            Ok(())
        }

        fn create(
            config: &RecordingConfig,
            schedule: FaultSchedule,
            seed: u64,
            run_sequence: u64,
        ) -> Result<Self, ReplayError> {
            let mut control_engine = chaoscontrol_fault::engine::FaultEngine::new(
                chaoscontrol_fault::engine::EngineConfig {
                    seed,
                    schedule: Some(schedule.clone()),
                    ..Default::default()
                },
            );
            control_engine.start_fresh_run_at(schedule.clone(), run_sequence);
            control_engine.force_setup_complete();
            Ok(Self {
                tick: 0,
                max_ticks: if config.quantum == SHORT_REPLAY_QUANTUM {
                    1
                } else {
                    1000
                },
                corrupt_trace: config.quantum == CORRUPT_FAULT_TRACE_QUANTUM,
                emit_schedule_trace: config.quantum == DIVERGENT_SCHEDULE_TRACE_QUANTUM,
                applied_trace: seed == 46,
                scheduled_fault_count: schedule.total(),
                run_start_tick: 0,
                schedule,
                run_sequence,
                seed,
                control_engine,
            })
        }

        fn fault_control_events(&self) -> &[FaultControlEvent] {
            &self.control_engine.fault_outcomes().control_events
        }

        fn replay_pause_request(
            &mut self,
            mut request: chaoscontrol_fault::mechanisms::PauseControlRequest,
        ) -> Result<(), ReplayError> {
            if self.corrupt_trace {
                request.end_tick += 1;
            }
            let _decision = self
                .control_engine
                .request_pause_control(request, self.tick);
            Ok(())
        }

        fn begin_counterfactual_fault_run(
            &mut self,
            schedule: FaultSchedule,
        ) -> Result<(), ReplayError> {
            self.run_sequence =
                self.run_sequence
                    .checked_add(1)
                    .ok_or_else(|| ReplayError::InvalidState {
                        message: "mock counterfactual run sequence exhausted".to_string(),
                    })?;
            self.scheduled_fault_count = schedule.total();
            self.run_start_tick = self.tick;
            self.control_engine
                .begin_counterfactual_run(schedule.clone())
                .map_err(|error| ReplayError::InvalidState {
                    message: error.to_string(),
                })?;
            self.schedule = schedule;
            Ok(())
        }

        fn step_round(&mut self) -> Result<RoundResult, ReplayError> {
            self.tick += 1;
            if self.seed == mechanism_fixtures::PAUSE_TRACE_SEED {
                let start = self.control_engine.fault_outcomes().events.len();
                let attempts = self
                    .control_engine
                    .poll_fault_attempts(self.tick * NANOSECONDS_PER_SIMULATION_TICK)
                    .map_err(|error| ReplayError::InvalidState {
                        message: error.to_string(),
                    })?;
                return Ok(RoundResult {
                    injection_traces: Vec::new(),
                    tick: self.tick,
                    vms_running: 2,
                    vms_halted: 0,
                    faults_fired: attempts.into_iter().map(|attempt| attempt.fault).collect(),
                    fault_outcomes: self.control_engine.fault_outcomes().events[start..].to_vec(),
                    messages_delivered: 0,
                    schedule_traces: Vec::new(),
                });
            }
            if self.seed == mechanism_fixtures::MECHANISM_TRACE_SEED {
                let recording = mechanism_fixtures::recording_for_schedule(self.schedule.clone());
                let mut fault_outcomes = if self.tick == 1 {
                    recording.fault_stage_events
                } else {
                    Vec::new()
                };
                if self.corrupt_trace {
                    if let Some(event) = fault_outcomes.first_mut() {
                        event.attempt_id = FaultAttemptId([9; 32]);
                    }
                }
                return Ok(RoundResult {
                    injection_traces: Vec::new(),
                    tick: self.tick,
                    vms_running: 2,
                    vms_halted: 0,
                    faults_fired: if self.tick == 1 {
                        self.schedule
                            .faults()
                            .iter()
                            .map(|entry| entry.fault.clone())
                            .collect()
                    } else {
                        Vec::new()
                    },
                    fault_outcomes,
                    messages_delivered: 0,
                    schedule_traces: Vec::new(),
                });
            }
            let attempt = if self.applied_trace {
                applied_mock_attempt(self.schedule.identity())
            } else {
                mock_attempt(self.tick, self.seed, self.schedule.identity())
            };
            let attempt_id = if self.corrupt_trace {
                FaultAttemptId([9; 32])
            } else {
                attempt.id
            };
            let fault_outcomes = if self.applied_trace && self.tick == 1 {
                let effect = FaultPlanEffect::ProcessKill { target: 0 };
                vec![
                    FaultStageEvent {
                        sequence: 0,
                        attempt_id,
                        kind: FaultStageKind::Selected,
                    },
                    FaultStageEvent {
                        sequence: 1,
                        attempt_id,
                        kind: FaultStageKind::Applicable {
                            effect: effect.clone(),
                        },
                    },
                    FaultStageEvent {
                        sequence: 2,
                        attempt_id,
                        kind: FaultStageKind::Applied { effect },
                    },
                ]
            } else if self.applied_trace {
                Vec::new()
            } else if usize::try_from(self.tick.saturating_sub(self.run_start_tick))
                .is_ok_and(|tick| tick > 0 && tick <= self.scheduled_fault_count)
            {
                vec![FaultStageEvent {
                    sequence: self.tick - 1,
                    attempt_id,
                    kind: FaultStageKind::Selected,
                }]
            } else {
                Vec::new()
            };
            Ok(RoundResult {
                injection_traces: Vec::new(),
                tick: self.tick,
                vms_running: if self.tick < self.max_ticks { 2 } else { 0 },
                vms_halted: 0,
                faults_fired: if fault_outcomes
                    .iter()
                    .any(|event| event.kind == FaultStageKind::Selected)
                {
                    vec![attempt.fault]
                } else {
                    Vec::new()
                },
                fault_outcomes,
                messages_delivered: 0,
                schedule_traces: if self.emit_schedule_trace && self.tick == 1 {
                    vec![one_step_schedule_trace()]
                } else {
                    Vec::new()
                },
            })
        }

        fn snapshot_all(&mut self) -> Result<SimulationSnapshot, ReplayError> {
            use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};

            let mut engine = FaultEngine::new(EngineConfig {
                seed: self.seed,
                schedule: Some(self.schedule.clone()),
                ..EngineConfig::default()
            });
            engine.start_fresh_run_at(self.schedule.clone(), self.run_sequence);

            Ok(SimulationSnapshot {
                tick: self.tick,
                vm_snapshots: vec![],
                network_state: NetworkFabric::new(2, 42),
                fault_engine_snapshot: if self.seed == mechanism_fixtures::PAUSE_TRACE_SEED {
                    self.control_engine.snapshot()
                } else {
                    engine.snapshot()
                },
                vcpu_stall_until: vec![],
                cpu_throttle: vec![],
                clock_freeze: vec![],
                clock_jitter_bound: vec![],
                memory_pressure: vec![],
                process_fault_attempt: vec![],
                pending_process_observations: Default::default(),
                fault_operation_sequence: 0,
            })
        }

        fn restore_all(&mut self, snapshot: &SimulationSnapshot) -> Result<(), ReplayError> {
            if snapshot.fault_engine_snapshot.schedule_id() != self.schedule.identity()
                || snapshot.fault_engine_snapshot.run_sequence() != self.run_sequence
            {
                return Err(ReplayError::InvalidState {
                    message: "mock runner rejected a foreign fault snapshot".to_string(),
                });
            }
            self.tick = snapshot.tick;
            if self.seed == mechanism_fixtures::PAUSE_TRACE_SEED {
                self.control_engine
                    .restore_orchestration(&snapshot.fault_engine_snapshot)
                    .map_err(|error| ReplayError::InvalidState {
                        message: error.to_string(),
                    })?;
            }
            Ok(())
        }

        fn tick(&self) -> u64 {
            self.tick
        }

        fn report(&self) -> OracleReport {
            OracleReport {
                total_runs: 1,
                ..OracleReport::empty()
            }
        }

        fn serial_output(&self, _vm_index: usize) -> &str {
            ""
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
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.start_fresh_run_at(FaultSchedule::new(), 1);

        // Add checkpoint at tick 500
        checkpoints.push(Checkpoint {
            id: 0,
            tick: 500,
            snapshot: Some(SimulationSnapshot {
                tick: 500,
                vm_snapshots: vec![],
                network_state: NetworkFabric::new(2, 42),
                fault_engine_snapshot: engine.snapshot(),
                vcpu_stall_until: vec![],
                cpu_throttle: vec![],
                clock_freeze: vec![],
                clock_jitter_bound: vec![],
                memory_pressure: vec![],
                process_fault_attempt: vec![],
                pending_process_observations: Default::default(),
                fault_operation_sequence: 0,
            }),
            serial_output: vec![],
            events_since_last: vec![],
        });

        Recording {
            trace_start_tick: 0,
            schema_version: crate::recording::RECORDING_SCHEMA_VERSION,
            session_id: "test".to_string(),
            timestamp: 0,
            config: RecordingConfig {
                smp_progress_mode: chaoscontrol_vmm::vm::VmConfig::default().smp_progress_mode,
                smp_progress_budget: chaoscontrol_vmm::vm::VmConfig::default().smp_progress_budget,
                num_vms: 2,
                vm_memory_size: 256 * 1024 * 1024,
                tsc_khz: 3_000_000,
                kernel_path: "/test/vmlinux".to_string(),
                initrd_path: None,
                quantum: 100,
                checkpoint_interval: 1000,
                disk_image_path: None,
                random_faults: false,
            },
            checkpoints,
            schedule: FaultSchedule::new(),
            seed: 42,
            fault_run_sequence: 1,
            fault_run_id: fault_run_id(42, 1, FaultSchedule::new().identity()),
            events: vec![],
            fault_stage_events: vec![],
            fault_round_deltas: vec![],
            fault_outcome_ledger: Default::default(),
            schedule_rounds: vec![],
            oracle_report: None,
            total_ticks: 1000,
        }
    }

    fn traced_recording(round_count: u64) -> Recording {
        let mut recording = test_recording();
        let mut schedule = FaultSchedule::new();
        for tick in 1..=round_count {
            schedule.add(chaoscontrol_fault::schedule::ScheduledFault::new(
                tick * NANOSECONDS_PER_SIMULATION_TICK,
                Fault::ProcessKill { target: 0 },
            ));
        }
        let schedule_id = schedule.identity();
        recording.schedule = schedule;
        recording.fault_run_id =
            fault_run_id(recording.seed, recording.fault_run_sequence, schedule_id);
        recording.checkpoints = CheckpointStore::new();
        let mut ledger = FaultOutcomeLedger::new();
        for tick in 1..=round_count {
            let attempt = mock_attempt(tick, recording.seed, schedule_id);
            ledger = transition_fault_outcome(
                &ledger,
                Some(&attempt),
                attempt.id,
                FaultStageKind::Selected,
            )
            .unwrap();
            recording.fault_round_deltas.push(FaultRoundTraceDelta {
                tick,
                event_start: tick - 1,
                event_end: tick,
            });
            recording.events.push(RecordedEvent::FaultFired {
                tick,
                fault: format!("{:?}", attempt.fault),
            });
        }
        recording.fault_stage_events = ledger.events.clone();
        recording.fault_outcome_ledger = ledger;
        recording
    }

    fn applied_trace_recording() -> Recording {
        const APPLIED_TRACE_SEED: u64 = 46;
        let mut schedule = FaultSchedule::new();
        schedule.add(chaoscontrol_fault::schedule::ScheduledFault::new(
            NANOSECONDS_PER_SIMULATION_TICK,
            Fault::ProcessKill { target: 0 },
        ));
        let attempt = applied_mock_attempt(schedule.identity());
        let effect = FaultPlanEffect::ProcessKill { target: 0 };
        let mut ledger = transition_fault_outcome(
            &FaultOutcomeLedger::new(),
            Some(&attempt),
            attempt.id,
            FaultStageKind::Selected,
        )
        .unwrap();
        ledger = transition_fault_outcome(
            &ledger,
            None,
            attempt.id,
            FaultStageKind::Applicable {
                effect: effect.clone(),
            },
        )
        .unwrap();
        ledger = transition_fault_outcome(
            &ledger,
            None,
            attempt.id,
            FaultStageKind::Applied { effect },
        )
        .unwrap();

        let mut recording = test_recording();
        recording.seed = APPLIED_TRACE_SEED;
        recording.schedule = schedule;
        recording.fault_run_id = fault_run_id(
            recording.seed,
            recording.fault_run_sequence,
            recording.schedule.identity(),
        );
        recording.checkpoints = CheckpointStore::new();
        recording.events = vec![RecordedEvent::FaultFired {
            tick: 1,
            fault: format!("{:?}", attempt.fault),
        }];
        recording.fault_stage_events = ledger.events.clone();
        recording.fault_round_deltas = vec![FaultRoundTraceDelta {
            tick: 1,
            event_start: 0,
            event_end: 3,
        }];
        recording.fault_outcome_ledger = ledger;
        recording
    }

    // r[verify chaoscontrol.replay_debugger.navigation]
    #[test]
    fn replay_and_debugger_share_fault_smp_and_arrival_verdicts() {
        for quantum in [
            100,
            CORRUPT_FAULT_TRACE_QUANTUM,
            DIVERGENT_SCHEDULE_TRACE_QUANTUM,
            SHORT_REPLAY_QUANTUM,
        ] {
            let mut recording = traced_recording(3);
            recording.config.quantum = quantum;
            recording.total_ticks = 3;
            let replay = ReplayEngine::<MockRunner>::new(recording.clone());
            let mut debugger = crate::debugger::Debugger::<MockRunner>::new(recording);
            let control = replay.replay_from(None, 3);
            let observed = debugger.goto(3);
            assert_eq!(control.is_ok(), observed.is_ok(), "quantum {quantum}");
            if let (Err(control), Err(observed)) = (control, observed) {
                assert_eq!(control.to_string(), observed.to_string());
                assert!(!debugger.state().live_state_available);
            }
        }
    }

    #[test]
    fn explicit_empty_evidence_allows_exact_start_end_and_reverse_navigation() {
        let mut recording = test_recording();
        recording.total_ticks = 3;
        recording.checkpoints = CheckpointStore::new();
        for tick in 1..=3 {
            recording.fault_round_deltas.push(FaultRoundTraceDelta {
                tick,
                event_start: 0,
                event_end: 0,
            });
            recording.schedule_rounds.push(RecordedScheduleRound {
                injection_traces: Vec::new(),
                tick,
                traces: Vec::new(),
            });
        }
        let replay = ReplayEngine::<MockRunner>::new(recording.clone());
        let mut debugger = crate::debugger::Debugger::<MockRunner>::new(recording);
        assert_eq!(replay.replay_from(None, 0).unwrap().ticks_executed, 0);
        assert_eq!(debugger.goto(0).unwrap().tick, 0);
        assert_eq!(replay.replay_from(None, 3).unwrap().ticks_executed, 3);
        assert_eq!(debugger.goto(3).unwrap().tick, 3);
        assert_eq!(debugger.rewind(3).unwrap().tick, 0);
        assert_eq!(debugger.step_forward(3).unwrap().tick, 3);
        assert!(debugger.goto(4).is_err());
        assert!(debugger.read_registers(0, 0).is_err());
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
        assert!(result.fault_outcomes.is_empty());

        let repeated = engine.replay_from(None, 100).unwrap();
        assert_eq!(result.fault_outcomes, repeated.fault_outcomes);
    }

    #[test]
    fn replay_compares_exact_recorded_fault_trace() {
        let recording = traced_recording(3);
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording.clone());

        let result = engine.replay_from(None, 3).unwrap();

        assert_eq!(result.fault_outcomes, recording.fault_stage_events);
    }

    #[test]
    fn exact_replay_rejects_schedule_trace_divergence() {
        let mut recording = test_recording();
        let trace = one_step_schedule_trace();
        recording.schedule_rounds.push(RecordedScheduleRound {
            injection_traces: Vec::new(),
            tick: 1,
            traces: vec![trace.clone()],
        });
        validate_recording(&recording).unwrap();
        validate_replayed_schedule_round(&recording, 1, &[trace]).unwrap();

        assert!(matches!(
            validate_replayed_schedule_round(&recording, 1, &[]),
            Err(ReplayError::InvalidState { .. })
        ));
    }

    #[test]
    fn exact_replay_path_rejects_live_schedule_trace_divergence() {
        let mut recording = test_recording();
        recording.config.quantum = DIVERGENT_SCHEDULE_TRACE_QUANTUM;
        let engine = ReplayEngine::<MockRunner>::new(recording);

        assert!(matches!(
            engine.replay_from(None, 1),
            Err(ReplayError::InvalidState { .. })
        ));
    }

    #[test]
    fn exact_replay_rejects_forged_recording_before_runner_creation() {
        let mut recording = test_recording();
        let mut trace = one_step_schedule_trace();
        trace.trace.records[0].post_state_id.0[0] ^= 1;
        recording.schedule_rounds.push(RecordedScheduleRound {
            injection_traces: Vec::new(),
            tick: 1,
            traces: vec![trace],
        });

        assert!(matches!(
            validate_recording(&recording),
            Err(crate::recording::RecordingValidationError::InvalidScheduleTrace)
        ));
    }

    #[test]
    fn replay_rejects_fault_trace_mismatch() {
        let mut recording = traced_recording(3);
        recording.config.quantum = CORRUPT_FAULT_TRACE_QUANTUM;
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        assert!(matches!(
            engine.replay_from(None, 3),
            Err(ReplayError::InvalidState { .. })
        ));
    }

    #[test]
    fn saved_and_loaded_applied_trace_replays_exactly() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("applied-trace.json");
        let recording = applied_trace_recording();
        crate::serialize::save_recording(&recording, &path).unwrap();
        let loaded = crate::serialize::load_recording(&path).unwrap();
        assert_eq!(loaded.schedule.identity(), recording.schedule.identity());
        assert_eq!(loaded.schedule.faults(), recording.schedule.faults());
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(loaded.clone());

        let result = engine.replay_from(None, 1).unwrap();

        assert_eq!(result.fault_outcomes, loaded.fault_stage_events);
        assert!(matches!(
            result.fault_outcomes.as_slice(),
            [
                FaultStageEvent {
                    kind: FaultStageKind::Selected,
                    ..
                },
                FaultStageEvent {
                    kind: FaultStageKind::Applicable { .. },
                    ..
                },
                FaultStageEvent {
                    kind: FaultStageKind::Applied { .. },
                    ..
                }
            ]
        ));
    }

    #[test]
    fn replay_rejects_halt_before_later_recorded_delta() {
        let mut recording = traced_recording(3);
        recording.config.quantum = SHORT_REPLAY_QUANTUM;
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        assert!(matches!(
            engine.replay_from(None, 3),
            Err(ReplayError::InvalidState { .. })
        ));
    }

    #[test]
    fn replay_rejects_target_tick_overflow() {
        let recording = test_recording();
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        assert!(matches!(
            engine.replay_from(Some(0), u64::MAX),
            Err(ReplayError::InvalidState { .. })
        ));
    }

    #[test]
    fn checkpoint_rejects_mismatched_recorded_ledger_prefix() {
        use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};

        let mut recording = applied_trace_recording();
        let mut checkpoint_engine = FaultEngine::new(EngineConfig {
            seed: recording.seed,
            schedule: Some(recording.schedule.clone()),
            ..EngineConfig::default()
        });
        checkpoint_engine
            .start_fresh_run_at(recording.schedule.clone(), recording.fault_run_sequence);
        recording.checkpoints.push(Checkpoint {
            id: 0,
            tick: 1,
            snapshot: Some(SimulationSnapshot {
                tick: 1,
                vm_snapshots: vec![],
                network_state: NetworkFabric::new(0, recording.seed),
                fault_engine_snapshot: checkpoint_engine.snapshot(),
                vcpu_stall_until: vec![],
                cpu_throttle: vec![],
                clock_freeze: vec![],
                clock_jitter_bound: vec![],
                memory_pressure: vec![],
                process_fault_attempt: vec![],
                pending_process_observations: Default::default(),
                fault_operation_sequence: 0,
            }),
            serial_output: vec![],
            events_since_last: vec![],
        });
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        assert!(matches!(
            engine.replay_from(Some(0), 0),
            Err(ReplayError::InvalidState { .. })
        ));
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

        let mut new_schedule = FaultSchedule::new();
        new_schedule.add(chaoscontrol_fault::schedule::ScheduledFault::new(
            0,
            Fault::ProcessKill { target: 0 },
        ));
        let result = engine.replay_with_schedule(0, new_schedule, 50).unwrap();
        assert_eq!(result.ticks_executed, 50);
        assert_eq!(result.fault_outcomes.len(), 1);
    }

    #[test]
    fn public_counterfactual_entries_reject_malformed_deltas_without_panicking() {
        let mut recording = test_recording();
        recording.fault_round_deltas.push(FaultRoundTraceDelta {
            tick: 1,
            event_start: 0,
            event_end: u64::MAX,
        });
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        assert!(matches!(
            engine.replay_with_schedule(0, FaultSchedule::new(), 1),
            Err(ReplayError::InvalidState { .. })
        ));
        assert!(matches!(
            engine.replay_with_modification(0, vec![], vec![], 1),
            Err(ReplayError::InvalidState { .. })
        ));
    }

    #[test]
    fn replay_runner_starts_the_recorded_run_sequence() {
        use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};

        const RECORDED_RUN_SEQUENCE: u64 = 7;
        let mut recording = test_recording();
        recording.fault_run_sequence = RECORDED_RUN_SEQUENCE;
        recording.fault_run_id = fault_run_id(
            recording.seed,
            RECORDED_RUN_SEQUENCE,
            recording.schedule.identity(),
        );
        let mut checkpoint_engine = FaultEngine::new(EngineConfig::default());
        checkpoint_engine.start_fresh_run_at(recording.schedule.clone(), RECORDED_RUN_SEQUENCE);
        recording.checkpoints = CheckpointStore::new();
        recording.checkpoints.push(Checkpoint {
            id: 0,
            tick: 0,
            snapshot: Some(SimulationSnapshot {
                tick: 0,
                vm_snapshots: vec![],
                network_state: NetworkFabric::new(0, recording.seed),
                fault_engine_snapshot: checkpoint_engine.snapshot(),
                vcpu_stall_until: vec![],
                cpu_throttle: vec![],
                clock_freeze: vec![],
                clock_jitter_bound: vec![],
                memory_pressure: vec![],
                process_fault_attempt: vec![],
                pending_process_observations: Default::default(),
                fault_operation_sequence: 0,
            }),
            serial_output: vec![],
            events_since_last: vec![],
        });
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        assert!(engine.replay_from(Some(0), 0).is_ok());
    }

    #[test]
    fn exact_replay_treats_empty_trace_as_authoritative() {
        let mut recording = test_recording();
        let mut schedule = FaultSchedule::new();
        schedule.add(chaoscontrol_fault::schedule::ScheduledFault::new(
            NANOSECONDS_PER_SIMULATION_TICK,
            Fault::ProcessKill { target: 0 },
        ));
        recording.schedule = schedule;
        recording.fault_run_id = fault_run_id(
            recording.seed,
            recording.fault_run_sequence,
            recording.schedule.identity(),
        );
        recording.checkpoints = CheckpointStore::new();
        recording.total_ticks = 1;
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        assert!(matches!(
            engine.replay_from(None, 1),
            Err(ReplayError::InvalidState { .. })
        ));
    }

    #[test]
    fn checkpoint_accepts_exact_zero_fault_prefix() {
        use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};

        let mut recording = traced_recording(1);
        let mut checkpoint_engine = FaultEngine::new(EngineConfig {
            seed: recording.seed,
            schedule: Some(recording.schedule.clone()),
            ..EngineConfig::default()
        });
        checkpoint_engine
            .start_fresh_run_at(recording.schedule.clone(), recording.fault_run_sequence);
        recording.checkpoints.push(Checkpoint {
            id: 0,
            tick: 0,
            snapshot: Some(SimulationSnapshot {
                tick: 0,
                vm_snapshots: vec![],
                network_state: NetworkFabric::new(0, recording.seed),
                fault_engine_snapshot: checkpoint_engine.snapshot(),
                vcpu_stall_until: vec![],
                cpu_throttle: vec![],
                clock_freeze: vec![],
                clock_jitter_bound: vec![],
                memory_pressure: vec![],
                process_fault_attempt: vec![],
                pending_process_observations: Default::default(),
                fault_operation_sequence: 0,
            }),
            serial_output: vec![],
            events_since_last: vec![],
        });
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        let result = engine.replay_from(Some(0), 0).unwrap();
        assert_eq!(result.ticks_executed, 0);
        assert!(result.fault_outcomes.is_empty());
    }

    #[test]
    fn checkpoint_rejects_tampered_counters_with_unchanged_events() {
        use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};

        let mut recording = traced_recording(1);
        let mut checkpoint_engine = FaultEngine::new(EngineConfig {
            seed: recording.seed,
            schedule: Some(recording.schedule.clone()),
            ..EngineConfig::default()
        });
        checkpoint_engine
            .start_fresh_run_at(recording.schedule.clone(), recording.fault_run_sequence);
        checkpoint_engine.force_setup_complete();
        checkpoint_engine
            .poll_fault_attempts(NANOSECONDS_PER_SIMULATION_TICK)
            .unwrap();
        let mut tampered = checkpoint_engine.snapshot();
        let unchanged_events = tampered.outcomes().events.clone();
        let mut tampered_ledger = tampered.outcomes().clone();
        tampered_ledger.counters.selected = tampered_ledger.counters.selected.saturating_add(1);
        tampered.replace_outcomes_for_validation_test(tampered_ledger);
        assert_eq!(tampered.outcomes().events, unchanged_events);
        recording.checkpoints.push(Checkpoint {
            id: 0,
            tick: 1,
            snapshot: Some(SimulationSnapshot {
                tick: 1,
                vm_snapshots: vec![],
                network_state: NetworkFabric::new(0, recording.seed),
                fault_engine_snapshot: tampered,
                vcpu_stall_until: vec![],
                cpu_throttle: vec![],
                clock_freeze: vec![],
                clock_jitter_bound: vec![],
                memory_pressure: vec![],
                process_fault_attempt: vec![],
                pending_process_observations: Default::default(),
                fault_operation_sequence: 0,
            }),
            serial_output: vec![],
            events_since_last: vec![],
        });
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        assert!(matches!(
            engine.replay_from(Some(0), 0),
            Err(ReplayError::InvalidState { .. })
        ));
    }

    #[test]
    fn exact_replay_rejects_targets_past_the_recorded_horizon() {
        let recording = test_recording();
        let engine: ReplayEngine<MockRunner> = ReplayEngine::new(recording);

        let boundary = engine.replay_from(None, 1_000).unwrap();
        assert_eq!(boundary.ticks_executed, 1_000);
        assert!(matches!(
            engine.replay_from(None, 1_001),
            Err(ReplayError::InvalidState { .. })
        ));
        assert!(matches!(
            engine.replay_from(Some(0), 501),
            Err(ReplayError::InvalidState { .. })
        ));
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

    #[path = "mechanism_fixtures.rs"]
    mod mechanism_fixtures;
}
