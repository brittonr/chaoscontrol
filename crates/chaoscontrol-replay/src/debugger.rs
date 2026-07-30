//! Time-travel debugger — interactive navigation of recorded execution.

use crate::checkpoint::Checkpoint;
use crate::recording::{RecordedEvent, Recording};
use crate::replay::{
    InvalidStateSnafu, MemoryModification, RegisterModification, ReplayEngine, ReplayError,
    ReplayResult, SimulationRunner,
};
use chaoscontrol_vmm::registers::{Register, RegisterState};

/// Interactive time-travel debugger.
pub struct Debugger<R: SimulationRunner> {
    recording: Recording,
    replay: ReplayEngine<R>,
    /// Live runner holding VM state at `current_tick`.
    runner: Option<R>,
    /// Current position in the recording.
    current_tick: u64,
    /// Current checkpoint (the one we're "at" or just after).
    current_checkpoint: Option<u64>,
}

impl<R: SimulationRunner> Debugger<R> {
    /// Create a new debugger from a recording.
    pub fn new(recording: Recording) -> Self {
        let replay = ReplayEngine::new(recording.clone());
        Self {
            recording,
            replay,
            runner: None,
            current_tick: 0,
            current_checkpoint: None,
        }
    }

    /// Ensure we have a live runner, creating one if needed.
    fn ensure_runner(&mut self) -> Result<(), ReplayError> {
        if self.runner.is_none() {
            let runner = R::create(
                &self.recording.config,
                self.recording.schedule.clone(),
                self.recording.seed,
            )?;
            self.runner = Some(runner);
        }
        Ok(())
    }

    /// Jump to a specific tick (finds nearest checkpoint, replays forward).
    pub fn goto(&mut self, tick: u64) -> Result<DebugState, ReplayError> {
        self.ensure_runner()?;
        let runner = self.runner.as_mut().unwrap();

        // Find the nearest checkpoint at or before target tick
        let checkpoint = self.recording.checkpoints.at_or_before(tick);

        if let Some(cp) = checkpoint {
            self.current_checkpoint = Some(cp.id);
            let ticks_to_run = tick.saturating_sub(cp.tick);

            if let Some(snapshot) = &cp.snapshot {
                runner.restore_all(snapshot)?;
            }
            for _ in 0..ticks_to_run {
                let round = runner.step_round()?;
                if round.vms_running == 0 {
                    break;
                }
            }
            self.current_tick = runner.tick();
        } else {
            // No checkpoint before target, replay from beginning
            let result = self.replay.replay_from(None, tick)?;
            self.current_tick = result.ticks_executed;
            self.current_checkpoint = None;
        }

        Ok(self.build_state())
    }

    /// Rewind by N ticks from current position.
    pub fn rewind(&mut self, ticks: u64) -> Result<DebugState, ReplayError> {
        let target_tick = self.current_tick.saturating_sub(ticks);
        self.goto(target_tick)
    }

    /// Step forward by N ticks.
    pub fn step_forward(&mut self, ticks: u64) -> Result<DebugState, ReplayError> {
        let target_tick = self.current_tick + ticks;
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
    /// The modification is live — subsequent `step_forward` will see it.
    /// Use `goto()` to rewind to the original state.
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
        runner.write_memory(vm_index, address, data)
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
        let mut state = runner.read_registers(vm_index, vcpu)?;
        reg.set(&mut state, value);
        runner.set_registers(vm_index, vcpu, &state)
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
                    format!("...{}", &full[full.len() - 200..])
                } else {
                    full
                }
            })
            .collect();

        DebugState {
            tick: self.current_tick,
            checkpoint_id,
            vm_statuses: vec!["Running".to_string(); self.recording.config.num_vms],
            events_at_tick,
            serial_snippets,
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
    use chaoscontrol_fault::oracle::OracleReport;
    use chaoscontrol_fault::schedule::FaultSchedule;
    use chaoscontrol_vmm::controller::{NetworkFabric, RoundResult, SimulationSnapshot};

    // Mock simulation runner for testing
    struct MockRunner {
        tick: u64,
    }

    impl SimulationRunner for MockRunner {
        fn create(
            _config: &RecordingConfig,
            _schedule: FaultSchedule,
            _seed: u64,
        ) -> Result<Self, ReplayError> {
            Ok(Self { tick: 0 })
        }

        fn step_round(&mut self) -> Result<RoundResult, ReplayError> {
            self.tick += 1;
            Ok(RoundResult {
                tick: self.tick,
                vms_running: 2,
                vms_halted: 0,
                faults_fired: vec![],
                fault_outcomes: vec![],
                messages_delivered: 0,
            })
        }

        fn snapshot_all(&self) -> Result<SimulationSnapshot, ReplayError> {
            use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};

            let engine = FaultEngine::new(EngineConfig::default());

            Ok(SimulationSnapshot {
                tick: self.tick,
                vm_snapshots: vec![],
                network_state: NetworkFabric::new(2, 42),
                fault_engine_snapshot: engine.snapshot(),
                vcpu_stall_until: vec![],
                clock_freeze: vec![],
                clock_jitter_bound: vec![],
                process_fault_attempt: vec![],
                pending_process_observations: Default::default(),
                fault_operation_sequence: 0,
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
            Ok(vec![0xAA; size])
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
            },
            checkpoints: CheckpointStore::new(),
            schedule: FaultSchedule::new(),
            seed: 42,
            fault_stage_events: vec![],
            fault_round_deltas: vec![],
            fault_outcome_ledger: Default::default(),
            events: vec![
                RecordedEvent::FaultFired {
                    tick: 50,
                    fault: "test".to_string(),
                },
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

    #[test]
    fn test_debugger_new() {
        let recording = test_recording();
        let debugger: Debugger<MockRunner> = Debugger::new(recording);

        assert_eq!(debugger.current_tick, 0);
        assert_eq!(debugger.current_checkpoint, None);
    }

    #[test]
    fn test_debugger_goto() {
        let recording = test_recording();
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        let state = debugger.goto(50).unwrap();
        assert_eq!(state.tick, 50);
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
        let recording = test_recording();
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        let _state = debugger.goto(0).unwrap();

        let next_fault = debugger.next_event(EventFilter::AnyFault);
        assert!(next_fault.is_some());
        assert_eq!(event_tick(next_fault.unwrap()), 50);
    }

    #[test]
    fn test_debugger_goto_bug() {
        let recording = test_recording();
        let mut debugger: Debugger<MockRunner> = Debugger::new(recording);

        let state = debugger.goto_bug(1).unwrap();
        assert_eq!(state.tick, 200);
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
        assert_eq!(events.len(), 2);
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
