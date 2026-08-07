//! Execution recording — captures checkpoints during a run.

use crate::checkpoint::{Checkpoint, CheckpointStore};
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::outcomes::{
    fault_run_id, validate_fault_outcome_ledger, FaultAttemptSource, FaultAttemptState,
    FaultOutcomeLedger, FaultRunId, FaultStageCounters, FaultStageEvent, FaultStageKind,
    FaultTransitionError, MAX_FAULT_ATTEMPTS, MAX_FAULT_OUTCOME_EVENTS,
    NANOSECONDS_PER_SIMULATION_TICK,
};
use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};
use chaoscontrol_vmm::controller::{RoundResult, SimulationSnapshot, VmScheduleTrace};
use chaoscontrol_vmm::scheduler::core::{
    validate_transition_trace, ScheduleStateId, DEFAULT_SCHEDULE_JOURNAL_LIMIT, MAX_SCHEDULE_VCPUS,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MAX_RECORDED_SCHEDULE_FAULTS: usize = MAX_FAULT_ATTEMPTS;
const INITIAL_SIMULATION_RUN_SEQUENCE: u64 = 1;
const MAX_RECORDED_SCHEDULE_ROUNDS: usize = DEFAULT_SCHEDULE_JOURNAL_LIMIT;
const MAX_RECORDED_SCHEDULE_RECORDS: usize = DEFAULT_SCHEDULE_JOURNAL_LIMIT;

mod recorded_fault_ledger {
    use super::{
        FaultAttemptState, FaultOutcomeLedger, FaultStageCounters, FaultStageEvent,
        MAX_FAULT_ATTEMPTS,
    };
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    #[derive(Serialize, Deserialize)]
    struct WireFaultLedger {
        attempts: Vec<FaultAttemptState>,
        events: Vec<FaultStageEvent>,
        counters: FaultStageCounters,
    }

    pub fn serialize<S>(ledger: &FaultOutcomeLedger, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        WireFaultLedger {
            attempts: ledger.attempts.values().cloned().collect(),
            events: ledger.events.clone(),
            counters: ledger.counters,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<FaultOutcomeLedger, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireFaultLedger::deserialize(deserializer)?;
        if wire.attempts.len() > MAX_FAULT_ATTEMPTS {
            return Err(serde::de::Error::custom(
                "recorded fault ledger exceeds attempt bound",
            ));
        }
        let mut attempts = BTreeMap::new();
        for state in wire.attempts {
            if attempts.insert(state.attempt.id, state).is_some() {
                return Err(serde::de::Error::custom(
                    "recorded fault ledger contains a duplicate attempt",
                ));
            }
        }
        Ok(FaultOutcomeLedger {
            attempts,
            events: wire.events,
            counters: wire.counters,
        })
    }
}

mod recorded_fault_schedule {
    use super::{FaultSchedule, ScheduledFault, MAX_RECORDED_SCHEDULE_FAULTS};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(schedule: &FaultSchedule, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        schedule.faults().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<FaultSchedule, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries = Vec::<ScheduledFault>::deserialize(deserializer)?;
        if entries.len() > MAX_RECORDED_SCHEDULE_FAULTS {
            return Err(serde::de::Error::custom(
                "recorded fault schedule exceeds entry bound",
            ));
        }
        if entries
            .windows(2)
            .any(|pair| pair[0].time_ns > pair[1].time_ns)
        {
            return Err(serde::de::Error::custom(
                "recorded fault schedule is not canonically ordered",
            ));
        }
        let mut schedule = FaultSchedule::new();
        for entry in entries {
            schedule.add(entry);
        }
        Ok(schedule)
    }
}

mod recorded_schedule_rounds {
    use super::{RecordedScheduleRound, MAX_RECORDED_SCHEDULE_ROUNDS, MAX_SCHEDULE_VCPUS};
    use serde::de::{SeqAccess, Visitor};
    use serde::{Deserializer, Serialize, Serializer};
    use std::fmt;

    pub fn serialize<S>(rounds: &[RecordedScheduleRound], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        rounds.serialize(serializer)
    }

    pub fn deserialize_traces<'de, D>(
        deserializer: D,
    ) -> Result<Vec<super::VmScheduleTrace>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TraceVisitor;
        impl<'de> Visitor<'de> for TraceVisitor {
            type Value = Vec<super::VmScheduleTrace>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "at most {MAX_SCHEDULE_VCPUS} VM schedule traces")
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let hint = sequence.size_hint().unwrap_or(0);
                if hint > MAX_SCHEDULE_VCPUS {
                    return Err(serde::de::Error::custom(
                        "recorded VM schedule trace count exceeds bound",
                    ));
                }
                let mut traces = Vec::with_capacity(hint);
                while let Some(trace) = sequence.next_element::<super::VmScheduleTrace>()? {
                    if traces.len() >= MAX_SCHEDULE_VCPUS {
                        return Err(serde::de::Error::custom(
                            "recorded VM schedule trace count exceeds bound",
                        ));
                    }
                    traces.push(trace);
                }
                Ok(traces)
            }
        }
        deserializer.deserialize_seq(TraceVisitor)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<RecordedScheduleRound>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RoundVisitor;
        impl<'de> Visitor<'de> for RoundVisitor {
            type Value = Vec<RecordedScheduleRound>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    formatter,
                    "at most {MAX_RECORDED_SCHEDULE_ROUNDS} recorded schedule rounds"
                )
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let hint = sequence.size_hint().unwrap_or(0);
                if hint > MAX_RECORDED_SCHEDULE_ROUNDS {
                    return Err(serde::de::Error::custom(
                        "recorded schedule round count exceeds bound",
                    ));
                }
                let mut rounds: Vec<RecordedScheduleRound> = Vec::with_capacity(hint);
                while let Some(round) = sequence.next_element::<RecordedScheduleRound>()? {
                    if rounds.len() >= MAX_RECORDED_SCHEDULE_ROUNDS {
                        return Err(serde::de::Error::custom(
                            "recorded schedule round count exceeds bound",
                        ));
                    }
                    if round.traces.len() > MAX_SCHEDULE_VCPUS {
                        return Err(serde::de::Error::custom(
                            "recorded VM schedule trace count exceeds bound",
                        ));
                    }
                    rounds.push(round);
                }
                Ok(rounds)
            }
        }
        deserializer.deserialize_seq(RoundVisitor)
    }
}

