//! Fault injection engine — the central orchestrator.
//!
//! The [`FaultEngine`] combines the fault schedule, property oracle, and
//! deterministic RNG into a single coordinator.  The VMM calls into the
//! engine on every hypercall and on every exit to check if faults are due.

use crate::faults::{Fault, GpRegister};
use crate::oracle::{AssertionKind, PropertyOracle};
use crate::outcomes::{
    fault_run_id, transition_fault_outcome, validate_fault_outcome_ledger, FaultAttempt,
    FaultAttemptId, FaultOutcomeLedger, FaultRunId, FaultScheduleId, FaultStageKind,
    FaultTransitionError,
};
use crate::schedule::FaultSchedule;
use chaoscontrol_protocol::*;
use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use snafu::{ResultExt, Snafu};
use std::collections::{BTreeMap, HashMap};

// ═══════════════════════════════════════════════════════════════════════
//  Choice recording for input tree exploration
// ═══════════════════════════════════════════════════════════════════════

/// Record of a single random choice made by the guest via the SDK.
///
/// The explorer uses these records to identify decision points in the
/// guest's execution and generate alternative branches.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChoiceRecord {
    /// Monotonic sequence number within this engine instance.
    /// Resets when the engine is restored from a snapshot.
    pub sequence_id: u64,
    /// Number of options: `random_choice(n)` → `n`, `get_random()` → `0`.
    /// Zero indicates an unbounded random value (u64).
    pub n_options: u32,
    /// The value that was actually returned to the guest.
    pub value: u64,
}

/// Errors from the fault engine.
#[derive(Debug, Snafu)]
pub enum EngineError {
    #[snafu(display("No active run — call begin_run() first"))]
    NoActiveRun,

    #[snafu(display("Payload decode failed"))]
    PayloadDecode,

    #[snafu(display("Unknown command: {value:#x}"))]
    UnknownCommand { value: u8 },
}

/// Deterministic fault-selection failures.
#[derive(Debug, Snafu)]
pub enum FaultSelectionError {
    #[snafu(display("fault run sequence is exhausted"))]
    RunSequenceExhausted,

    #[snafu(display("fault selection sequence overflowed"))]
    SelectionSequenceOverflow,

    #[snafu(display("fault outcome transition failed: {source}"))]
    OutcomeTransition { source: FaultTransitionError },
}

/// Configuration for the fault engine.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Master seed for deterministic fault generation.
    pub seed: u64,
    /// Number of VMs in the simulation (for fault targeting).
    pub num_vms: usize,
    /// Pre-built fault schedule (optional).
    pub schedule: Option<FaultSchedule>,
    /// Whether to generate random faults in addition to scheduled ones.
    pub random_faults: bool,
    /// Mean interval between random faults (nanoseconds of virtual time).
    pub random_fault_interval_ns: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            num_vms: 1,
            schedule: None,
            random_faults: false,
            random_fault_interval_ns: 1_000_000_000, // 1 second
        }
    }
}

/// Snapshot of the engine state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EngineSnapshot {
    rng_seed: [u8; 32],
    rng_stream: u64,
    rng_word_pos: u128,
    oracle: crate::oracle::OracleSnapshot,
    schedule: crate::schedule::FaultScheduleSnapshot,
    outcomes: FaultOutcomeLedger,
    schedule_id: FaultScheduleId,
    run_id: FaultRunId,
    run_sequence: u64,
    selection_sequence: u64,
    run_exhausted: bool,
    setup_complete: bool,
    next_random_fault_time_ns: u64,
    /// Choice counter at snapshot time — restored so sequence IDs
    /// align with overrides set by the explorer.
    choice_count: u64,
}

/// The central fault injection engine.
///
/// Coordinates between the guest SDK, the property oracle, and the
/// fault schedule.  Used by the VMM to handle SDK hypercalls and to
/// query for pending faults.
///
/// # Example
///
/// ```
/// use chaoscontrol_fault::engine::{FaultEngine, EngineConfig};
/// use chaoscontrol_fault::faults::Fault;
/// use chaoscontrol_fault::schedule::FaultScheduleBuilder;
///
/// let schedule = FaultScheduleBuilder::new()
///     .at_ns(1_000_000, Fault::NetworkPartition {
///         side_a: vec![0],
///         side_b: vec![1, 2],
///     })
///     .build();
///
/// let config = EngineConfig {
///     seed: 42,
///     num_vms: 3,
///     schedule: Some(schedule),
///     ..Default::default()
/// };
///
/// let mut engine = FaultEngine::new(config);
/// engine.begin_run();
///
/// // Signal setup complete so faults can fire
/// let page = chaoscontrol_protocol::HypercallPage::zeroed();
/// let mut setup_page = page;
/// setup_page.command = chaoscontrol_protocol::CMD_LIFECYCLE_SETUP_COMPLETE;
/// engine.handle_hypercall(&setup_page);
///
/// // Check for due faults at virtual time 1ms
/// let faults = engine.poll_faults(1_000_000).unwrap();
/// assert_eq!(faults.len(), 1);
/// ```
pub struct FaultEngine {
    config: EngineConfig,
    rng: ChaCha20Rng,
    oracle: PropertyOracle,
    schedule: FaultSchedule,
    /// Ordered stage ledger for selected fault attempts.
    outcomes: FaultOutcomeLedger,
    /// Identity of the current complete schedule.
    schedule_id: FaultScheduleId,
    /// Identity of the current engine run.
    run_id: FaultRunId,
    /// Monotonic run sequence used in `run_id`.
    run_sequence: u64,
    /// Monotonic selection position within the current run.
    selection_sequence: u64,
    /// True when another unique run identity cannot be created.
    run_exhausted: bool,
    /// Whether the guest has signaled setup_complete.
    setup_complete: bool,
    /// Next time (virtual ns) to consider injecting a random fault.
    next_random_fault_time_ns: u64,
    /// History of random choices made since last drain.
    /// Used by the explorer to discover decision points.
    choice_history: Vec<ChoiceRecord>,
    /// Per-sequence overrides: `sequence_id → forced value`.
    /// When set, the override value is used instead of the RNG.
    /// The RNG token is still consumed to keep state consistent.
    random_overrides: BTreeMap<u64, u64>,
    /// Monotonic counter of random hypercalls (CMD_RANDOM_CHOICE + CMD_RANDOM_GET).
    /// Resets on restore to align with the snapshot's position.
    choice_count: u64,
    /// Last-reported guidance distance per assertion ID.
    /// Written by `CMD_GUIDANCE` hypercalls, read by the explorer.
    guidance_values: HashMap<u32, f64>,
}

