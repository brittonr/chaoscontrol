//! Fault injection engine — the central orchestrator.
//!
//! The [`FaultEngine`] combines the fault schedule, property oracle, and
//! deterministic RNG into a single coordinator.  The VMM calls into the
//! engine on every hypercall and on every exit to check if faults are due.

use crate::faults::Fault;
use crate::oracle::PropertyOracle;
use crate::schedule::FaultSchedule;
use chaoscontrol_protocol::*;
use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use thiserror::Error;

/// Errors from the fault engine.
#[derive(Error, Debug)]
pub enum EngineError {
    #[error("No active run — call begin_run() first")]
    NoActiveRun,

    #[error("Payload decode failed")]
    PayloadDecode,

    #[error("Unknown command: {0:#x}")]
    UnknownCommand(u8),
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
#[derive(Debug, Clone)]
pub struct EngineSnapshot {
    rng_seed: [u8; 32],
    rng_stream: u64,
    rng_word_pos: u128,
    oracle: crate::oracle::OracleSnapshot,
    schedule: crate::schedule::FaultScheduleSnapshot,
    faults_injected: u64,
    setup_complete: bool,
    next_random_fault_time_ns: u64,
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
        }
    }

    /// Begin a new test run.
    pub fn begin_run(&mut self) {
        self.oracle.begin_run();
        self.setup_complete = false;
        self.schedule.reset();
        self.next_random_fault_time_ns = self.config.random_fault_interval_ns;
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
            CMD_ASSERT_ALWAYS => {
                let cond = page.condition();
                let message = self.decode_message(page);
                self.oracle.record_always(page.id, cond, &message);
                if cond {
                    (0, STATUS_OK)
                } else {
                    (0, STATUS_ASSERTION_FAILED)
                }
            }
            CMD_ASSERT_SOMETIMES => {
                let cond = page.condition();
                let message = self.decode_message(page);
                self.oracle.record_sometimes(page.id, cond, &message);
                (0, STATUS_OK)
            }
            CMD_ASSERT_REACHABLE => {
                let message = self.decode_message(page);
                self.oracle.record_reachable(page.id, &message);
                (0, STATUS_OK)
            }
            CMD_ASSERT_UNREACHABLE => {
                let message = self.decode_message(page);
                self.oracle.record_unreachable(page.id, &message);
                (0, STATUS_UNREACHABLE_REACHED)
            }
            CMD_LIFECYCLE_SETUP_COMPLETE => {
                self.setup_complete = true;
                self.oracle.record_setup_complete();
                (0, STATUS_OK)
            }
            CMD_LIFECYCLE_SEND_EVENT => {
                let (name, details) = self.decode_event(page);
                self.oracle.record_event(&name, details);
                (0, STATUS_OK)
            }
            CMD_RANDOM_GET => {
                let value = self.rng.next_u64();
                (value, STATUS_OK)
            }
            CMD_RANDOM_CHOICE => {
                let n = page.id; // n is passed via id field
                let value = if n <= 1 {
                    0
                } else {
                    self.rng.next_u64() % n as u64
                };
                (value, STATUS_OK)
            }
            _cmd => {
                // Unknown command — return error
                (0, STATUS_ERROR)
            }
        }
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
            self.next_random_fault_time_ns =
                current_time_ns + self.config.random_fault_interval_ns;
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
        }
    }

    /// Restore engine state from a snapshot.
    pub fn restore(&mut self, snapshot: &EngineSnapshot) {
        self.rng = ChaCha20Rng::from_seed(snapshot.rng_seed);
        self.rng.set_stream(snapshot.rng_stream);
        self.rng.set_word_pos(snapshot.rng_word_pos);
        self.oracle.restore(&snapshot.oracle);
        self.schedule.restore(&snapshot.schedule);
        self.faults_injected = snapshot.faults_injected;
        self.setup_complete = snapshot.setup_complete;
        self.next_random_fault_time_ns = snapshot.next_random_fault_time_ns;
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
        decode_payload(buf)
            .map(|p| p.message)
            .unwrap_or_default()
    }

    fn decode_event(&self, page: &HypercallPage) -> (String, Vec<(String, String)>) {
        let payload_len = page.payload_len as usize;
        if payload_len == 0 {
            return (String::new(), Vec::new());
        }
        let buf = &page.payload[..payload_len.min(PAYLOAD_MAX)];
        decode_payload(buf)
            .map(|p| (p.message, p.details))
            .unwrap_or_default()
    }

    fn generate_random_fault(&mut self) -> Option<Fault> {
        if self.config.num_vms == 0 {
            return None;
        }

        let target = (self.rng.next_u64() as usize) % self.config.num_vms;
        let fault_type = self.rng.next_u64() % 8;

        Some(match fault_type {
            0 => Fault::ProcessKill { target },
            1 => Fault::ProcessPause {
                target,
                duration_ns: (self.rng.next_u64() % 5_000_000_000) + 100_000_000,
            },
            2 => {
                // Network partition: target vs everyone else
                let side_a = vec![target];
                let side_b = (0..self.config.num_vms)
                    .filter(|&i| i != target)
                    .collect();
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
                offset_ns: (self.rng.next_u64() % 10_000_000_000) as i64
                    - 5_000_000_000,
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
        details: &[(&str, &str)],
    ) -> HypercallPage {
        let mut page = make_page(command, flags, id);
        if let Some(len) = encode_payload(&mut page.payload, message, details) {
            page.payload_len = len as u16;
        }
        page
    }

    #[test]
    fn handle_assert_always_true() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page_with_payload(CMD_ASSERT_ALWAYS, 0x01, 1, "test", &[]);
        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_OK);
        assert!(!engine.has_assertion_failure());
    }

    #[test]
    fn handle_assert_always_false() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page_with_payload(CMD_ASSERT_ALWAYS, 0x00, 1, "bad", &[]);
        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_ASSERTION_FAILED);
        assert!(engine.has_assertion_failure());
    }

    #[test]
    fn handle_assert_sometimes() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page_with_payload(CMD_ASSERT_SOMETIMES, 0x00, 1, "rare", &[]);
        let (_, status) = engine.handle_hypercall(&page);
        assert_eq!(status, STATUS_OK); // sometimes(false) is not an immediate failure
    }

    #[test]
    fn handle_assert_unreachable() {
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.begin_run();

        let page = make_page_with_payload(CMD_ASSERT_UNREACHABLE, 0x00, 1, "impossible", &[]);
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
        let mut e1 = FaultEngine::new(EngineConfig { seed: 123, ..Default::default() });
        let mut e2 = FaultEngine::new(EngineConfig { seed: 123, ..Default::default() });
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
        let mut e1 = FaultEngine::new(EngineConfig { seed: 1, ..Default::default() });
        let mut e2 = FaultEngine::new(EngineConfig { seed: 2, ..Default::default() });
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
        engine.begin_run();

        // Record some state
        let page = make_page(CMD_RANDOM_GET, 0, 0);
        let (v1, _) = engine.handle_hypercall(&page);

        let snap = engine.snapshot();

        // Advance further
        let (v2, _) = engine.handle_hypercall(&page);
        assert_ne!(v1, v2);

        // Restore and verify same next value
        engine.restore(&snap);
        engine.begin_run();
        let (v3, _) = engine.handle_hypercall(&page);
        assert_eq!(v2, v3);
    }

    #[test]
    fn cross_run_oracle_report() {
        let mut engine = FaultEngine::new(EngineConfig::default());

        // Run 0: always=true, sometimes=false
        engine.begin_run();
        let p1 = make_page_with_payload(CMD_ASSERT_ALWAYS, 0x01, 1, "stable", &[]);
        let p2 = make_page_with_payload(CMD_ASSERT_SOMETIMES, 0x00, 2, "rare", &[]);
        engine.handle_hypercall(&p1);
        engine.handle_hypercall(&p2);
        engine.end_run();

        // Run 1: always=true, sometimes=true
        engine.begin_run();
        engine.handle_hypercall(&p1);
        let p2_true = make_page_with_payload(CMD_ASSERT_SOMETIMES, 0x01, 2, "rare", &[]);
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
}