/// A recorded execution session.
///
/// r[impl chaoscontrol.fault_outcomes.compatibility]
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
    /// The complete canonical fault schedule that was executed.
    #[serde(with = "recorded_fault_schedule")]
    pub schedule: FaultSchedule,
    /// Master seed.
    pub seed: u64,
    /// Exact fault run sequence represented by this recording.
    pub fault_run_sequence: u64,
    /// Exact fault run identity represented by this recording.
    pub fault_run_id: FaultRunId,
    /// Events that occurred. `FaultFired` is a projection of `Selected` only.
    pub events: Vec<RecordedEvent>,
    /// Canonical bounded fault-stage trace.
    #[serde(default)]
    pub fault_stage_events: Vec<FaultStageEvent>,
    /// Non-empty per-round slices of the canonical fault-stage trace.
    #[serde(default)]
    pub fault_round_deltas: Vec<FaultRoundTraceDelta>,
    /// Authoritative ledger that supplies the canonical trace.
    #[serde(default, with = "recorded_fault_ledger")]
    pub fault_outcome_ledger: FaultOutcomeLedger,
    /// Bounded exact SMP traces grouped by simulation round.
    #[serde(default, with = "recorded_schedule_rounds")]
    pub schedule_rounds: Vec<RecordedScheduleRound>,
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

/// Non-empty exact SMP traces emitted during one simulation round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedScheduleRound {
    /// Simulation tick that produced the traces.
    pub tick: u64,
    /// Canonically ordered VM traces for this tick.
    #[serde(deserialize_with = "recorded_schedule_rounds::deserialize_traces")]
    pub traces: Vec<VmScheduleTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingValidationError {
    TraceBoundExceeded,
    InvalidLedger(FaultTransitionError),
    TraceLedgerMismatch,
    InvalidRoundDelta,
    RoundDeltaMismatch,
    FaultFiredProjectionMismatch,
    ScheduleBoundExceeded,
    ScheduleNotCanonical,
    RecordingRunIdentityMismatch,
    AttemptScheduleMismatch,
    AttemptRunMismatch,
    AttemptSourceMismatch,
    CheckpointRunMismatch,
    CheckpointSnapshotMismatch,
    EvidenceBeyondHorizon,
    CheckpointOrderMismatch,
    SelectedDeltaTickMismatch,
    ScheduleTraceBoundExceeded,
    InvalidScheduleRound,
    InvalidScheduleTrace,
    ScheduleTraceDiscontinuity,
}

