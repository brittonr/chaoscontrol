//! Time-travel debugger — interactive navigation of recorded execution.

use crate::checkpoint::Checkpoint;
use crate::recording::{RecordedEvent, Recording};
use crate::replay::{
    checked_target_tick, InvalidStateSnafu, MemoryModification, RegisterModification, ReplayEngine,
    ReplayError, ReplayRecording, ReplayResult, SimulationRunner,
};
use crate::session::ExactSession;
use chaoscontrol_vmm::registers::{Register, RegisterState};
use std::sync::Arc;

/// Interactive time-travel debugger.
pub struct Debugger<R: SimulationRunner> {
    recording: ReplayRecording,
    replay: ReplayEngine<R>,
    /// Live runner holding VM state at `current_tick`.
    runner: Option<R>,
    /// Current position in the recording.
    current_tick: u64,
    /// Current checkpoint (the one we're "at" or just after).
    current_checkpoint: Option<u64>,
    /// Live edited state is explicitly outside the original session claim.
    counterfactual: bool,
    observation_limit: u64,
}

impl<R: SimulationRunner> Debugger<R> {
    /// Create a new debugger from a recording.
    pub fn new(recording: Recording) -> Self {
        let replay = ReplayEngine::new(recording);
        let recording = replay.recording_source();
        Self {
            recording,
            replay,
            runner: None,
            current_tick: 0,
            current_checkpoint: None,
            counterfactual: false,
            observation_limit: crate::session::SessionLimits::default().max_output_bytes,
        }
    }

    /// Open an admitted persisted session, with its complete execution inputs.
    pub fn from_session(session: Arc<ExactSession>) -> Self {
        let replay = ReplayEngine::from_session(session.clone());
        let recording = replay.recording_source();
        let current_tick = session.manifest().retained_start_tick;
        let observation_limit = session.limits().max_output_bytes;
        Self {
            recording,
            replay,
            runner: None,
            current_tick,
            current_checkpoint: None,
            counterfactual: false,
            observation_limit,
        }
    }

    /// Jump using the replay engine's identical fault/SMP/arrival validators.
    // r[impl chaoscontrol.replay_debugger.navigation]
    // r[impl chaoscontrol.replay_debugger.failure_isolation]
    pub fn goto(&mut self, tick: u64) -> Result<DebugState, ReplayError> {
        // Invalidate before admission too: failure must never leave an old live
        // state looking like the requested position. Metadata remains last verified.
        self.runner.take();
        self.counterfactual = false;
        let (runner, checkpoint) = self.replay.seek(tick)?;
        self.current_tick = runner.tick();
        self.current_checkpoint = checkpoint;
        self.runner = Some(runner);
        Ok(self.build_state())
    }

    /// Rewind by N ticks from current position.
    pub fn rewind(&mut self, ticks: u64) -> Result<DebugState, ReplayError> {
        let target_tick = self
            .current_tick
            .saturating_sub(ticks)
            .max(self.replay.start_tick());
        self.goto(target_tick)
    }

    /// Step forward by N ticks.
    pub fn step_forward(&mut self, ticks: u64) -> Result<DebugState, ReplayError> {
        let target_tick = match checked_target_tick(self.current_tick, ticks) {
            Ok(tick) => tick,
            Err(error) => {
                self.runner.take();
                return Err(error);
            }
        };
        self.goto(target_tick)
    }

    /// Jump to next event of a given type.
    pub fn next_event(&self, event_filter: EventFilter) -> Option<&RecordedEvent> {
        self.recording.events.iter().find(|e| {
            let tick = event_tick(e);
            tick > self.current_tick && event_filter.matches(e)
        })
    }

    /// Jump to the tick where a bug was detected.
    pub fn goto_bug(&mut self, bug_id: u64) -> Result<DebugState, ReplayError> {
        for event in &self.recording.events {
            if let RecordedEvent::BugDetected {
                tick, bug_id: id, ..
            } = event
            {
                if *id == bug_id {
                    return self.goto(*tick);
                }
            }
        }
        self.runner.take();
        InvalidStateSnafu {
            message: format!("Bug {} not found", bug_id),
        }
        .fail()
    }

