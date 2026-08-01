//! Fault injection engine — the central orchestrator.
//!
//! The [`FaultEngine`] combines the fault schedule, property oracle, and
//! deterministic RNG into a single coordinator.  The VMM calls into the
//! engine on every hypercall and on every exit to check if faults are due.

use crate::faults::{Fault, GpRegister};
use crate::oracle::{AssertionKind, PropertyOracle};
use crate::schedule::FaultSchedule;
use chaoscontrol_protocol::admission::{BoundAssertionEvent, CatalogBuilder, CatalogConflict};
use chaoscontrol_protocol::transport::{
    decode_catalog_begin, decode_catalog_complete, decode_descriptor_frame, decode_event_frame,
};
use chaoscontrol_protocol::*;
use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use snafu::Snafu;
use std::collections::BTreeMap;

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
#[serde(deny_unknown_fields)]
pub struct EngineSnapshot {
    rng_seed: [u8; 32],
    rng_stream: u64,
    rng_word_pos: u128,
    oracle: crate::oracle::OracleSnapshot,
    schedule: crate::schedule::FaultScheduleSnapshot,
    faults_injected: u64,
    setup_complete: bool,
    next_random_fault_time_ns: u64,
    /// Choice counter at snapshot time — restored so sequence IDs
    /// align with overrides set by the explorer.
    choice_count: u64,
}

pub fn validate_engine_snapshot(
    snapshot: &EngineSnapshot,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    crate::oracle_validation::validate_restorable_oracle_snapshot(&snapshot.oracle)?;
    let oracle_setup_complete = snapshot
        .oracle
        .current_run
        .as_ref()
        .is_some_and(|run| run.setup_complete);
    if snapshot.setup_complete != oracle_setup_complete {
        return Err(crate::oracle_validation::OracleValidationError::Status);
    }
    Ok(())
}