impl FaultEngine {
    /// Create a new engine with the given configuration.
    pub fn new(config: EngineConfig) -> Self {
        let rng = Self::rng_from_seed(config.seed);
        let schedule = config.schedule.clone().unwrap_or_default();
        let schedule_id = schedule.identity();
        let run_sequence = 0;
        let run_id = fault_run_id(config.seed, run_sequence, schedule_id);
        let next_random_fault_time_ns = config.random_fault_interval_ns;

        Self {
            config,
            rng,
            oracle: PropertyOracle::new(),
            schedule,
            outcomes: FaultOutcomeLedger::default(),
            schedule_id,
            run_id,
            run_sequence,
            selection_sequence: 0,
            run_exhausted: false,
            setup_complete: false,
            next_random_fault_time_ns,
            choice_history: Vec::new(),
            random_overrides: BTreeMap::new(),
            choice_count: 0,
            guidance_values: HashMap::new(),
        }
    }

    /// Begin a new test run.
    pub fn begin_run(&mut self) {
        self.oracle.begin_run();
        self.setup_complete = false;
        self.schedule.reset();
        self.schedule_id = self.schedule.identity();
        self.next_random_fault_time_ns = self.config.random_fault_interval_ns;
        self.choice_history.clear();
        match self.run_sequence.checked_add(1) {
            Some(run_sequence) => {
                self.run_sequence = run_sequence;
                self.selection_sequence = 0;
                self.run_id = fault_run_id(self.config.seed, run_sequence, self.schedule_id);
                self.run_exhausted = false;
            }
            None => {
                self.run_id = fault_run_id(self.config.seed, self.run_sequence, self.schedule_id);
                self.run_exhausted = true;
            }
        }
    }

    /// End the current test run.
    pub fn end_run(&mut self) {
        self.oracle.end_run();
    }

    /// Handle a hypercall from the guest SDK.
    ///
    /// Reads the hypercall page, dispatches the command, and returns
    /// the result and status to write back.
    pub fn handle_hypercall(&mut self, page: &HypercallPage) -> (u64, u8) {
        match page.command {
            CMD_ASSERT_CATALOG => {
                let kind = page.flags;
                let message = self.decode_message(page);
                let oracle_kind = match kind {
                    0 => AssertionKind::Always,
                    1 => AssertionKind::Sometimes,
                    2 => AssertionKind::Reachable,
                    3 => AssertionKind::Unreachable,
                    _ => AssertionKind::Always,
                };
                let details: serde_json::Value =
                    serde_json::from_slice(&page.payload).unwrap_or_default();
                let guest = details
                    .get("guest")
                    .and_then(|v| v.as_str())
                    .unwrap_or("uncategorized");
                let category = details
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("uncategorized");
                self.oracle.register_catalog_entry_with_metadata(
                    page.id,
                    oracle_kind,
                    &message,
                    guest,
                    category,
                );
                (0, STATUS_OK)
            }
            CMD_ASSERT_ALWAYS => {
                let cond = page.condition();
                let (message, details) = self.decode_message_and_details(page);
                let details_ref = if cond { None } else { Some(details.as_slice()) };
                self.oracle
                    .record_always_with_details(page.id, cond, &message, details_ref);
                if cond {
                    (0, STATUS_OK)
                } else {
                    (0, STATUS_ASSERTION_FAILED)
                }
            }
            CMD_ASSERT_SOMETIMES => {
                let cond = page.condition();
                let (message, details) = self.decode_message_and_details(page);
                let details_ref = if cond { None } else { Some(details.as_slice()) };
                self.oracle
                    .record_sometimes_with_details(page.id, cond, &message, details_ref);
                (0, STATUS_OK)
            }
            CMD_ASSERT_REACHABLE => {
                let message = self.decode_message(page);
                self.oracle.record_reachable(page.id, &message);
                (0, STATUS_OK)
            }
            CMD_ASSERT_UNREACHABLE => {
                let (message, details) = self.decode_message_and_details(page);
                self.oracle.record_unreachable_with_details(
                    page.id,
                    &message,
                    Some(details.as_slice()),
                );
                (0, STATUS_UNREACHABLE_REACHED)
            }
            CMD_LIFECYCLE_SETUP_COMPLETE => {
                self.setup_complete = true;
                self.oracle.record_setup_complete();
                (0, STATUS_OK)
            }
            CMD_LIFECYCLE_SEND_EVENT => {
                let (name, json_details) = self.decode_event(page);
                let details = serde_json::from_slice::<serde_json::Value>(&json_details)
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                self.oracle.record_event(&name, details);
                (0, STATUS_OK)
            }
            CMD_RANDOM_GET => {
                let seq = self.choice_count;
                self.choice_count += 1;
                let value = if let Some(&override_val) = self.random_overrides.get(&seq) {
                    // Consume the RNG token to keep state consistent
                    // for all subsequent choices.
                    let _ = self.rng.next_u64();
                    override_val
                } else {
                    self.rng.next_u64()
                };
                self.choice_history.push(ChoiceRecord {
                    sequence_id: seq,
                    n_options: 0,
                    value,
                });
                (value, STATUS_OK)
            }
            CMD_RANDOM_CHOICE => {
                let seq = self.choice_count;
                self.choice_count += 1;
                let n = page.id; // n is passed via id field
                let value = if let Some(&override_val) = self.random_overrides.get(&seq) {
                    let _ = self.rng.next_u64();
                    if n <= 1 {
                        0
                    } else {
                        override_val % n as u64
                    }
                } else if n <= 1 {
                    0
                } else {
                    self.rng.next_u64() % n as u64
                };
                self.choice_history.push(ChoiceRecord {
                    sequence_id: seq,
                    n_options: n,
                    value,
                });
                (value, STATUS_OK)
            }
            CMD_GUIDANCE => {
                // Guest writes f64 distance into the result field.
                let distance = f64::from_le_bytes(page.result.to_le_bytes());
                self.guidance_values.insert(page.id, distance);
                (0, STATUS_OK)
            }
            _cmd => {
                // Unknown command — return error
                (0, STATUS_ERROR)
            }
        }
    }