    /// Read guest physical memory at the current position.
    pub fn read_memory(
        &self,
        vm_index: usize,
        address: u64,
        size: usize,
    ) -> Result<Vec<u8>, ReplayError> {
        if u64::try_from(size).map_or(true, |size| size > self.observation_limit / 4) {
            return Err(ReplayError::InvalidState {
                message: "memory observation exceeds admitted output byte bound".into(),
            });
        }
        let runner = self.runner.as_ref().ok_or_else(|| {
            InvalidStateSnafu {
                message: "No runner — call goto() first".to_string(),
            }
            .build()
        })?;
        runner.read_memory(vm_index, address, size)
    }

    /// Read VM registers at the current position.
    pub fn read_registers(
        &self,
        vm_index: usize,
        vcpu: usize,
    ) -> Result<RegisterState, ReplayError> {
        let runner = self.runner.as_ref().ok_or_else(|| {
            InvalidStateSnafu {
                message: "No runner — call goto() first".to_string(),
            }
            .build()
        })?;
        runner.read_registers(vm_index, vcpu)
    }

    /// Write bytes to guest physical memory (destructive analysis).
    ///
    /// The live state is labeled counterfactual. Navigation independently
    /// restores the original session; it does not certify an edited suffix.
    pub fn poke_memory(
        &mut self,
        vm_index: usize,
        address: u64,
        data: &[u8],
    ) -> Result<(), ReplayError> {
        let runner = self.runner.as_mut().ok_or_else(|| {
            InvalidStateSnafu {
                message: "No runner — call goto() first".to_string(),
            }
            .build()
        })?;
        self.counterfactual = true;
        let result = runner.write_memory(vm_index, address, data);
        if result.is_err() {
            self.runner.take();
        }
        result
    }

    /// Set a single register on a vCPU (destructive analysis).
    pub fn set_register(
        &mut self,
        vm_index: usize,
        vcpu: usize,
        reg: Register,
        value: u64,
    ) -> Result<(), ReplayError> {
        let runner = self.runner.as_mut().ok_or_else(|| {
            InvalidStateSnafu {
                message: "No runner — call goto() first".to_string(),
            }
            .build()
        })?;
        self.counterfactual = true;
        let result = (|| {
            let mut state = runner.read_registers(vm_index, vcpu)?;
            reg.set(&mut state, value);
            runner.set_registers(vm_index, vcpu, &state)
        })();
        if result.is_err() {
            self.runner.take();
        }
        result
    }

    /// Get serial output up to the current position.
    pub fn serial_output(&self, vm_index: usize) -> String {
        // Find the most recent checkpoint at or before current tick
        if let Some(cp) = self.recording.checkpoints.at_or_before(self.current_tick) {
            cp.serial_output.get(vm_index).cloned().unwrap_or_default()
        } else {
            String::new()
        }
    }

    /// Get all events between two ticks.
    pub fn events_between(&self, start_tick: u64, end_tick: u64) -> Vec<&RecordedEvent> {
        self.recording
            .events
            .iter()
            .filter(|e| {
                let tick = event_tick(e);
                tick >= start_tick && tick <= end_tick
            })
            .collect()
    }

    /// Counterfactual: modify memory and/or registers at current position
    /// and continue execution for N ticks.
    pub fn counterfactual(
        &mut self,
        memory_mods: Vec<MemoryModification>,
        register_mods: Vec<RegisterModification>,
        ticks: u64,
    ) -> Result<ReplayResult, ReplayError> {
        if self.runner.is_none() {
            return Err(ReplayError::InvalidState {
                message: "no verified live position for counterfactual analysis".into(),
            });
        }
        let checkpoint_id = self.current_checkpoint.ok_or_else(|| {
            InvalidStateSnafu {
                message: "No checkpoint available for counterfactual".to_string(),
            }
            .build()
        })?;

        self.replay
            .replay_with_modification(checkpoint_id, memory_mods, register_mods, ticks)
    }

    /// List all checkpoints.
    pub fn checkpoints(&self) -> &[Checkpoint] {
        self.recording.checkpoints.all()
    }

    /// Named retained-output bound also limits script intake and hex observations.
    pub fn observation_limit(&self) -> u64 {
        self.observation_limit
    }

    /// Get current state.
    pub fn state(&self) -> DebugState {
        self.build_state()
    }

