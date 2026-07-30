//! Execution recording — captures checkpoints during a run.

use crate::checkpoint::{Checkpoint, CheckpointStore};
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::outcomes::{
    validate_fault_outcome_ledger, FaultOutcomeLedger, FaultStageEvent, FaultStageKind,
    FaultTransitionError, MAX_FAULT_OUTCOME_EVENTS, NANOSECONDS_PER_SIMULATION_TICK,
};
use chaoscontrol_fault::schedule::FaultSchedule;
use chaoscontrol_vmm::controller::{RoundResult, SimulationSnapshot};
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
    /// Events that occurred. `FaultFired` is a projection of `Selected` only.
    pub events: Vec<RecordedEvent>,
    /// Canonical bounded fault-stage trace.
    #[serde(default)]
    pub fault_stage_events: Vec<FaultStageEvent>,
    /// Non-empty per-round slices of the canonical fault-stage trace.
    #[serde(default)]
    pub fault_round_deltas: Vec<FaultRoundTraceDelta>,
    /// Authoritative ledger that supplies the canonical trace.
    #[serde(default)]
    pub fault_outcome_ledger: FaultOutcomeLedger,
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
    #[serde(default = "default_disk_image_path")]
    pub disk_image_path: Option<String>,
}

fn default_disk_image_path() -> Option<String> {
    None
}

/// A non-empty trace slice produced by one simulation round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaultRoundTraceDelta {
    pub tick: u64,
    pub event_start: u64,
    pub event_end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingValidationError {
    TraceBoundExceeded,
    InvalidLedger(FaultTransitionError),
    TraceLedgerMismatch,
    InvalidRoundDelta,
    RoundDeltaMismatch,
    FaultFiredProjectionMismatch,
}