    /// Select due fault attempts without claiming application or observation.
    ///
    /// r[impl chaoscontrol.fault_outcomes.accounting]
    pub fn poll_fault_attempts(
        &mut self,
        current_time_ns: u64,
    ) -> Result<Vec<FaultAttempt>, FaultSelectionError> {
        if !self.setup_complete {
            return Ok(Vec::new());
        }
        if self.run_exhausted {
            return RunSequenceExhaustedSnafu.fail();
        }

        let mut faults = Vec::new();
        for scheduled in self.schedule.drain_due(current_time_ns) {
            faults.push(scheduled.fault);
        }
        if self.config.random_faults && current_time_ns >= self.next_random_fault_time_ns {
            if let Some(fault) = self.generate_random_fault() {
                faults.push(fault);
            }
            self.next_random_fault_time_ns =
                current_time_ns.saturating_add(self.config.random_fault_interval_ns);
        }

        let mut attempts = Vec::with_capacity(faults.len());
        for fault in faults {
            attempts.push(self.select_fault(fault, current_time_ns)?);
        }
        Ok(attempts)
    }

    /// Compatibility selector mapped exactly to the selected stage.
    pub fn poll_faults(&mut self, current_time_ns: u64) -> Result<Vec<Fault>, FaultSelectionError> {
        self.poll_fault_attempts(current_time_ns)
            .map(|attempts| attempts.into_iter().map(|attempt| attempt.fault).collect())
    }

    fn select_fault(
        &mut self,
        fault: Fault,
        selected_at_ns: u64,
    ) -> Result<FaultAttempt, FaultSelectionError> {
        let selection_sequence = self.selection_sequence;
        let next_selection_sequence = selection_sequence
            .checked_add(1)
            .ok_or(FaultSelectionError::SelectionSequenceOverflow)?;
        let attempt = FaultAttempt::new(
            self.run_id,
            self.schedule_id,
            selection_sequence,
            selected_at_ns,
            fault,
        );
        let next = transition_fault_outcome(
            &self.outcomes,
            Some(&attempt),
            attempt.id,
            FaultStageKind::Selected,
        )
        .context(OutcomeTransitionSnafu)?;
        self.outcomes = next;
        self.selection_sequence = next_selection_sequence;
        Ok(attempt)
    }

    /// Whether the current run has an immediate assertion failure.
    pub fn has_assertion_failure(&self) -> bool {
        self.oracle.has_immediate_failure()
    }

    /// Get a reference to the property oracle.
    pub fn oracle(&self) -> &PropertyOracle {
        &self.oracle
    }

    /// Get a mutable reference to the property oracle.
    pub fn oracle_mut(&mut self) -> &mut PropertyOracle {
        &mut self.oracle
    }

    /// Legacy alias mapped exactly to the selected-stage counter.
    pub fn faults_injected(&self) -> u64 {
        self.outcomes.counters.selected
    }

    /// Return the authoritative ordered stage ledger.
    pub fn fault_outcomes(&self) -> &FaultOutcomeLedger {
        &self.outcomes
    }

    /// Record one validated stage from the imperative application shell.
    pub fn record_fault_stage(
        &mut self,
        attempt_id: FaultAttemptId,
        kind: FaultStageKind,
    ) -> Result<(), FaultTransitionError> {
        let next = transition_fault_outcome(&self.outcomes, None, attempt_id, kind)?;
        self.outcomes = next;
        Ok(())
    }

    /// Whether setup_complete has been received for the current run.
    pub fn is_setup_complete(&self) -> bool {
        self.setup_complete
    }

    /// Force setup_complete to true.
    ///
    /// Use this in integration tests where the guest doesn't use the SDK
    /// but you still want faults to fire on schedule.
    pub fn force_setup_complete(&mut self) {
        self.setup_complete = true;
    }

    /// Reset setup_complete to false (used during VM restart).
    pub fn reset_setup_complete(&mut self) {
        self.setup_complete = false;
    }

    /// Replace the fault schedule (for exploration branch mutations).
    pub fn set_schedule(&mut self, schedule: FaultSchedule) {
        self.schedule = schedule;
        self.schedule_id = self.schedule.identity();
        self.run_id = fault_run_id(self.config.seed, self.run_sequence, self.schedule_id);
        self.selection_sequence = 0;
    }