    /// Build debug state from current position.
    fn build_state(&self) -> DebugState {
        let checkpoint_id = self.current_checkpoint.unwrap_or(0);

        let events_at_tick: Vec<_> = self
            .recording
            .events
            .iter()
            .filter(|e| event_tick(e) == self.current_tick)
            .cloned()
            .collect();

        let serial_snippets: Vec<_> = (0..self.recording.config.num_vms)
            .map(|i| {
                let full = self.serial_output(i);
                if full.len() > 200 {
                    format!("...{}", crate::text::suffix_at_char_boundary(&full, 200))
                } else {
                    full
                }
            })
            .collect();

        DebugState {
            tick: self.current_tick,
            checkpoint_id,
            vm_statuses: vec![
                if self.runner.is_none() {
                    "Unavailable"
                } else if self.counterfactual {
                    "Counterfactual"
                } else {
                    "Verified"
                }
                .to_string();
                self.recording.config.num_vms
            ],
            events_at_tick,
            serial_snippets,
            verified_original: self.runner.is_some() && !self.counterfactual,
            live_state_available: self.runner.is_some(),
            counterfactual: self.counterfactual && self.runner.is_some(),
        }
    }

    /// Get the recording.
    pub fn recording(&self) -> &Recording {
        &self.recording
    }
}

/// Current debugger state at a position.
#[derive(Debug, Clone)]
pub struct DebugState {
    pub tick: u64,
    pub checkpoint_id: u64,
    pub vm_statuses: Vec<String>,
    pub events_at_tick: Vec<RecordedEvent>,
    pub serial_snippets: Vec<String>,
    /// False after any failed seek, even though tick retains last verified metadata.
    pub verified_original: bool,
    pub live_state_available: bool,
    pub counterfactual: bool,
}

// RegisterState re-exported from chaoscontrol_vmm::registers

/// Filter for finding events.
#[derive(Debug, Clone)]
pub enum EventFilter {
    AnyFault,
    AnyAssertion,
    FailedAssertion,
    AnyBug,
    VmStatusChange,
    SerialOutput,
}