/// An event recorded during execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RecordedEvent {
    /// Legacy compatibility projection: a fault was selected at this tick.
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
        Self::new_for_run(config, schedule, seed, INITIAL_SIMULATION_RUN_SEQUENCE)
    }

    /// Create a recorder bound to one exact fault run.
    pub fn new_for_run(
        config: RecordingConfig,
        schedule: FaultSchedule,
        seed: u64,
        fault_run_sequence: u64,
    ) -> Self {
        let session_id = format!("rec_{}", uuid_like_id());
        let timestamp = unix_timestamp_secs();
        let next_checkpoint_tick = config.checkpoint_interval;
        let fault_run_id = fault_run_id(seed, fault_run_sequence, schedule.identity());

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
                fault_run_sequence,
                fault_run_id,
                events: Vec::new(),
                fault_stage_events: Vec::new(),
                fault_round_deltas: Vec::new(),
                fault_outcome_ledger: FaultOutcomeLedger::default(),
                schedule_rounds: Vec::new(),
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
        if let Some(schedule_round) = next.schedule_round {
            self.recording.schedule_rounds.push(schedule_round);
        }
        self.recording.total_ticks = self.recording.total_ticks.max(round.tick);
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
    schedule_round: Option<RecordedScheduleRound>,
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
    let schedule_round = plan_schedule_round(recording, round)?;
    Ok(PlannedRecordedRound {
        fault_fired,
        round_deltas,
        schedule_round,
    })
}

fn plan_schedule_round(
    recording: &Recording,
    round: &RoundResult,
) -> Result<Option<RecordedScheduleRound>, RecordingValidationError> {
    if round.schedule_traces.is_empty() {
        return Ok(None);
    }
    if recording
        .schedule_rounds
        .last()
        .is_some_and(|prior| prior.tick >= round.tick)
    {
        return Err(RecordingValidationError::InvalidScheduleRound);
    }
    let existing_records = recording
        .schedule_rounds
        .iter()
        .flat_map(|recorded_round| &recorded_round.traces)
        .try_fold(0usize, |count, vm_trace| {
            count.checked_add(vm_trace.trace.records.len())
        })
        .ok_or(RecordingValidationError::ScheduleTraceBoundExceeded)?;
    let new_records = round
        .schedule_traces
        .iter()
        .try_fold(0usize, |count, vm_trace| {
            count.checked_add(vm_trace.trace.records.len())
        })
        .ok_or(RecordingValidationError::ScheduleTraceBoundExceeded)?;
    if existing_records
        .checked_add(new_records)
        .ok_or(RecordingValidationError::ScheduleTraceBoundExceeded)?
        > MAX_RECORDED_SCHEDULE_RECORDS
    {
        return Err(RecordingValidationError::ScheduleTraceBoundExceeded);
    }

    let mut prior_ids: BTreeMap<usize, ScheduleStateId> = BTreeMap::new();
    for recorded_round in &recording.schedule_rounds {
        for vm_trace in &recorded_round.traces {
            let final_state =
                validate_transition_trace(&vm_trace.trace, DEFAULT_SCHEDULE_JOURNAL_LIMIT)
                    .map_err(|_| RecordingValidationError::InvalidScheduleTrace)?;
            prior_ids.insert(vm_trace.vm_index, final_state.identity());
        }
    }

    let mut prior_vm = None;
    for vm_trace in &round.schedule_traces {
        if vm_trace.vm_index >= recording.config.num_vms
            || prior_vm.is_some_and(|prior| prior >= vm_trace.vm_index)
            || vm_trace.trace.records.is_empty()
        {
            return Err(RecordingValidationError::InvalidScheduleRound);
        }
        let final_state =
            validate_transition_trace(&vm_trace.trace, DEFAULT_SCHEDULE_JOURNAL_LIMIT)
                .map_err(|_| RecordingValidationError::InvalidScheduleTrace)?;
        if prior_ids
            .get(&vm_trace.vm_index)
            .is_some_and(|prior| *prior != vm_trace.trace.initial_state_id)
        {
            return Err(RecordingValidationError::ScheduleTraceDiscontinuity);
        }
        prior_ids.insert(vm_trace.vm_index, final_state.identity());
        prior_vm = Some(vm_trace.vm_index);
    }

    Ok(Some(RecordedScheduleRound {
        tick: round.tick,
        traces: round.schedule_traces.clone(),
    }))
}