    /// Snapshot the engine state.
    pub fn snapshot(&self) -> EngineSnapshot {
        EngineSnapshot {
            rng_seed: self.rng.get_seed(),
            rng_stream: self.rng.get_stream(),
            rng_word_pos: self.rng.get_word_pos(),
            oracle: self.oracle.snapshot(),
            schedule: self.schedule.snapshot(),
            outcomes: self.outcomes.clone(),
            schedule_id: self.schedule_id,
            run_id: self.run_id,
            run_sequence: self.run_sequence,
            selection_sequence: self.selection_sequence,
            run_exhausted: self.run_exhausted,
            setup_complete: self.setup_complete,
            next_random_fault_time_ns: self.next_random_fault_time_ns,
            choice_count: self.choice_count,
        }
    }

    /// Validate an untrusted engine snapshot without changing live state.
    pub fn validate_snapshot(&self, snapshot: &EngineSnapshot) -> Result<(), FaultTransitionError> {
        validate_fault_outcome_ledger(&snapshot.outcomes)?;
        let mut restored_schedule = self.schedule.clone();
        restored_schedule.restore(&snapshot.schedule);
        if restored_schedule.identity() != snapshot.schedule_id {
            return Err(FaultTransitionError::SnapshotScheduleIdentityMismatch);
        }
        if snapshot.run_id
            != fault_run_id(
                self.config.seed,
                snapshot.run_sequence,
                snapshot.schedule_id,
            )
        {
            return Err(FaultTransitionError::SnapshotRunIdentityMismatch);
        }
        let mut current_indices = snapshot
            .outcomes
            .attempts
            .values()
            .filter(|state| {
                state.attempt.run_id == snapshot.run_id
                    && state.attempt.schedule_id == snapshot.schedule_id
            })
            .map(|state| state.attempt.selection_index)
            .collect::<Vec<_>>();
        current_indices.sort_unstable();
        let index_count = u64::try_from(current_indices.len())
            .map_err(|_| FaultTransitionError::SnapshotSelectionSequenceMismatch)?;
        if index_count != snapshot.selection_sequence
            || current_indices
                .iter()
                .copied()
                .enumerate()
                .any(|(expected, actual)| u64::try_from(expected).ok() != Some(actual))
        {
            return Err(FaultTransitionError::SnapshotSelectionSequenceMismatch);
        }
        Ok(())
    }

    /// Validate and restore engine state from an untrusted snapshot.
    pub fn restore(&mut self, snapshot: &EngineSnapshot) -> Result<(), FaultTransitionError> {
        self.validate_snapshot(snapshot)?;
        let mut restored_schedule = self.schedule.clone();
        restored_schedule.restore(&snapshot.schedule);
        let mut restored_rng = ChaCha20Rng::from_seed(snapshot.rng_seed);
        restored_rng.set_stream(snapshot.rng_stream);
        restored_rng.set_word_pos(snapshot.rng_word_pos);
        self.rng = restored_rng;
        self.oracle.restore(&snapshot.oracle);
        self.schedule = restored_schedule;
        self.outcomes = snapshot.outcomes.clone();
        self.schedule_id = snapshot.schedule_id;
        self.run_id = snapshot.run_id;
        self.run_sequence = snapshot.run_sequence;
        self.selection_sequence = snapshot.selection_sequence;
        self.run_exhausted = snapshot.run_exhausted;
        self.setup_complete = snapshot.setup_complete;
        self.next_random_fault_time_ns = snapshot.next_random_fault_time_ns;
        self.choice_count = snapshot.choice_count;
        // Clear history — new run starts recording fresh.
        // random_overrides are NOT cleared — they're set externally
        // before each branch by the explorer.
        self.choice_history.clear();
        Ok(())
    }

    // ── Input tree exploration ────────────────────────────────

    /// Drain the choice history — returns all records and clears the buffer.
    ///
    /// The explorer calls this after each branch run to examine what
    /// random decisions the guest made.
    pub fn drain_choice_history(&mut self) -> Vec<ChoiceRecord> {
        std::mem::take(&mut self.choice_history)
    }

    /// Set overrides for specific choice sequence positions.
    ///
    /// On the next run, when `choice_count` reaches a key in this map,
    /// that value is returned to the guest instead of the RNG's value.
    /// The RNG token is still consumed to keep subsequent state consistent.
    pub fn set_random_overrides(&mut self, overrides: BTreeMap<u64, u64>) {
        self.random_overrides = overrides;
    }

    /// Clear all random overrides.
    pub fn clear_random_overrides(&mut self) {
        self.random_overrides.clear();
    }

    /// Get the current choice sequence counter.
    pub fn choice_count(&self) -> u64 {
        self.choice_count
    }

    /// Get the last-reported guidance distances.
    ///
    /// Maps assertion IDs to their most recent distance-to-violation
    /// hint.  The explorer reads this after each execution quantum to
    /// guide mutation toward property violations.
    pub fn guidance_values(&self) -> &HashMap<u32, f64> {
        &self.guidance_values
    }

    // ── Internal ────────────────────────────────────────────────