impl EventFilter {
    /// Check if an event matches this filter.
    pub fn matches(&self, event: &RecordedEvent) -> bool {
        matches!(
            (self, event),
            (EventFilter::AnyFault, RecordedEvent::FaultFired { .. })
                | (
                    EventFilter::AnyAssertion,
                    RecordedEvent::AssertionHit { .. }
                )
                | (
                    EventFilter::FailedAssertion,
                    RecordedEvent::AssertionHit { passed: false, .. }
                )
                | (EventFilter::AnyBug, RecordedEvent::BugDetected { .. })
                | (
                    EventFilter::VmStatusChange,
                    RecordedEvent::VmStatusChange { .. }
                )
                | (
                    EventFilter::SerialOutput,
                    RecordedEvent::SerialOutput { .. }
                )
        )
    }
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
    use crate::checkpoint::CheckpointStore;
    use crate::recording::RecordingConfig;
    use chaoscontrol_fault::faults::Fault;
    use chaoscontrol_fault::oracle::OracleReport;
    use chaoscontrol_fault::outcomes::{
        FaultPlanEffect, FaultStageKind, NANOSECONDS_PER_SIMULATION_TICK,
    };
    use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};
    use chaoscontrol_vmm::controller::{NetworkFabric, RoundResult, SimulationSnapshot};

    const SETTLEMENT_FAILURE_QUANTUM: u64 = 780;

    #[test]
    fn every_exact_target_settles_before_becoming_inspectable() {
        let mut debugger = Debugger::<MockRunner>::new(test_recording());
        for tick in [0, 1, 0] {
            assert!(debugger.goto(tick).unwrap().verified_original);
            assert!(debugger.runner.as_ref().unwrap().observation_settled);
        }
        let mut debugger = Debugger::<MockRunner>::new(checkpoint_trace_recording());
        assert!(debugger.goto(1).unwrap().verified_original);
        assert!(debugger.runner.as_ref().unwrap().observation_settled);
    }

    #[test]
    fn failed_observation_settlement_never_exposes_a_verified_position() {
        let mut recording = test_recording();
        recording.config.quantum = SETTLEMENT_FAILURE_QUANTUM;
        let mut debugger = Debugger::<MockRunner>::new(recording);
        for tick in [0, 1, 0] {
            assert!(debugger.goto(tick).is_err());
            assert!(!debugger.state().verified_original);
            assert!(!debugger.state().live_state_available);
            assert!(debugger.read_registers(0, 0).is_err());
        }
    }

    // Mock simulation runner for testing
    struct MockRunner {
        tick: u64,
        instance: u64,
        mode: u64,
        poisoned: bool,
        observation_settled: bool,
    }

    impl SimulationRunner for MockRunner {
        fn settle_exact_observation(&mut self) -> Result<(), ReplayError> {
            if self.mode == SETTLEMENT_FAILURE_QUANTUM {
                self.poisoned = true;
                return Err(ReplayError::Runner {
                    message: "injected observation settlement failure".into(),
                });
            }
            self.observation_settled = true;
            Ok(())
        }

        fn create(
            config: &RecordingConfig,
            _schedule: FaultSchedule,
            _seed: u64,
            _run_sequence: u64,
        ) -> Result<Self, ReplayError> {
            static NEXT_INSTANCE: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            Ok(Self {
                tick: 0,
                mode: config.quantum,
                poisoned: false,
                observation_settled: false,
                instance: NEXT_INSTANCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            })
        }

        fn fault_control_events(&self) -> &[chaoscontrol_fault::evidence::FaultControlEvent] {
            &[]
        }

        fn replay_pause_request(
            &mut self,
            _request: chaoscontrol_fault::mechanisms::PauseControlRequest,
        ) -> Result<(), ReplayError> {
            Err(ReplayError::InvalidState {
                message: "this debugger fixture has no pause-control engine".into(),
            })
        }

        fn begin_counterfactual_fault_run(
            &mut self,
            _schedule: FaultSchedule,
        ) -> Result<(), ReplayError> {
            Ok(())
        }

        fn step_round(&mut self) -> Result<RoundResult, ReplayError> {
            self.observation_settled = false;
            self.tick += 1;
            if self.poisoned {
                return Err(ReplayError::Runner {
                    message: "poisoned fixture reused".into(),
                });
            }
            if self.mode == POISON_QUANTUM && self.tick == 2 {
                self.poisoned = true;
                return Err(ReplayError::Runner {
                    message: "injected controller poison after progress".into(),
                });
            }
            if self.mode == DRIFT_QUANTUM {
                self.tick += 1;
            }
            Ok(RoundResult {
                tick: self.tick,
                vms_running: if self.mode == EARLY_STOP_QUANTUM {
                    0
                } else {
                    2
                },
                vms_halted: 0,
                faults_fired: vec![],
                fault_outcomes: vec![],
                messages_delivered: 0,
                schedule_traces: Vec::new(),
            })
        }

        fn snapshot_all(&mut self) -> Result<SimulationSnapshot, ReplayError> {
            use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};

            let engine = FaultEngine::new(EngineConfig::default());

            Ok(SimulationSnapshot {
                tick: self.tick,
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
            })
        }

        fn restore_all(&mut self, snapshot: &SimulationSnapshot) -> Result<(), ReplayError> {
            if self.poisoned {
                return Err(ReplayError::Runner {
                    message: "poisoned fixture restored".into(),
                });
            }
            self.tick = snapshot.tick;
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

        fn serial_output(&self, _vm_index: usize) -> String {
            String::new()
        }

        fn read_memory(
            &self,
            _vm_index: usize,
            _addr: u64,
            size: usize,
        ) -> Result<Vec<u8>, ReplayError> {
            let tick_byte = u8::try_from(self.tick).unwrap_or(u8::MAX);
            Ok(vec![tick_byte; size])
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
                checkpoint_interval: 100,
                disk_image_path: None,
                random_faults: false,
            },
            checkpoints: CheckpointStore::new(),
            schedule: FaultSchedule::new(),
            seed: 42,
            fault_run_sequence: 1,
            fault_run_id: chaoscontrol_fault::outcomes::fault_run_id(
                42,
                1,
                FaultSchedule::new().identity(),
            ),
            fault_stage_events: vec![],
            fault_round_deltas: vec![],
            fault_outcome_ledger: Default::default(),
            schedule_rounds: vec![],
            events: vec![
                RecordedEvent::AssertionHit {
                    tick: 100,
                    vm_index: 0,
                    assertion_id: 1,
                    location: "test.rs:10".to_string(),
                    passed: true,
                },
                RecordedEvent::BugDetected {
                    tick: 200,
                    bug_id: 1,
                    description: "test bug".to_string(),
                    checkpoint_id: None,
                },
            ],
            oracle_report: None,
            total_ticks: 1000,
        }
    }

    fn checkpoint_trace_recording() -> Recording {
        use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};

        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(
            NANOSECONDS_PER_SIMULATION_TICK,
            Fault::ProcessKill { target: 0 },
        ));
        let mut engine = FaultEngine::new(EngineConfig {
            schedule: Some(schedule.clone()),
            ..EngineConfig::default()
        });
        engine.start_fresh_run_at(schedule.clone(), 1);
        engine.force_setup_complete();
        let attempts = engine
            .poll_fault_attempts(NANOSECONDS_PER_SIMULATION_TICK)
            .unwrap();
        let attempt = &attempts[0];
        let effect = FaultPlanEffect::ProcessKill { target: 0 };
        engine
            .record_fault_stage(
                attempt.id,
                FaultStageKind::Applicable {
                    effect: effect.clone(),
                },
            )
            .unwrap();
        engine
            .record_fault_stage(attempt.id, FaultStageKind::Applied { effect })
            .unwrap();

        let mut recording = test_recording();
        recording.schedule = schedule.clone();
        recording.fault_run_sequence = 1;
        recording.fault_run_id = chaoscontrol_fault::outcomes::fault_run_id(
            recording.seed,
            recording.fault_run_sequence,
            schedule.identity(),
        );
        recording.events = vec![RecordedEvent::FaultFired {
            tick: 1,
            fault: format!("{:?}", attempt.fault),
        }];
        recording.fault_stage_events = engine.fault_outcomes().events.clone();
        recording.fault_round_deltas = vec![crate::recording::FaultRoundTraceDelta {
            tick: 1,
            event_start: 0,
            event_end: 3,
        }];
        recording.fault_outcome_ledger = engine.fault_outcomes().clone();
        recording.checkpoints.push(Checkpoint {
            id: 0,
            tick: 1,
            snapshot: Some(SimulationSnapshot {
                tick: 1,
                vm_snapshots: vec![],
                network_state: NetworkFabric::new(0, 42),
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
        recording
    }

    const EARLY_STOP_QUANTUM: u64 = 777;
    const POISON_QUANTUM: u64 = 778;
    const DRIFT_QUANTUM: u64 = 779;

    // r[verify chaoscontrol.replay_debugger.failure_isolation]
    #[test]
    fn failed_seek_discards_poisoned_runner_and_retry_is_fresh() {
        let mut recording = test_recording();
        recording.config.quantum = POISON_QUANTUM;
        let mut debugger = Debugger::<MockRunner>::new(recording);
        debugger.goto(1).unwrap();
        let first = debugger.runner.as_ref().unwrap().instance;
        assert!(debugger.goto(3).is_err());
        let failed = debugger.state();
        assert_eq!(failed.tick, 1, "only last verified metadata survives");
        assert!(!failed.live_state_available);
        assert!(!failed.verified_original);
        assert!(debugger.read_memory(0, 0, 1).is_err());
        assert!(debugger.read_registers(0, 0).is_err());
        debugger.goto(1).unwrap();
        assert_ne!(first, debugger.runner.as_ref().unwrap().instance);
        assert!(debugger.state().verified_original);
    }

    // r[verify chaoscontrol.replay_debugger.navigation]
    #[test]
    fn early_stop_and_tick_drift_never_publish_a_position() {
        for mode in [EARLY_STOP_QUANTUM, DRIFT_QUANTUM] {
            let mut recording = test_recording();
            recording.config.quantum = mode;
            let mut debugger = Debugger::<MockRunner>::new(recording);
            assert!(debugger.goto(3).is_err());
            assert!(!debugger.state().live_state_available);
            assert!(!debugger.state().verified_original);
        }
    }

    // r[verify chaoscontrol.replay_debugger.claim_boundary]
    #[test]
    fn edits_are_counterfactual_and_navigation_restores_original_claim() {
        let mut debugger = Debugger::<MockRunner>::new(test_recording());
        debugger.goto(1).unwrap();
        debugger.poke_memory(0, 0, &[1]).unwrap();
        assert!(debugger.state().counterfactual);
        assert!(!debugger.state().verified_original);
        debugger.goto(1).unwrap();
        assert!(debugger.state().verified_original);
        debugger.set_register(0, 0, Register::Rax, 7).unwrap();
        assert!(debugger.state().counterfactual);
    }

    #[test]
    fn deterministic_script_seeks_both_directions_and_rejects_errors() {
        let debugger = Debugger::<MockRunner>::new(test_recording());
        crate::inspect::run_script(
            debugger,
            std::io::Cursor::new(b"goto 2\nmem 0 0 4\nregs 0 0\nrewind 1\nstep 1\nstate\nquit\n"),
        )
        .unwrap();
        let debugger = Debugger::<MockRunner>::new(test_recording());
        assert!(crate::inspect::run_script(
            debugger,
            std::io::Cursor::new(b"goto 1001\nmem 0 0 4\n")
        )
        .is_err());
        let debugger = Debugger::<MockRunner>::new(test_recording());
        assert!(
            crate::inspect::run_script(debugger, std::io::Cursor::new(b"sh -c anything\n"))
                .is_err()
        );
    }

    #[test]
    fn checkpoint_choice_uses_latest_admitted_checkpoint() {
        let mut recording = checkpoint_trace_recording();
        let mut latest = recording.checkpoints.all()[0].clone();
        latest.id = 9;
        let mut earlier = latest.clone();
        earlier.id = 0;
        earlier.tick = 0;
        // Earlier snapshots need their own empty ledger, not the tick-one prefix.
        let mut runner = MockRunner::create(
            &recording.config,
            recording.schedule.clone(),
            recording.seed,
            recording.fault_run_sequence,
        )
        .unwrap();
        earlier.snapshot = Some(runner.snapshot_all().unwrap());
        let mut engine = chaoscontrol_fault::engine::FaultEngine::new(
            chaoscontrol_fault::engine::EngineConfig {
                seed: recording.seed,
                num_vms: recording.config.num_vms,
                schedule: Some(recording.schedule.clone()),
                random_faults: recording.config.random_faults,
                ..Default::default()
            },
        );
        engine.start_fresh_run_at(recording.schedule.clone(), recording.fault_run_sequence);
        earlier.snapshot.as_mut().unwrap().fault_engine_snapshot = engine.snapshot();
        recording.checkpoints = CheckpointStore::new();
        recording.checkpoints.push(earlier);
        recording.checkpoints.push(latest);
        let mut debugger = Debugger::<MockRunner>::new(recording);
        let state = debugger.goto(1).unwrap();
        assert_eq!(state.checkpoint_id, 9);
        assert_eq!(debugger.read_memory(0, 0, 1).unwrap(), [1]);
        assert_eq!(debugger.rewind(1).unwrap().checkpoint_id, 0);
        assert_eq!(debugger.step_forward(1).unwrap().checkpoint_id, 9);
    }

    #[test]
    fn test_debugger_new() {
        let recording = test_recording();
        let debugger: Debugger<MockRunner> = Debugger::new(recording);

        assert_eq!(debugger.current_tick, 0);
        assert_eq!(debugger.current_checkpoint, None);
    }

    #[test]
    fn debugger_checkpoint_accepts_exact_fault_trace_prefix() {
        let recording = checkpoint_trace_recording();
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        let state = debugger.goto(1).unwrap();

        assert_eq!(state.tick, 1);
        assert_eq!(state.checkpoint_id, 0);
    }

    #[test]
    fn debugger_checkpoint_rejects_tampered_fault_trace_prefix() {
        let mut recording = checkpoint_trace_recording();
        let empty_engine = chaoscontrol_fault::engine::FaultEngine::new(Default::default());
        let mut checkpoint = recording.checkpoints.all()[0].clone();
        checkpoint.snapshot.as_mut().unwrap().fault_engine_snapshot = empty_engine.snapshot();
        recording.checkpoints = CheckpointStore::new();
        recording.checkpoints.push(checkpoint);
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        assert!(matches!(
            debugger.goto(1),
            Err(ReplayError::InvalidState { .. })
        ));
    }

    #[test]
    fn debugger_checkpoint_rejects_missing_snapshot() {
        let mut recording = test_recording();
        recording.checkpoints.push(Checkpoint {
            id: 0,
            tick: 1,
            snapshot: None,
            serial_output: vec![],
            events_since_last: vec![],
        });
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        assert!(matches!(
            debugger.goto(1),
            Err(ReplayError::InvalidState { .. })
        ));
    }

    #[test]
    fn test_debugger_goto() {
        let recording = test_recording();
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        let state = debugger.goto(50).unwrap();
        assert_eq!(state.tick, 50);
    }

    #[test]
    fn debugger_without_checkpoint_installs_the_advanced_owned_runner() {
        const TARGET_TICK: u64 = 50;
        let recording = test_recording();
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        debugger.goto(TARGET_TICK).unwrap();
        let memory = debugger.read_memory(0, 0, 1).unwrap();

        assert_eq!(memory, vec![u8::try_from(TARGET_TICK).unwrap()]);
    }

    #[test]
    fn test_debugger_step_forward() {
        let recording = test_recording();
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        let _state1 = debugger.goto(100).unwrap();
        let state2 = debugger.step_forward(50).unwrap();
        assert_eq!(state2.tick, 150);
    }

    #[test]
    fn test_debugger_rewind() {
        let recording = test_recording();
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        let _state1 = debugger.goto(200).unwrap();
        let state2 = debugger.rewind(50).unwrap();
        assert_eq!(state2.tick, 150);
    }

    #[test]
    fn test_debugger_next_event() {
        let recording = checkpoint_trace_recording();
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        let _state = debugger.goto(0).unwrap();

        let next_fault = debugger.next_event(EventFilter::AnyFault);
        assert!(next_fault.is_some());
        assert_eq!(event_tick(next_fault.unwrap()), 1);
    }

    #[test]
    fn test_debugger_goto_bug() {
        let recording = test_recording();
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        let state = debugger.goto_bug(1).unwrap();
        assert_eq!(state.tick, 200);
    }

    #[test]
    fn debugger_rejects_navigation_past_the_recorded_horizon() {
        let recording = test_recording();
        let horizon = recording.total_ticks;
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        assert!(matches!(
            debugger.goto(horizon + 1),
            Err(ReplayError::InvalidState { .. })
        ));
    }

    #[test]
    fn test_debugger_goto_bug_not_found() {
        let recording = test_recording();
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        let result = debugger.goto_bug(999);
        assert!(result.is_err());
    }

    #[test]
    fn test_debugger_events_between() {
        let recording = test_recording();
        let debugger: Debugger<MockRunner> = Debugger::new(recording);

        let events = debugger.events_between(50, 100);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_event_filter_matches() {
        let fault_event = RecordedEvent::FaultFired {
            tick: 10,
            fault: "test".to_string(),
        };
        let assertion_pass = RecordedEvent::AssertionHit {
            tick: 20,
            vm_index: 0,
            assertion_id: 1,
            location: "test".to_string(),
            passed: true,
        };
        let assertion_fail = RecordedEvent::AssertionHit {
            tick: 30,
            vm_index: 0,
            assertion_id: 2,
            location: "test".to_string(),
            passed: false,
        };
        let bug_event = RecordedEvent::BugDetected {
            tick: 40,
            bug_id: 1,
            description: "bug".to_string(),
            checkpoint_id: None,
        };

        assert!(EventFilter::AnyFault.matches(&fault_event));
        assert!(!EventFilter::AnyFault.matches(&assertion_pass));

        assert!(EventFilter::AnyAssertion.matches(&assertion_pass));
        assert!(EventFilter::AnyAssertion.matches(&assertion_fail));

        assert!(!EventFilter::FailedAssertion.matches(&assertion_pass));
        assert!(EventFilter::FailedAssertion.matches(&assertion_fail));

        assert!(EventFilter::AnyBug.matches(&bug_event));
        assert!(!EventFilter::AnyBug.matches(&fault_event));
    }

    #[test]
    fn test_debugger_state() {
        let recording = test_recording();
        let debugger: Debugger<MockRunner> = Debugger::new(recording);

        let state = debugger.state();
        assert_eq!(state.tick, 0);
        assert_eq!(state.vm_statuses.len(), 2);
    }

    #[test]
    fn test_register_state_structure() {
        let regs = RegisterState {
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
            cs: 8,
            ss: 16,
            ds: 16,
            es: 16,
            fs: 16,
            gs: 16,
            cr0: 0,
            cr3: 0,
            cr4: 0,
        };

        assert_eq!(regs.rip, 0x1000);
        assert_eq!(regs.rsp, 0x2000);
    }
}