/// An event recorded during execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        let timestamp = unix_timestamp_secs();

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
                fault_stage_events: Vec::new(),
                fault_round_deltas: Vec::new(),
                fault_outcome_ledger: FaultOutcomeLedger::default(),
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

    /// Record a non-fault event.
    pub fn record_event(&mut self, event: RecordedEvent) {
        assert!(!matches!(event, RecordedEvent::FaultFired { .. }));
        self.recording.events.push(event);
    }

    /// Record the exact stage delta from one completed round.
    pub fn record_round(
        &mut self,
        round: &RoundResult,
        ledger: &FaultOutcomeLedger,
    ) -> Result<(), RecordingValidationError> {
        let next = plan_recorded_round(&self.recording, round, ledger)?;
        self.recording.events.extend(next.fault_fired);
        self.recording.fault_stage_events = ledger.events.clone();
        self.recording.fault_round_deltas = next.round_deltas;
        self.recording.fault_outcome_ledger = ledger.clone();
        Ok(())
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

#[derive(Debug)]
struct PlannedRecordedRound {
    fault_fired: Vec<RecordedEvent>,
    round_deltas: Vec<FaultRoundTraceDelta>,
}

fn plan_recorded_round(
    recording: &Recording,
    round: &RoundResult,
    ledger: &FaultOutcomeLedger,
) -> Result<PlannedRecordedRound, RecordingValidationError> {
    validate_recording(recording)?;
    validate_fault_outcome_ledger(ledger).map_err(RecordingValidationError::InvalidLedger)?;
    let event_start = recording.fault_stage_events.len();
    let event_end = event_start
        .checked_add(round.fault_outcomes.len())
        .ok_or(RecordingValidationError::TraceBoundExceeded)?;
    if event_end > MAX_FAULT_OUTCOME_EVENTS {
        return Err(RecordingValidationError::TraceBoundExceeded);
    }
    if ledger.events.get(..event_start) != Some(recording.fault_stage_events.as_slice())
        || ledger.events.get(event_start..event_end) != Some(round.fault_outcomes.as_slice())
        || event_end != ledger.events.len()
    {
        return Err(RecordingValidationError::RoundDeltaMismatch);
    }

    let mut selected_faults = Vec::new();
    let mut fault_fired = Vec::new();
    for event in &round.fault_outcomes {
        if event.kind == FaultStageKind::Selected {
            let state = ledger
                .attempts
                .get(&event.attempt_id)
                .ok_or(RecordingValidationError::RoundDeltaMismatch)?;
            let selected_tick = state.attempt.selected_at_ns / NANOSECONDS_PER_SIMULATION_TICK;
            if selected_tick != round.tick {
                return Err(RecordingValidationError::RoundDeltaMismatch);
            }
            selected_faults.push(state.attempt.fault.clone());
            fault_fired.push(RecordedEvent::FaultFired {
                tick: round.tick,
                fault: format!("{:?}", state.attempt.fault),
            });
        }
    }
    if selected_faults != round.faults_fired {
        return Err(RecordingValidationError::FaultFiredProjectionMismatch);
    }

    let mut round_deltas = recording.fault_round_deltas.clone();
    if event_start != event_end {
        if round_deltas
            .last()
            .is_some_and(|delta| delta.tick >= round.tick)
        {
            return Err(RecordingValidationError::InvalidRoundDelta);
        }
        round_deltas.push(FaultRoundTraceDelta {
            tick: round.tick,
            event_start: u64::try_from(event_start)
                .map_err(|_| RecordingValidationError::TraceBoundExceeded)?,
            event_end: u64::try_from(event_end)
                .map_err(|_| RecordingValidationError::TraceBoundExceeded)?,
        });
    }
    Ok(PlannedRecordedRound {
        fault_fired,
        round_deltas,
    })
}

pub fn validate_recording(recording: &Recording) -> Result<(), RecordingValidationError> {
    if recording.fault_stage_events.len() > MAX_FAULT_OUTCOME_EVENTS {
        return Err(RecordingValidationError::TraceBoundExceeded);
    }
    validate_fault_outcome_ledger(&recording.fault_outcome_ledger)
        .map_err(RecordingValidationError::InvalidLedger)?;
    if recording.fault_stage_events != recording.fault_outcome_ledger.events {
        return Err(RecordingValidationError::TraceLedgerMismatch);
    }

    let mut expected_start = 0_u64;
    let mut prior_tick = None;
    for delta in &recording.fault_round_deltas {
        if delta.event_start != expected_start
            || delta.event_start >= delta.event_end
            || usize::try_from(delta.event_end).map_or(true, |event_end| {
                event_end > recording.fault_stage_events.len()
            })
            || prior_tick.is_some_and(|tick| tick >= delta.tick)
        {
            return Err(RecordingValidationError::InvalidRoundDelta);
        }
        expected_start = delta.event_end;
        prior_tick = Some(delta.tick);
    }
    let trace_len = u64::try_from(recording.fault_stage_events.len())
        .map_err(|_| RecordingValidationError::TraceBoundExceeded)?;
    if expected_start != trace_len {
        return Err(RecordingValidationError::InvalidRoundDelta);
    }

    if recording.fault_stage_events.is_empty()
        && recording.fault_round_deltas.is_empty()
        && recording.fault_outcome_ledger.events.is_empty()
    {
        return Ok(());
    }

    let expected_fault_fired = recording
        .fault_outcome_ledger
        .events
        .iter()
        .filter(|event| event.kind == FaultStageKind::Selected)
        .map(|event| {
            let state = recording
                .fault_outcome_ledger
                .attempts
                .get(&event.attempt_id)
                .ok_or(RecordingValidationError::TraceLedgerMismatch)?;
            Ok(RecordedEvent::FaultFired {
                tick: state.attempt.selected_at_ns / NANOSECONDS_PER_SIMULATION_TICK,
                fault: format!("{:?}", state.attempt.fault),
            })
        })
        .collect::<Result<Vec<_>, RecordingValidationError>>()?;
    let recorded_fault_fired = recording
        .events
        .iter()
        .filter(|event| matches!(event, RecordedEvent::FaultFired { .. }))
        .cloned()
        .collect::<Vec<_>>();
    if recorded_fault_fired != expected_fault_fired {
        return Err(RecordingValidationError::FaultFiredProjectionMismatch);
    }
    Ok(())
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

/// Read host wall-clock seconds at the recording shell boundary.
#[allow(
    ambient_clock,
    reason = "recording metadata needs host wall-clock timestamp"
)]
fn unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Generate a unique ID (simple timestamp-based).
#[allow(
    ambient_clock,
    reason = "recording shell ID uses host wall-clock entropy"
)]
fn uuid_like_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_fault::faults::Fault;
    use chaoscontrol_fault::outcomes::{
        transition_fault_outcome, FaultAttempt, FaultRunId, FaultScheduleId,
    };

    fn selected_round(tick: u64) -> (RoundResult, FaultOutcomeLedger) {
        let selected_at_ns = tick * NANOSECONDS_PER_SIMULATION_TICK;
        let attempt = FaultAttempt::new(
            FaultRunId([1; 32]),
            0,
            FaultScheduleId([2; 32]),
            0,
            selected_at_ns,
            Fault::ProcessKill { target: 0 },
        );
        let ledger = transition_fault_outcome(
            &FaultOutcomeLedger::default(),
            Some(&attempt),
            attempt.id,
            FaultStageKind::Selected,
        )
        .unwrap();
        let round = RoundResult {
            tick,
            vms_running: 1,
            vms_halted: 0,
            faults_fired: vec![attempt.fault],
            fault_outcomes: ledger.events.clone(),
            messages_delivered: 0,
        };
        (round, ledger)
    }

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
            network_state: NetworkFabric::new(0, 42),
            fault_engine_snapshot: engine.snapshot(),
            vcpu_stall_until: vec![],
            clock_freeze: vec![],
            clock_jitter_bound: vec![],
            process_fault_attempt: vec![],
            pending_process_observations: Default::default(),
            fault_operation_sequence: 0,
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

        recorder.record_event(RecordedEvent::SerialOutput {
            tick: 100,
            vm_index: 0,
            data: "ready".to_string(),
        });

        assert_eq!(recorder.recording.events.len(), 1);
    }

    #[test]
    fn recorder_persists_canonical_round_trace_and_selected_projection() {
        let mut recorder = Recorder::new(test_config(), FaultSchedule::new(), 42);
        let (round, ledger) = selected_round(1);

        recorder.record_round(&round, &ledger).unwrap();

        assert_eq!(recorder.recording.fault_stage_events, ledger.events);
        assert_eq!(recorder.recording.fault_outcome_ledger, ledger);
        assert_eq!(
            recorder.recording.fault_round_deltas,
            vec![FaultRoundTraceDelta {
                tick: 1,
                event_start: 0,
                event_end: 1,
            }]
        );
        assert!(matches!(
            recorder.recording.events.as_slice(),
            [RecordedEvent::FaultFired { tick: 1, .. }]
        ));
        validate_recording(&recorder.recording).unwrap();
    }

    #[test]
    fn tampered_recorded_trace_is_rejected() {
        let mut recorder = Recorder::new(test_config(), FaultSchedule::new(), 42);
        let (round, ledger) = selected_round(1);
        recorder.record_round(&round, &ledger).unwrap();
        recorder.recording.fault_stage_events[0].sequence = 1;

        assert_eq!(
            validate_recording(&recorder.recording),
            Err(RecordingValidationError::TraceLedgerMismatch)
        );
    }

    #[test]
    #[should_panic]
    fn direct_fault_fired_recording_is_rejected() {
        let mut recorder = Recorder::new(test_config(), FaultSchedule::new(), 42);
        recorder.record_event(RecordedEvent::FaultFired {
            tick: 1,
            fault: "not authoritative".to_string(),
        });
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
            catalog_size: 0,
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
        recorder.record_event(RecordedEvent::SerialOutput {
            tick: 100,
            vm_index: 0,
            data: "first".to_string(),
        });
        recorder.record_event(RecordedEvent::SerialOutput {
            tick: 500,
            vm_index: 0,
            data: "second".to_string(),
        });
        recorder.record_event(RecordedEvent::SerialOutput {
            tick: 1500,
            vm_index: 0,
            data: "third".to_string(),
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