pub fn validate_engine_snapshot_assertion_evidence(
    snapshot: &EngineSnapshot,
    identity: &chaoscontrol_protocol::admission::AssertionEvidenceIdentity,
) -> Result<(), crate::oracle_validation::OracleValidationError> {
    validate_engine_snapshot(snapshot)?;
    crate::oracle_snapshot_validation::resolve_snapshot_assertion_evidence(
        &snapshot.oracle,
        identity,
    )
    .map(|_| ())
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
/// let faults = engine.poll_faults(1_000_000);
/// assert_eq!(faults.len(), 1);
/// ```
pub struct FaultEngine {
    config: EngineConfig,
    rng: ChaCha20Rng,
    oracle: PropertyOracle,
    schedule: FaultSchedule,
    /// Total faults injected across all runs.
    faults_injected: u64,
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
    /// Pending strict assertion catalog. It becomes authoritative only at completion.
    catalog_builder: Option<CatalogBuilder>,
}

impl FaultEngine {
    /// Create a new engine with the given configuration.
    pub fn new(config: EngineConfig) -> Self {
        let rng = Self::rng_from_seed(config.seed);
        let schedule = config.schedule.clone().unwrap_or_default();
        let next_random_fault_time_ns = config.random_fault_interval_ns;

        Self {
            config,
            rng,
            oracle: PropertyOracle::new(),
            schedule,
            faults_injected: 0,
            setup_complete: false,
            next_random_fault_time_ns,
            choice_history: Vec::new(),
            random_overrides: BTreeMap::new(),
            choice_count: 0,
            catalog_builder: None,
        }
    }

    /// Begin a new test run.
    pub fn begin_run(&mut self) {
        self.catalog_builder = None;
        self.oracle.begin_run();
        self.setup_complete = false;
        self.schedule.reset();
        self.next_random_fault_time_ns = self.config.random_fault_interval_ns;
        self.choice_history.clear();
    }

    /// End the current test run.
    pub fn end_run(&mut self) {
        if self.catalog_builder.take().is_some() {
            self.oracle
                .mark_identity_conflict(CatalogConflict::CatalogIncomplete);
        }
        self.oracle.end_run();
    }

    /// Handle a hypercall from the guest SDK.
    ///
    /// Reads the hypercall page, dispatches the command, and returns
    /// the result and status to write back.
    pub fn handle_hypercall(&mut self, page: &HypercallPage) -> (u64, u8) {
        match page.command {
            CMD_ASSERT_CATALOG_BEGIN => self.handle_catalog_begin(page),
            CMD_ASSERT_CATALOG_DESCRIPTOR => self.handle_catalog_descriptor(page),
            CMD_ASSERT_CATALOG_COMPLETE => self.handle_catalog_complete(page),
            CMD_ASSERT_ALWAYS => self.handle_assertion_event(page, AssertionKind::Always),
            CMD_ASSERT_SOMETIMES => self.handle_assertion_event(page, AssertionKind::Sometimes),
            CMD_ASSERT_REACHABLE => self.handle_assertion_event(page, AssertionKind::Reachable),
            CMD_ASSERT_UNREACHABLE => self.handle_assertion_event(page, AssertionKind::Unreachable),
            CMD_LIFECYCLE_SETUP_COMPLETE => match self.oracle.record_setup_complete() {
                Ok(()) => {
                    self.setup_complete = true;
                    (0, STATUS_OK)
                }
                Err(_) => (0, STATUS_ERROR),
            },
            CMD_LIFECYCLE_SEND_EVENT => {
                let (name, json_details) = self.decode_event(page);
                let details = serde_json::from_slice::<serde_json::Value>(&json_details)
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                match self.oracle.record_event(&name, details) {
                    Ok(()) => (0, STATUS_OK),
                    Err(_) => (0, STATUS_ERROR),
                }
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
            _cmd => {
                // Unknown command — return error
                (0, STATUS_ERROR)
            }
        }
    }

    fn handle_catalog_begin(&mut self, page: &HypercallPage) -> (u64, u8) {
        if self.catalog_builder.is_some()
            || self.oracle.catalog_status()
                != chaoscontrol_protocol::admission::CatalogValidationStatus::Pending
        {
            return self.catalog_failure(CatalogConflict::AlreadyBegun);
        }
        let Ok(payload) = self.page_payload(page) else {
            return self.catalog_failure(CatalogConflict::Descriptor(
                chaoscontrol_protocol::identity::AssertionError::MalformedCanonical,
            ));
        };
        if let Err(error) = decode_catalog_begin(payload) {
            return self.catalog_failure(CatalogConflict::Descriptor(error));
        }
        let expected = page.id as usize;
        match CatalogBuilder::begin(expected) {
            Ok(builder) => {
                self.catalog_builder = Some(builder);
                (0, STATUS_OK)
            }
            Err(conflict) => self.catalog_failure(conflict),
        }
    }

    fn handle_catalog_descriptor(&mut self, page: &HypercallPage) -> (u64, u8) {
        let Ok(payload) = self.page_payload(page) else {
            return self.catalog_failure(CatalogConflict::Descriptor(
                chaoscontrol_protocol::identity::AssertionError::MalformedCanonical,
            ));
        };
        let frame = match decode_descriptor_frame(payload) {
            Ok(frame) => frame,
            Err(error) => return self.catalog_failure(CatalogConflict::Descriptor(error)),
        };
        if frame.descriptor.compatibility_id != Some(page.id) {
            return self.catalog_failure(CatalogConflict::CompatibilityAliasConflict);
        }
        let result = match self.catalog_builder.as_mut() {
            Some(builder) => builder.insert_with_fingerprint(frame.descriptor, frame.fingerprint),
            None => return self.catalog_failure(CatalogConflict::CatalogIncomplete),
        };
        match result {
            Ok(_) => (0, STATUS_OK),
            Err(conflict) => self.catalog_failure(conflict),
        }
    }

    fn handle_catalog_complete(&mut self, page: &HypercallPage) -> (u64, u8) {
        let Ok(payload) = self.page_payload(page) else {
            return self.catalog_failure(CatalogConflict::Descriptor(
                chaoscontrol_protocol::identity::AssertionError::MalformedCanonical,
            ));
        };
        let token = match decode_catalog_complete(payload) {
            Ok(token) => token,
            Err(error) => return self.catalog_failure(CatalogConflict::Descriptor(error)),
        };
        let Some(builder) = self.catalog_builder.as_ref() else {
            return self.catalog_failure(CatalogConflict::CatalogIncomplete);
        };
        let completed_count = page.id as usize;
        if completed_count != builder.expected_frames()
            || completed_count != builder.received_frames()
        {
            return self.catalog_failure(CatalogConflict::UnexpectedDescriptorCount);
        }
        let builder = self
            .catalog_builder
            .take()
            .expect("catalog builder was checked");
        match builder
            .complete(token)
            .and_then(|catalog| self.oracle.activate_catalog(catalog))
        {
            Ok(()) => (0, STATUS_OK),
            Err(conflict) => self.catalog_failure(conflict),
        }
    }

    fn handle_assertion_event(&mut self, page: &HypercallPage, kind: AssertionKind) -> (u64, u8) {
        if self.oracle.catalog_status()
            != chaoscontrol_protocol::admission::CatalogValidationStatus::Accepted
        {
            self.oracle
                .mark_identity_conflict(CatalogConflict::CatalogIncomplete);
            return (0, STATUS_ASSERTION_EVENT_REJECTED);
        }
        let Ok(payload) = self.page_payload(page) else {
            self.oracle
                .mark_identity_conflict(CatalogConflict::Descriptor(
                    chaoscontrol_protocol::identity::AssertionError::MalformedCanonical,
                ));
            return (0, STATUS_ASSERTION_EVENT_REJECTED);
        };
        let frame = match decode_event_frame(payload, kind) {
            Ok(frame) => frame,
            Err(error) => {
                self.oracle
                    .mark_identity_conflict(CatalogConflict::Descriptor(error));
                return (0, STATUS_ASSERTION_EVENT_REJECTED);
            }
        };
        let event = BoundAssertionEvent {
            catalog_token: frame.catalog_token,
            fingerprint: frame.fingerprint,
            kind: frame.kind,
        };
        let condition = page.condition();
        let details = if condition {
            None
        } else {
            Some(frame.details.as_slice())
        };
        if self
            .oracle
            .record_bound_event_with_compatibility(&event, page.id, condition, details)
            .is_err()
        {
            return (0, STATUS_ASSERTION_EVENT_REJECTED);
        }
        match kind {
            AssertionKind::Always if !condition => (0, STATUS_ASSERTION_FAILED),
            AssertionKind::Unreachable => (0, STATUS_UNREACHABLE_REACHED),
            _ => (0, STATUS_OK),
        }
    }

    fn catalog_failure(&mut self, conflict: CatalogConflict) -> (u64, u8) {
        let status = if conflict == CatalogConflict::CardinalityOverflow {
            STATUS_ASSERTION_LIMIT_EXCEEDED
        } else {
            STATUS_ASSERTION_IDENTITY_CONFLICT
        };
        self.catalog_builder = None;
        self.oracle.mark_identity_conflict(conflict);
        (0, status)
    }

    fn page_payload<'a>(&self, page: &'a HypercallPage) -> Result<&'a [u8], EngineError> {
        let payload_length = page.payload_len as usize;
        if payload_length > PAYLOAD_MAX {
            return Err(EngineError::PayloadDecode);
        }
        Ok(&page.payload[..payload_length])
    }

    /// Poll for faults that should be injected at the given virtual time.
    ///
    /// Returns all due faults.  Only injects faults after `setup_complete`
    /// has been received (faults during setup would be confusing).
    pub fn poll_faults(&mut self, current_time_ns: u64) -> Vec<Fault> {
        if !self.setup_complete {
            return Vec::new();
        }

        let mut faults = Vec::new();

        // Drain scheduled faults
        for sf in self.schedule.drain_due(current_time_ns) {
            faults.push(sf.fault);
        }

        // Maybe generate a random fault
        if self.config.random_faults && current_time_ns >= self.next_random_fault_time_ns {
            if let Some(fault) = self.generate_random_fault() {
                faults.push(fault);
            }
            self.next_random_fault_time_ns = current_time_ns + self.config.random_fault_interval_ns;
        }

        self.faults_injected += faults.len() as u64;
        faults
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

    /// Total faults injected across all runs.
    pub fn faults_injected(&self) -> u64 {
        self.faults_injected
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
        if self.oracle.record_setup_complete().is_ok() {
            self.setup_complete = true;
        }
    }

    /// Reset setup_complete to false (used during VM restart).
    pub fn reset_setup_complete(&mut self) {
        self.setup_complete = false;
        self.oracle.reset_setup_complete();
    }

    /// Replace the fault schedule (for exploration branch mutations).
    pub fn set_schedule(&mut self, schedule: FaultSchedule) {
        self.schedule = schedule;
    }

    /// Snapshot the engine state.
    pub fn snapshot(&self) -> EngineSnapshot {
        EngineSnapshot {
            rng_seed: self.rng.get_seed(),
            rng_stream: self.rng.get_stream(),
            rng_word_pos: self.rng.get_word_pos(),
            oracle: self.oracle.snapshot(),
            schedule: self.schedule.snapshot(),
            faults_injected: self.faults_injected,
            setup_complete: self.setup_complete,
            next_random_fault_time_ns: self.next_random_fault_time_ns,
            choice_count: self.choice_count,
        }
    }

    /// Restore engine state from a snapshot.
    pub fn restore(
        &mut self,
        snapshot: &EngineSnapshot,
    ) -> Result<(), crate::oracle_validation::OracleValidationError> {
        validate_engine_snapshot(snapshot)?;
        self.rng = ChaCha20Rng::from_seed(snapshot.rng_seed);
        self.rng.set_stream(snapshot.rng_stream);
        self.rng.set_word_pos(snapshot.rng_word_pos);
        self.oracle.restore(&snapshot.oracle)?;
        self.schedule.restore(&snapshot.schedule);
        self.faults_injected = snapshot.faults_injected;
        self.setup_complete = snapshot.setup_complete;
        self.next_random_fault_time_ns = snapshot.next_random_fault_time_ns;
        self.choice_count = snapshot.choice_count;
        // Clear history — new run starts recording fresh.
        // random_overrides are NOT cleared — they're set externally
        // before each branch by the explorer.
        self.choice_history.clear();
        self.catalog_builder = None;
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

    // ── Internal ────────────────────────────────────────────────

    fn rng_from_seed(seed: u64) -> ChaCha20Rng {
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&seed.to_le_bytes());
        ChaCha20Rng::from_seed(key)
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
    fn setup_complete_without_active_run_is_rejected() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        let page = make_page(CMD_LIFECYCLE_SETUP_COMPLETE, 0, 0);

        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_ERROR);
        assert!(!engine.setup_complete);
        assert!(!engine.oracle.is_setup_complete());
    }

    #[test]
    fn strict_event_before_catalog_is_rejected() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page_with_payload(CMD_ASSERT_ALWAYS, 0x01, 1, "test", b"{}");
        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_ASSERTION_EVENT_REJECTED);
        assert!(!engine.has_assertion_failure());
        assert!(!engine.oracle().report().collision_safe_evidence);
    }

    #[test]
    fn strict_false_event_before_catalog_is_rejected() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page_with_payload(CMD_ASSERT_ALWAYS, 0x00, 1, "bad", b"{}");
        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_ASSERTION_EVENT_REJECTED);
        assert!(!engine.has_assertion_failure());
    }

    #[test]
    fn strict_sometimes_event_before_catalog_is_rejected() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page_with_payload(CMD_ASSERT_SOMETIMES, 0x00, 1, "rare", b"{}");
        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_ASSERTION_EVENT_REJECTED);
    }

    #[test]
    fn strict_unreachable_event_before_catalog_is_rejected() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page_with_payload(CMD_ASSERT_UNREACHABLE, 0x00, 1, "impossible", b"{}");
        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_ASSERTION_EVENT_REJECTED);
        assert!(!engine.has_assertion_failure());
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
        let faults = engine.poll_faults(1_000_000);
        assert!(faults.is_empty());

        // After setup_complete: faults fire
        let page = make_page(CMD_LIFECYCLE_SETUP_COMPLETE, 0, 0);
        engine.handle_hypercall(&page);
        let faults = engine.poll_faults(1_000_000);
        assert_eq!(faults.len(), 1);
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

        let faults = engine.poll_faults(500);
        assert!(faults.is_empty());

        let faults = engine.poll_faults(1500);
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0], Fault::NetworkHeal);

        let faults = engine.poll_faults(3000);
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0], Fault::ProcessKill { target: 0 });
    }

    #[test]
    fn snapshot_restore_engine() {
        let mut engine = FaultEngine::new(EngineConfig::default());

        // Record some state
        let page = make_page(CMD_RANDOM_GET, 0, 0);
        let (v1, _) = engine.handle_hypercall(&page);

        let snap = engine.snapshot();

        // Advance further
        let (v2, _) = engine.handle_hypercall(&page);
        assert_ne!(v1, v2);

        // Restore and verify same next value
        engine.restore(&snap).expect("restore engine");
        engine.begin_run();
        let (v3, _) = engine.handle_hypercall(&page);
        assert_eq!(v2, v3);
    }

    #[test]
    fn removed_identity_commands_return_error() {
        const REMOVED_LEGACY_COMMAND: u8 = 0x05;
        const REMOVED_GUIDANCE_COMMAND: u8 = 0x07;
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        for command in [REMOVED_LEGACY_COMMAND, REMOVED_GUIDANCE_COMMAND] {
            let page = make_page(command, 0, 0);
            let (_, status) = engine.handle_hypercall(&page);
            assert_eq!(status, STATUS_ERROR);
        }
    }

    #[test]
    fn snapshot_setup_state_must_match_oracle_run() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        let before = engine.oracle.report();
        let mut snapshot = engine.snapshot();
        snapshot.setup_complete = true;

        assert_eq!(
            validate_engine_snapshot(&snapshot),
            Err(crate::oracle_validation::OracleValidationError::Status)
        );
        assert!(engine.restore(&snapshot).is_err());
        assert_eq!(engine.oracle.report(), before);
    }

    #[test]
    fn incomplete_catalog_cannot_span_runs() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();
        engine.catalog_builder = Some(CatalogBuilder::begin(1).expect("catalog builder"));
        engine.end_run();

        assert!(engine.catalog_builder.is_none());
        assert_eq!(
            engine.oracle.catalog_status(),
            chaoscontrol_protocol::admission::CatalogValidationStatus::FatalConflict
        );
        engine.catalog_builder = Some(CatalogBuilder::begin(1).expect("stale builder"));
        engine.begin_run();
        assert!(engine.catalog_builder.is_none());
    }

    #[test]
    fn restore_clears_ambient_catalog_builder() {
        let snapshot = FaultEngine::new(EngineConfig::default()).snapshot();
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.catalog_builder = Some(CatalogBuilder::begin(1).expect("stale builder"));

        engine
            .restore(&snapshot)
            .expect("restore pristine snapshot");
        assert!(engine.catalog_builder.is_none());
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

        let page = make_page(CMD_RANDOM_CHOICE, 0, 5);
        engine.handle_hypercall(&page);
        engine.handle_hypercall(&page);
        assert_eq!(engine.choice_count(), 2);

        let snap = engine.snapshot();

        // Advance further
        engine.handle_hypercall(&page);
        assert_eq!(engine.choice_count(), 3);

        // Restore → back to 2
        engine.restore(&snap).expect("restore engine");
        assert_eq!(engine.choice_count(), 2);

        // History cleared on restore
        assert!(engine.drain_choice_history().is_empty());
    }

    #[test]
    fn overrides_persist_across_restore() {
        let mut engine = FaultEngine::new(EngineConfig::default());

        // Set override
        let mut overrides = BTreeMap::new();
        overrides.insert(0, 42);
        engine.set_random_overrides(overrides);

        // Take snapshot and restore
        let snap = engine.snapshot();
        engine.restore(&snap).expect("restore engine");

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