pub fn validate_recording(recording: &Recording) -> Result<(), RecordingValidationError> {
    if recording
        .events
        .iter()
        .any(|event| event_tick(event) > recording.total_ticks)
    {
        return Err(RecordingValidationError::EvidenceBeyondHorizon);
    }
    let mut prior_checkpoint = None;
    for checkpoint in recording.checkpoints.all() {
        if checkpoint.tick > recording.total_ticks
            || checkpoint
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.tick > recording.total_ticks)
            || checkpoint
                .events_since_last
                .iter()
                .any(|event| event_tick(event) > recording.total_ticks)
        {
            return Err(RecordingValidationError::EvidenceBeyondHorizon);
        }
        if checkpoint
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.tick != checkpoint.tick)
        {
            return Err(RecordingValidationError::CheckpointSnapshotMismatch);
        }
        if prior_checkpoint.is_some_and(|(prior_id, prior_tick)| {
            prior_id >= checkpoint.id || prior_tick >= checkpoint.tick
        }) {
            return Err(RecordingValidationError::CheckpointOrderMismatch);
        }
        prior_checkpoint = Some((checkpoint.id, checkpoint.tick));
    }
    if recording.schedule.total() > MAX_RECORDED_SCHEDULE_FAULTS {
        return Err(RecordingValidationError::ScheduleBoundExceeded);
    }
    if recording
        .schedule
        .faults()
        .windows(2)
        .any(|pair| pair[0].time_ns > pair[1].time_ns)
    {
        return Err(RecordingValidationError::ScheduleNotCanonical);
    }
    if recording.fault_stage_events.len() > MAX_FAULT_OUTCOME_EVENTS {
        return Err(RecordingValidationError::TraceBoundExceeded);
    }
    validate_fault_outcome_ledger(&recording.fault_outcome_ledger)
        .map_err(RecordingValidationError::InvalidLedger)?;
    let schedule_id = recording.schedule.identity();
    let expected_run_id = fault_run_id(recording.seed, recording.fault_run_sequence, schedule_id);
    if recording.fault_run_id != expected_run_id {
        return Err(RecordingValidationError::RecordingRunIdentityMismatch);
    }
    let mut scheduled_prefix = Vec::new();
    for state in recording.fault_outcome_ledger.attempts.values() {
        if state.attempt.schedule_id != schedule_id {
            return Err(RecordingValidationError::AttemptScheduleMismatch);
        }
        if state.attempt.run_sequence != recording.fault_run_sequence
            || state.attempt.run_id != recording.fault_run_id
        {
            return Err(RecordingValidationError::AttemptRunMismatch);
        }
        match state.attempt.source {
            FaultAttemptSource::Direct => {
                return Err(RecordingValidationError::AttemptSourceMismatch);
            }
            FaultAttemptSource::Scheduled {
                entry_index,
                scheduled_at_ns,
            } => {
                let entry_index = usize::try_from(entry_index)
                    .map_err(|_| RecordingValidationError::AttemptSourceMismatch)?;
                let entry = recording
                    .schedule
                    .entry(entry_index)
                    .ok_or(RecordingValidationError::AttemptSourceMismatch)?;
                if entry.time_ns != scheduled_at_ns
                    || entry.fault != state.attempt.fault
                    || state.attempt.selected_at_ns < scheduled_at_ns
                {
                    return Err(RecordingValidationError::AttemptSourceMismatch);
                }
                scheduled_prefix.push(entry_index);
            }
            FaultAttemptSource::Random => {
                return Err(RecordingValidationError::AttemptSourceMismatch);
            }
        }
    }
    scheduled_prefix.sort_unstable();
    if scheduled_prefix
        .iter()
        .copied()
        .enumerate()
        .any(|(expected, actual)| expected != actual)
    {
        return Err(RecordingValidationError::AttemptSourceMismatch);
    }
    for checkpoint in recording.checkpoints.all() {
        if let Some(snapshot) = &checkpoint.snapshot {
            let engine = &snapshot.fault_engine_snapshot;
            if engine.schedule_id() != schedule_id
                || engine.run_sequence() != recording.fault_run_sequence
                || engine.run_id() != recording.fault_run_id
            {
                return Err(RecordingValidationError::CheckpointRunMismatch);
            }
        }
    }
    if recording.fault_stage_events != recording.fault_outcome_ledger.events {
        return Err(RecordingValidationError::TraceLedgerMismatch);
    }

    let mut expected_start = 0_u64;
    let mut prior_tick = None;
    for delta in &recording.fault_round_deltas {
        if delta.tick > recording.total_ticks {
            return Err(RecordingValidationError::EvidenceBeyondHorizon);
        }
        if delta.event_start != expected_start
            || delta.event_start >= delta.event_end
            || usize::try_from(delta.event_end).map_or(true, |event_end| {
                event_end > recording.fault_stage_events.len()
            })
            || prior_tick.is_some_and(|tick| tick >= delta.tick)
        {
            return Err(RecordingValidationError::InvalidRoundDelta);
        }
        let event_start = usize::try_from(delta.event_start)
            .map_err(|_| RecordingValidationError::InvalidRoundDelta)?;
        let event_end = usize::try_from(delta.event_end)
            .map_err(|_| RecordingValidationError::InvalidRoundDelta)?;
        for event in &recording.fault_stage_events[event_start..event_end] {
            if event.kind == FaultStageKind::Selected {
                let state = recording
                    .fault_outcome_ledger
                    .attempts
                    .get(&event.attempt_id)
                    .ok_or(RecordingValidationError::TraceLedgerMismatch)?;
                if state.attempt.selected_at_ns % NANOSECONDS_PER_SIMULATION_TICK != 0
                    || state.attempt.selected_at_ns / NANOSECONDS_PER_SIMULATION_TICK != delta.tick
                {
                    return Err(RecordingValidationError::SelectedDeltaTickMismatch);
                }
            }
        }
        expected_start = delta.event_end;
        prior_tick = Some(delta.tick);
    }
    let trace_len = u64::try_from(recording.fault_stage_events.len())
        .map_err(|_| RecordingValidationError::TraceBoundExceeded)?;
    if expected_start != trace_len {
        return Err(RecordingValidationError::InvalidRoundDelta);
    }

    if recording.schedule_rounds.len() > MAX_RECORDED_SCHEDULE_ROUNDS {
        return Err(RecordingValidationError::ScheduleTraceBoundExceeded);
    }
    let mut schedule_record_count = 0usize;
    let mut prior_schedule_tick = None;
    let mut prior_schedule_ids: BTreeMap<usize, ScheduleStateId> = BTreeMap::new();
    for schedule_round in &recording.schedule_rounds {
        if schedule_round.tick > recording.total_ticks
            || prior_schedule_tick.is_some_and(|tick| tick >= schedule_round.tick)
            || schedule_round.traces.is_empty()
            || schedule_round.traces.len() > recording.config.num_vms
            || schedule_round.traces.len() > MAX_SCHEDULE_VCPUS
        {
            return Err(RecordingValidationError::InvalidScheduleRound);
        }
        let mut prior_vm = None;
        for vm_trace in &schedule_round.traces {
            if vm_trace.vm_index >= recording.config.num_vms
                || prior_vm.is_some_and(|vm| vm >= vm_trace.vm_index)
                || vm_trace.trace.records.is_empty()
            {
                return Err(RecordingValidationError::InvalidScheduleRound);
            }
            schedule_record_count = schedule_record_count
                .checked_add(vm_trace.trace.records.len())
                .ok_or(RecordingValidationError::ScheduleTraceBoundExceeded)?;
            if schedule_record_count > MAX_RECORDED_SCHEDULE_RECORDS {
                return Err(RecordingValidationError::ScheduleTraceBoundExceeded);
            }
            let final_state =
                validate_transition_trace(&vm_trace.trace, DEFAULT_SCHEDULE_JOURNAL_LIMIT)
                    .map_err(|_| RecordingValidationError::InvalidScheduleTrace)?;
            if prior_schedule_ids
                .get(&vm_trace.vm_index)
                .is_some_and(|prior| *prior != vm_trace.trace.initial_state_id)
            {
                return Err(RecordingValidationError::ScheduleTraceDiscontinuity);
            }
            prior_schedule_ids.insert(vm_trace.vm_index, final_state.identity());
            prior_vm = Some(vm_trace.vm_index);
        }
        prior_schedule_tick = Some(schedule_round.tick);
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
        transition_fault_outcome, FaultAttempt, FaultAttemptSource,
    };
    use chaoscontrol_vmm::scheduler::core::{
        plan_execution_observation, ExecutionProgressObservation, ProgressMode, ScheduleTrace,
    };
    use chaoscontrol_vmm::scheduler::{SchedulerConfig, SchedulingStrategy};

    const TEST_VCPU_COUNT: usize = 2;
    const TEST_INSTRUCTION_QUANTUM: u64 = 2;

    fn one_step_vm_trace(vm_index: usize) -> VmScheduleTrace {
        let config = SchedulerConfig {
            num_vcpus: TEST_VCPU_COUNT,
            quantum: TEST_INSTRUCTION_QUANTUM,
            strategy: SchedulingStrategy::RoundRobin,
            seed: 42,
        };
        let initial_state = chaoscontrol_vmm::scheduler::core::ScheduleState::new(
            &config,
            ProgressMode::ExactSingleStep,
        )
        .unwrap();
        let planned = plan_execution_observation(
            &initial_state,
            ExecutionProgressObservation::ExactInstruction { vcpu: 0 },
        )
        .unwrap()
        .unwrap();
        VmScheduleTrace {
            vm_index,
            trace: ScheduleTrace {
                initial_state_id: initial_state.identity(),
                initial_state,
                records: vec![planned.record],
            },
        }
    }

    fn selected_round(tick: u64) -> (FaultSchedule, RoundResult, FaultOutcomeLedger) {
        const TEST_SEED: u64 = 42;
        let selected_at_ns = tick * NANOSECONDS_PER_SIMULATION_TICK;
        let fault = Fault::ProcessKill { target: 0 };
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(selected_at_ns, fault.clone()));
        let schedule_id = schedule.identity();
        let attempt = FaultAttempt::new_with_source(
            fault_run_id(TEST_SEED, INITIAL_SIMULATION_RUN_SEQUENCE, schedule_id),
            INITIAL_SIMULATION_RUN_SEQUENCE,
            schedule_id,
            0,
            selected_at_ns,
            FaultAttemptSource::Scheduled {
                entry_index: 0,
                scheduled_at_ns: selected_at_ns,
            },
            fault,
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
            schedule_traces: Vec::new(),
        };
        (schedule, round, ledger)
    }

    fn selected_recording(
        schedule: FaultSchedule,
        seed: u64,
        attempts: &[FaultAttempt],
    ) -> Recording {
        let mut ledger = FaultOutcomeLedger::default();
        for attempt in attempts {
            ledger = transition_fault_outcome(
                &ledger,
                Some(attempt),
                attempt.id,
                FaultStageKind::Selected,
            )
            .unwrap();
        }
        let tick = attempts.first().map_or(0, |attempt| {
            attempt.selected_at_ns / NANOSECONDS_PER_SIMULATION_TICK
        });
        let round = RoundResult {
            tick,
            vms_running: 1,
            vms_halted: 0,
            faults_fired: attempts
                .iter()
                .map(|attempt| attempt.fault.clone())
                .collect(),
            fault_outcomes: ledger.events.clone(),
            messages_delivered: 0,
            schedule_traces: Vec::new(),
        };
        let mut recorder = Recorder::new_for_run(
            test_config(),
            schedule,
            seed,
            INITIAL_SIMULATION_RUN_SEQUENCE,
        );
        recorder.record_round(&round, &ledger).unwrap();
        recorder.recording
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
        let (schedule, round, ledger) = selected_round(1);
        let mut recorder = Recorder::new(test_config(), schedule, 42);

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
    fn recorder_persists_and_validates_exact_schedule_trace() {
        let (schedule, mut round, ledger) = selected_round(1);
        round.schedule_traces.push(one_step_vm_trace(0));
        let mut recorder = Recorder::new(test_config(), schedule, 42);

        recorder.record_round(&round, &ledger).unwrap();

        assert_eq!(recorder.recording.schedule_rounds.len(), 1);
        assert_eq!(
            recorder.recording.schedule_rounds[0].traces,
            round.schedule_traces
        );
        validate_recording(&recorder.recording).unwrap();
    }

    #[test]
    fn forged_schedule_trace_is_rejected_before_recording_mutation() {
        let (schedule, mut round, ledger) = selected_round(1);
        let mut forged = one_step_vm_trace(0);
        forged.trace.records[0].post_state_id.0[0] ^= 1;
        round.schedule_traces.push(forged);
        let mut recorder = Recorder::new(test_config(), schedule, 42);

        assert_eq!(
            recorder.record_round(&round, &ledger),
            Err(RecordingValidationError::InvalidScheduleTrace)
        );
        assert!(recorder.recording.schedule_rounds.is_empty());
        assert!(recorder.recording.fault_stage_events.is_empty());
    }

    #[test]
    fn schedule_trace_chain_discontinuity_is_rejected() {
        let (schedule, mut first_round, first_ledger) = selected_round(1);
        first_round.schedule_traces.push(one_step_vm_trace(0));
        let mut recorder = Recorder::new(test_config(), schedule, 42);
        recorder.record_round(&first_round, &first_ledger).unwrap();

        let second_round = RoundResult {
            tick: 2,
            vms_running: 1,
            vms_halted: 0,
            faults_fired: Vec::new(),
            fault_outcomes: Vec::new(),
            messages_delivered: 0,
            schedule_traces: vec![one_step_vm_trace(0)],
        };
        let second_ledger = first_ledger.clone();
        assert_eq!(
            recorder.record_round(&second_round, &second_ledger),
            Err(RecordingValidationError::ScheduleTraceDiscontinuity)
        );
    }

    #[test]
    fn tampered_recorded_trace_is_rejected() {
        let (schedule, round, ledger) = selected_round(1);
        let mut recorder = Recorder::new(test_config(), schedule, 42);
        recorder.record_round(&round, &ledger).unwrap();
        recorder.recording.fault_stage_events[0].sequence = 1;

        assert_eq!(
            validate_recording(&recorder.recording),
            Err(RecordingValidationError::TraceLedgerMismatch)
        );
    }

    #[test]
    fn legacy_fault_projection_without_selected_evidence_is_rejected() {
        let mut recorder = Recorder::new(test_config(), FaultSchedule::new(), 42);
        recorder.recording.total_ticks = 1;
        recorder.recording.events.push(RecordedEvent::FaultFired {
            tick: 1,
            fault: "unverified".to_string(),
        });

        assert_eq!(
            validate_recording(&recorder.recording),
            Err(RecordingValidationError::FaultFiredProjectionMismatch)
        );
    }

    #[test]
    fn recording_rejects_mixed_and_recomputed_run_identities() {
        const TEST_SEED: u64 = 42;
        const SECOND_RUN_SEQUENCE: u64 = 2;
        let selected_at_ns = NANOSECONDS_PER_SIMULATION_TICK;
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(
            selected_at_ns,
            Fault::ProcessKill { target: 0 },
        ));
        schedule.add(ScheduledFault::new(
            selected_at_ns,
            Fault::ProcessKill { target: 1 },
        ));
        let schedule_id = schedule.identity();
        let first = FaultAttempt::new_with_source(
            fault_run_id(TEST_SEED, INITIAL_SIMULATION_RUN_SEQUENCE, schedule_id),
            INITIAL_SIMULATION_RUN_SEQUENCE,
            schedule_id,
            0,
            selected_at_ns,
            FaultAttemptSource::Scheduled {
                entry_index: 0,
                scheduled_at_ns: selected_at_ns,
            },
            Fault::ProcessKill { target: 0 },
        );
        let second = FaultAttempt::new_with_source(
            fault_run_id(TEST_SEED, SECOND_RUN_SEQUENCE, schedule_id),
            SECOND_RUN_SEQUENCE,
            schedule_id,
            1,
            selected_at_ns,
            FaultAttemptSource::Scheduled {
                entry_index: 1,
                scheduled_at_ns: selected_at_ns,
            },
            Fault::ProcessKill { target: 1 },
        );
        let mixed = selected_recording(schedule.clone(), TEST_SEED, &[first.clone(), second]);
        assert_eq!(
            validate_recording(&mixed),
            Err(RecordingValidationError::AttemptRunMismatch)
        );

        let mut recomputed = selected_recording(schedule, TEST_SEED, &[first]);
        recomputed.fault_run_sequence = SECOND_RUN_SEQUENCE;
        recomputed.fault_run_id = fault_run_id(TEST_SEED, SECOND_RUN_SEQUENCE, schedule_id);
        assert_eq!(
            validate_recording(&recomputed),
            Err(RecordingValidationError::AttemptRunMismatch)
        );
    }

    #[test]
    fn recording_rejects_impossible_random_attempt() {
        const TEST_SEED: u64 = 42;
        let schedule = FaultSchedule::new();
        let schedule_id = schedule.identity();
        let attempt = FaultAttempt::new_with_source(
            fault_run_id(TEST_SEED, INITIAL_SIMULATION_RUN_SEQUENCE, schedule_id),
            INITIAL_SIMULATION_RUN_SEQUENCE,
            schedule_id,
            0,
            NANOSECONDS_PER_SIMULATION_TICK,
            FaultAttemptSource::Random,
            Fault::ProcessKill { target: 0 },
        );
        let recording = selected_recording(schedule, TEST_SEED, &[attempt]);

        assert_eq!(
            validate_recording(&recording),
            Err(RecordingValidationError::AttemptSourceMismatch)
        );
    }

    #[test]
    fn recording_rejects_gapped_and_duplicate_scheduled_sources() {
        const TEST_SEED: u64 = 42;
        let selected_at_ns = NANOSECONDS_PER_SIMULATION_TICK;
        let mut gap_schedule = FaultSchedule::new();
        gap_schedule.add(ScheduledFault::new(
            selected_at_ns,
            Fault::ProcessKill { target: 0 },
        ));
        gap_schedule.add(ScheduledFault::new(
            selected_at_ns,
            Fault::ProcessKill { target: 1 },
        ));
        let gap_schedule_id = gap_schedule.identity();
        let gap_attempt = FaultAttempt::new_with_source(
            fault_run_id(TEST_SEED, INITIAL_SIMULATION_RUN_SEQUENCE, gap_schedule_id),
            INITIAL_SIMULATION_RUN_SEQUENCE,
            gap_schedule_id,
            0,
            selected_at_ns,
            FaultAttemptSource::Scheduled {
                entry_index: 1,
                scheduled_at_ns: selected_at_ns,
            },
            Fault::ProcessKill { target: 1 },
        );
        let gap = selected_recording(gap_schedule, TEST_SEED, &[gap_attempt]);
        assert_eq!(
            validate_recording(&gap),
            Err(RecordingValidationError::AttemptSourceMismatch)
        );

        let mut duplicate_schedule = FaultSchedule::new();
        duplicate_schedule.add(ScheduledFault::new(
            selected_at_ns,
            Fault::ProcessKill { target: 0 },
        ));
        let duplicate_schedule_id = duplicate_schedule.identity();
        let duplicate_run_id = fault_run_id(
            TEST_SEED,
            INITIAL_SIMULATION_RUN_SEQUENCE,
            duplicate_schedule_id,
        );
        let first = FaultAttempt::new_with_source(
            duplicate_run_id,
            INITIAL_SIMULATION_RUN_SEQUENCE,
            duplicate_schedule_id,
            0,
            selected_at_ns,
            FaultAttemptSource::Scheduled {
                entry_index: 0,
                scheduled_at_ns: selected_at_ns,
            },
            Fault::ProcessKill { target: 0 },
        );
        let second = FaultAttempt::new_with_source(
            duplicate_run_id,
            INITIAL_SIMULATION_RUN_SEQUENCE,
            duplicate_schedule_id,
            1,
            selected_at_ns,
            FaultAttemptSource::Scheduled {
                entry_index: 0,
                scheduled_at_ns: selected_at_ns,
            },
            Fault::ProcessKill { target: 0 },
        );
        let duplicate = selected_recording(duplicate_schedule, TEST_SEED, &[first, second]);
        assert_eq!(
            validate_recording(&duplicate),
            Err(RecordingValidationError::AttemptSourceMismatch)
        );
    }

    #[test]
    fn recording_rejects_evidence_beyond_horizon_and_checkpoint_aliases() {
        let (schedule, round, ledger) = selected_round(1);
        let mut recorder = Recorder::new(test_config(), schedule, 42);
        recorder.record_round(&round, &ledger).unwrap();
        recorder.recording.total_ticks = 0;
        assert_eq!(
            validate_recording(&recorder.recording),
            Err(RecordingValidationError::EvidenceBeyondHorizon)
        );

        let mut checkpoint_recording =
            Recorder::new(test_config(), FaultSchedule::new(), 42).recording;
        checkpoint_recording.total_ticks = 2;
        checkpoint_recording.checkpoints.push(Checkpoint {
            id: 0,
            tick: 1,
            snapshot: None,
            serial_output: vec![],
            events_since_last: vec![],
        });
        checkpoint_recording.checkpoints.push(Checkpoint {
            id: 0,
            tick: 2,
            snapshot: None,
            serial_output: vec![],
            events_since_last: vec![],
        });
        assert_eq!(
            validate_recording(&checkpoint_recording),
            Err(RecordingValidationError::CheckpointOrderMismatch)
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
            total_runs: 1,
            ..OracleReport::empty()
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