    fn rng_from_seed(seed: u64) -> ChaCha20Rng {
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&seed.to_le_bytes());
        ChaCha20Rng::from_seed(key)
    }

    fn decode_message(&self, page: &HypercallPage) -> String {
        let payload_len = page.payload_len as usize;
        if payload_len == 0 {
            return String::new();
        }
        let buf = &page.payload[..payload_len.min(PAYLOAD_MAX)];
        decode_payload(buf).map(|p| p.message).unwrap_or_default()
    }

    fn decode_message_and_details(&self, page: &HypercallPage) -> (String, Vec<u8>) {
        let payload_len = page.payload_len as usize;
        if payload_len == 0 {
            return (String::new(), b"{}".to_vec());
        }
        let buf = &page.payload[..payload_len.min(PAYLOAD_MAX)];
        decode_payload(buf)
            .map(|p| (p.message, p.json_details))
            .unwrap_or_else(|| (String::new(), b"{}".to_vec()))
    }

    fn decode_event(&self, page: &HypercallPage) -> (String, Vec<u8>) {
        let payload_len = page.payload_len as usize;
        if payload_len == 0 {
            return (String::new(), b"{}".to_vec());
        }
        let buf = &page.payload[..payload_len.min(PAYLOAD_MAX)];
        decode_payload(buf)
            .map(|p| (p.message, p.json_details))
            .unwrap_or_else(|| (String::new(), b"{}".to_vec()))
    }

    fn generate_random_fault(&mut self) -> Option<Fault> {
        if self.config.num_vms == 0 {
            return None;
        }

        let target = (self.rng.next_u64() as usize) % self.config.num_vms;
        let fault_type = self.rng.next_u64() % 20;

        Some(match fault_type {
            0 => Fault::ProcessKill { target },
            1 => Fault::ProcessPause {
                target,
                duration_ns: (self.rng.next_u64() % 5_000_000_000) + 100_000_000,
            },
            2 => {
                // Network partition: target vs everyone else
                let side_a = vec![target];
                let side_b = (0..self.config.num_vms).filter(|&i| i != target).collect();
                Fault::NetworkPartition { side_a, side_b }
            }
            3 => Fault::NetworkHeal,
            4 => Fault::PacketLoss {
                target,
                rate_ppm: ((self.rng.next_u64() % 500_000) + 10_000) as u32,
            },
            5 => Fault::DiskWriteError {
                target,
                offset: self.rng.next_u64() % (1024 * 1024),
            },
            6 => Fault::DiskTornWrite {
                target,
                offset: self.rng.next_u64() % (1024 * 1024),
                bytes_written: ((self.rng.next_u64() % 511) + 1) as usize,
            },
            7 => Fault::ClockSkew {
                target,
                offset_ns: (self.rng.next_u64() % 10_000_000_000) as i64 - 5_000_000_000,
            },
            8 => Fault::NetworkJitter {
                target,
                jitter_ns: (self.rng.next_u64() % 50_000_000) + 1_000_000, // 1–51 ms
            },
            9 => Fault::NetworkBandwidth {
                target,
                bytes_per_sec: (self.rng.next_u64() % 10_000_000) + 100_000, // 100 KB/s–10 MB/s
            },
            10 => Fault::PacketDuplicate {
                target,
                rate_ppm: ((self.rng.next_u64() % 200_000) + 10_000) as u32, // 1–21 %
            },
            11 => Fault::InjectInterrupt {
                target,
                irq: (self.rng.next_u64() % 8) as u32, // 0-7: PIT, serial, virtio
            },
            12 => Fault::InjectNmi {
                target,
                vcpu: 0, // BSP — SMP-aware targeting is future work
            },
            13 => Fault::DiskSlow {
                target,
                delay_ns: (self.rng.next_u64() % 50_000_000) + 1_000_000, // 1–51 ms
            },
            14 => Fault::DiskFsyncLie { target },
            15 => Fault::DiskPartialRead {
                target,
                offset: self.rng.next_u64() % (1024 * 1024),
                max_bytes: ((self.rng.next_u64() % 4095) + 1) as usize,
            },
            16 => Fault::CpuBitflip {
                target,
                vcpu: 0,
                register: GpRegister::ALL[(self.rng.next_u64() % 16) as usize],
                bit: (self.rng.next_u64() % 64) as u8,
            },
            17 => Fault::CpuStall {
                target,
                vcpu: 0,
                duration_ticks: (self.rng.next_u64() % 200) + 1,
            },
            18 => Fault::ClockFreeze {
                target,
                duration_ticks: (self.rng.next_u64() % 500) + 10,
            },
            19 => Fault::ClockJitter {
                target,
                bound_tsc: (self.rng.next_u64() % 5000) + 100,
            },
            _ => unreachable!(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::FaultScheduleBuilder;

    fn make_page(command: u8, flags: u8, id: u32) -> HypercallPage {
        let mut page = HypercallPage::zeroed();
        page.command = command;
        page.flags = flags;
        page.id = id;
        page
    }

    fn make_page_with_payload(
        command: u8,
        flags: u8,
        id: u32,
        message: &str,
        json_details: &[u8],
    ) -> HypercallPage {
        let mut page = make_page(command, flags, id);
        if let Some(len) = encode_payload(&mut page.payload, message, json_details) {
            page.payload_len = len as u16;
        }
        page
    }

    #[test]
    fn handle_assert_always_true() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page_with_payload(CMD_ASSERT_ALWAYS, 0x01, 1, "test", b"{}");
        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_OK);
        assert!(!engine.has_assertion_failure());
    }

    #[test]
    fn handle_assert_always_false() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page_with_payload(CMD_ASSERT_ALWAYS, 0x00, 1, "bad", b"{}");
        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_ASSERTION_FAILED);
        assert!(engine.has_assertion_failure());
    }

    #[test]
    fn handle_assert_sometimes() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page_with_payload(CMD_ASSERT_SOMETIMES, 0x00, 1, "rare", b"{}");
        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_OK); // sometimes(false) is not an immediate failure
    }

    #[test]
    fn handle_assert_unreachable() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page_with_payload(CMD_ASSERT_UNREACHABLE, 0x00, 1, "impossible", b"{}");
        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_UNREACHABLE_REACHED);
        assert!(engine.has_assertion_failure());
    }

    #[test]
    fn handle_random_get() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page(CMD_RANDOM_GET, 0, 0);
        let (val1, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_OK);

        let (val2, _) = engine.handle_hypercall(&page);
        assert_ne!(val1, val2); // Different random values
    }

    #[test]
    fn random_deterministic_with_same_seed() {
        let mut e1 = FaultEngine::new(EngineConfig {
            seed: 123,
            ..Default::default()
        });
        let mut e2 = FaultEngine::new(EngineConfig {
            seed: 123,
            ..Default::default()
        });
        e1.begin_run();
        e2.begin_run();

        let page = make_page(CMD_RANDOM_GET, 0, 0);
        for _ in 0..10 {
            let (v1, _) = e1.handle_hypercall(&page);
            let (v2, _) = e2.handle_hypercall(&page);
            assert_eq!(v1, v2);
        }
    }

    #[test]
    fn random_different_with_different_seed() {
        let mut e1 = FaultEngine::new(EngineConfig {
            seed: 1,
            ..Default::default()
        });
        let mut e2 = FaultEngine::new(EngineConfig {
            seed: 2,
            ..Default::default()
        });
        e1.begin_run();
        e2.begin_run();

        let page = make_page(CMD_RANDOM_GET, 0, 0);
        let (v1, _) = e1.handle_hypercall(&page);
        let (v2, _) = e2.handle_hypercall(&page);
        assert_ne!(v1, v2);
    }

    #[test]
    fn handle_random_choice() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page(CMD_RANDOM_CHOICE, 0, 5); // Choose from 0..5
        let (val, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_OK);
        assert!(val < 5);
    }

    #[test]
    fn setup_complete_gates_faults() {
        let schedule = FaultScheduleBuilder::new()
            .at_ns(0, Fault::ProcessKill { target: 0 })
            .build();

        let mut engine = FaultEngine::new(EngineConfig {
            schedule: Some(schedule),
            ..Default::default()
        });
        engine.begin_run();

        // Before setup_complete: no faults
        let faults = engine.poll_faults(1_000_000).unwrap();
        assert!(faults.is_empty());

        // After setup_complete: faults are selected.
        let page = make_page(CMD_LIFECYCLE_SETUP_COMPLETE, 0, 0);
        engine.handle_hypercall(&page);
        let faults = engine.poll_faults(1_000_000).unwrap();
        assert_eq!(faults.len(), 1);
    }

    #[test]
    fn selection_sequence_overflow_does_not_commit_selected_attempt() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();
        engine.selection_sequence = u64::MAX;
        let before = engine.fault_outcomes().clone();

        let result = engine.select_fault(Fault::ProcessKill { target: 0 }, 0);

        assert!(matches!(
            result,
            Err(FaultSelectionError::SelectionSequenceOverflow)
        ));
        assert_eq!(engine.fault_outcomes(), &before);
        assert_eq!(engine.selection_sequence, u64::MAX);
    }

    #[test]
    fn scheduled_faults_fire_at_correct_time() {
        let schedule = FaultScheduleBuilder::new()
            .at_ns(1000, Fault::NetworkHeal)
            .at_ns(2000, Fault::ProcessKill { target: 0 })
            .build();

        let mut engine = FaultEngine::new(EngineConfig {
            schedule: Some(schedule),
            ..Default::default()
        });
        engine.begin_run();
        engine.setup_complete = true;

        let faults = engine.poll_faults(500).unwrap();
        assert!(faults.is_empty());

        let faults = engine.poll_faults(1500).unwrap();
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0], Fault::NetworkHeal);

        let faults = engine.poll_faults(3000).unwrap();
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0], Fault::ProcessKill { target: 0 });
    }

    #[test]
    fn snapshot_restore_preserves_attempts_and_stage_counters() {
        // r[verify chaoscontrol.fault_outcomes.validation.replay]
        use crate::outcomes::{
            FaultObservation, FaultObservationEffect, FaultObservationSubsystem, FaultStageKind,
        };

        let schedule = FaultScheduleBuilder::new()
            .at_ns(0, Fault::ProcessKill { target: 0 })
            .build();
        let mut engine = FaultEngine::new(EngineConfig {
            schedule: Some(schedule),
            ..Default::default()
        });
        engine.begin_run();
        engine.force_setup_complete();
        let attempts = engine.poll_fault_attempts(0).unwrap();
        let attempt_id = attempts[0].id;
        engine
            .record_fault_stage(
                attempt_id,
                FaultStageKind::Applicable {
                    effect: crate::outcomes::FaultPlanEffect::ProcessKill { target: 0 },
                },
            )
            .unwrap();
        engine
            .record_fault_stage(
                attempt_id,
                FaultStageKind::Applied {
                    effect: crate::outcomes::FaultPlanEffect::ProcessKill { target: 0 },
                },
            )
            .unwrap();
        let snapshot = engine.snapshot();
        let expected = engine.fault_outcomes().clone();

        engine
            .record_fault_stage(
                attempt_id,
                FaultStageKind::Observed {
                    observation: FaultObservation::new(
                        attempt_id,
                        FaultObservationSubsystem::Process,
                        0,
                        FaultObservationEffect::ProcessSkipped,
                    ),
                },
            )
            .unwrap();
        assert_eq!(engine.fault_outcomes().events.len(), 4);
        engine.restore(&snapshot).unwrap();

        assert_eq!(engine.fault_outcomes(), &expected);
        assert_eq!(engine.fault_outcomes().events.len(), 3);
        assert_eq!(engine.fault_outcomes().counters.selected, 1);
        assert_eq!(engine.fault_outcomes().counters.applied, 1);
    }

    #[test]
    fn tampered_snapshot_ledger_is_rejected_without_replacing_live_state() {
        let schedule = FaultScheduleBuilder::new()
            .at_ns(0, Fault::ProcessKill { target: 0 })
            .build();
        let mut engine = FaultEngine::new(EngineConfig {
            schedule: Some(schedule),
            ..Default::default()
        });
        engine.begin_run();
        engine.force_setup_complete();
        engine.poll_fault_attempts(0).unwrap();
        let before = engine.fault_outcomes().clone();
        let mut tampered = engine.snapshot();
        tampered.outcomes.events[0].sequence = 1;

        let result = engine.restore(&tampered);

        assert_eq!(result, Err(FaultTransitionError::EventSequenceMismatch));
        assert_eq!(engine.fault_outcomes(), &before);
    }

    #[test]
    fn exhausted_snapshot_still_requires_derived_run_identity() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.run_sequence = u64::MAX - 1;
        engine.begin_run();
        engine.begin_run();
        assert!(engine.run_exhausted);
        let valid = engine.snapshot();
        assert!(engine.validate_snapshot(&valid).is_ok());

        let mut tampered = valid;
        tampered.run_id = FaultRunId([0; 32]);
        assert_eq!(
            engine.validate_snapshot(&tampered),
            Err(FaultTransitionError::SnapshotRunIdentityMismatch)
        );
    }

    #[test]
    fn snapshot_rejects_duplicate_current_run_selection_indices() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();
        let first = FaultAttempt::new(
            engine.run_id,
            engine.schedule_id,
            0,
            0,
            Fault::ProcessKill { target: 0 },
        );
        let second = FaultAttempt::new(engine.run_id, engine.schedule_id, 0, 1, Fault::NetworkHeal);
        let ledger = transition_fault_outcome(
            &FaultOutcomeLedger::default(),
            Some(&first),
            first.id,
            FaultStageKind::Selected,
        )
        .unwrap();
        let ledger =
            transition_fault_outcome(&ledger, Some(&second), second.id, FaultStageKind::Selected)
                .unwrap();
        let mut snapshot = engine.snapshot();
        snapshot.outcomes = ledger;
        snapshot.selection_sequence = 2;

        assert_eq!(
            engine.validate_snapshot(&snapshot),
            Err(FaultTransitionError::SnapshotSelectionSequenceMismatch)
        );
    }

    #[test]
    fn snapshot_restore_engine() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        // Record some state
        let page = make_page(CMD_RANDOM_GET, 0, 0);
        let (v1, _) = engine.handle_hypercall(&page);

        let snap = engine.snapshot();

        // Advance further
        let (v2, _) = engine.handle_hypercall(&page);
        assert_ne!(v1, v2);

        // Restore and verify same next value
        engine.restore(&snap).unwrap();
        engine.begin_run();
        let (v3, _) = engine.handle_hypercall(&page);
        assert_eq!(v2, v3);
    }

    #[test]
    fn cross_run_oracle_report() {
        let mut engine = FaultEngine::new(EngineConfig::default());

        // Run 0: always=true, sometimes=false
        engine.begin_run();
        let p1 = make_page_with_payload(CMD_ASSERT_ALWAYS, 0x01, 1, "stable", b"{}");
        let p2 = make_page_with_payload(CMD_ASSERT_SOMETIMES, 0x00, 2, "rare", b"{}");
        engine.handle_hypercall(&p1);
        engine.handle_hypercall(&p2);
        engine.end_run();

        // Run 1: always=true, sometimes=true
        engine.begin_run();
        engine.handle_hypercall(&p1);
        let p2_true = make_page_with_payload(CMD_ASSERT_SOMETIMES, 0x01, 2, "rare", b"{}");
        engine.handle_hypercall(&p2_true);
        engine.end_run();

        let report = engine.oracle().report();
        assert_eq!(report.total_runs, 2);
        assert_eq!(report.passed, 2); // both pass cross-run
        assert_eq!(report.failed, 0);
    }

    #[test]
    fn unknown_command_returns_error() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page(0xFF, 0, 0);
        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_ERROR);
    }

    // ── Input tree exploration tests ────────────────────────────

    #[test]
    fn choice_history_recorded() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        // random_choice(5)
        let page = make_page(CMD_RANDOM_CHOICE, 0, 5);
        let (val, _) = engine.handle_hypercall(&page);

        // get_random()
        let page2 = make_page(CMD_RANDOM_GET, 0, 0);
        let (val2, _) = engine.handle_hypercall(&page2);

        let history = engine.drain_choice_history();
        assert_eq!(history.len(), 2);

        assert_eq!(history[0].sequence_id, 0);
        assert_eq!(history[0].n_options, 5);
        assert_eq!(history[0].value, val);

        assert_eq!(history[1].sequence_id, 1);
        assert_eq!(history[1].n_options, 0); // get_random
        assert_eq!(history[1].value, val2);

        assert_eq!(engine.choice_count(), 2);
    }

    #[test]
    fn drain_choice_history_clears() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page(CMD_RANDOM_CHOICE, 0, 3);
        engine.handle_hypercall(&page);

        let h1 = engine.drain_choice_history();
        assert_eq!(h1.len(), 1);

        // Second drain is empty
        let h2 = engine.drain_choice_history();
        assert!(h2.is_empty());

        // But choice_count persists
        assert_eq!(engine.choice_count(), 1);
    }

    #[test]
    fn random_override_forces_value() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        // Override sequence 0 → force value 2
        let mut overrides = BTreeMap::new();
        overrides.insert(0, 2);
        engine.set_random_overrides(overrides);

        let page = make_page(CMD_RANDOM_CHOICE, 0, 5);
        let (val, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_OK);
        assert_eq!(val, 2); // Forced!

        let history = engine.drain_choice_history();
        assert_eq!(history[0].value, 2);
    }

    #[test]
    fn random_override_clamps_to_n() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        // Override with value 99, but n=3 → 99 % 3 = 0
        let mut overrides = BTreeMap::new();
        overrides.insert(0, 99);
        engine.set_random_overrides(overrides);

        let page = make_page(CMD_RANDOM_CHOICE, 0, 3);
        let (val, _) = engine.handle_hypercall(&page);
        assert_eq!(val, 0); // 99 % 3 = 0
    }

    #[test]
    fn random_override_preserves_rng_state() {
        // Two engines with same seed. One uses override at seq 0,
        // the other uses normal RNG. After seq 0, both should
        // produce the same values (RNG token consumed either way).
        let mut e1 = FaultEngine::new(EngineConfig {
            seed: 42,
            ..Default::default()
        });
        let mut e2 = FaultEngine::new(EngineConfig {
            seed: 42,
            ..Default::default()
        });
        e1.begin_run();
        e2.begin_run();

        // Override seq 0 on e1 only
        let mut overrides = BTreeMap::new();
        overrides.insert(0, 999);
        e1.set_random_overrides(overrides);

        // Seq 0: different values
        let page = make_page(CMD_RANDOM_CHOICE, 0, 1000);
        let (v1_0, _) = e1.handle_hypercall(&page);
        let (v2_0, _) = e2.handle_hypercall(&page);
        assert_eq!(v1_0, 999); // override
        assert_ne!(v2_0, 999); // natural

        // Seq 1: SAME values (RNG state in sync)
        let (v1_1, _) = e1.handle_hypercall(&page);
        let (v2_1, _) = e2.handle_hypercall(&page);
        assert_eq!(v1_1, v2_1);
    }

    #[test]
    fn choice_count_survives_snapshot() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page(CMD_RANDOM_CHOICE, 0, 5);
        engine.handle_hypercall(&page);
        engine.handle_hypercall(&page);
        assert_eq!(engine.choice_count(), 2);

        let snap = engine.snapshot();

        // Advance further
        engine.handle_hypercall(&page);
        assert_eq!(engine.choice_count(), 3);

        // Restore → back to 2
        engine.restore(&snap).unwrap();
        assert_eq!(engine.choice_count(), 2);

        // History cleared on restore
        assert!(engine.drain_choice_history().is_empty());
    }

    // ── Guidance tests ──────────────────────────────────────────────

    #[test]
    fn guidance_stores_value() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let val = 2.72f64;
        let mut page = make_page(CMD_GUIDANCE, 0, 0xABCD);
        page.result = u64::from_le_bytes(val.to_le_bytes());
        let (_, status) = engine.handle_hypercall(&page);

        assert_eq!(status, STATUS_OK);
        assert_eq!(engine.guidance_values().get(&0xABCD), Some(&val));
    }

    #[test]
    fn guidance_overwrites_value() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let val1 = 2.72f64;
        let mut page = make_page(CMD_GUIDANCE, 0, 0xABCD);
        page.result = u64::from_le_bytes(val1.to_le_bytes());
        engine.handle_hypercall(&page);

        page.result = u64::from_le_bytes(1.0f64.to_le_bytes());
        engine.handle_hypercall(&page);

        assert_eq!(engine.guidance_values().get(&0xABCD), Some(&1.0));
    }

    #[test]
    fn guidance_stores_nan() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let mut page = make_page(CMD_GUIDANCE, 0, 42);
        page.result = u64::from_le_bytes(f64::NAN.to_le_bytes());
        let (_, status) = engine.handle_hypercall(&page);

        assert_eq!(status, STATUS_OK);
        let val = engine.guidance_values().get(&42).unwrap();
        assert!(val.is_nan());
    }

    #[test]
    fn guidance_multiple_assertions() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let mut p1 = make_page(CMD_GUIDANCE, 0, 1);
        p1.result = u64::from_le_bytes(10.0f64.to_le_bytes());
        engine.handle_hypercall(&p1);

        let mut p2 = make_page(CMD_GUIDANCE, 0, 2);
        p2.result = u64::from_le_bytes(0.0f64.to_le_bytes());
        engine.handle_hypercall(&p2);

        assert_eq!(engine.guidance_values().len(), 2);
        assert_eq!(engine.guidance_values().get(&1), Some(&10.0));
        assert_eq!(engine.guidance_values().get(&2), Some(&0.0));
    }

    #[test]
    fn overrides_persist_across_restore() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        // Set override
        let mut overrides = BTreeMap::new();
        overrides.insert(0, 42);
        engine.set_random_overrides(overrides);

        // Take snapshot and restore
        let snap = engine.snapshot();
        engine.restore(&snap).unwrap();

        // Override still active
        let page = make_page(CMD_RANDOM_GET, 0, 0);
        let (val, _) = engine.handle_hypercall(&page);
        assert_eq!(val, 42);
    }

    #[test]
    fn get_random_override() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let mut overrides = BTreeMap::new();
        overrides.insert(0, 0xDEAD_BEEF);
        engine.set_random_overrides(overrides);

        let page = make_page(CMD_RANDOM_GET, 0, 0);
        let (val, _) = engine.handle_hypercall(&page);
        assert_eq!(val, 0xDEAD_BEEF);
    }
}
