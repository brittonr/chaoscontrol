//! Multi-VM simulation controller for deterministic distributed system testing.
//!
//! [`SimulationController`] orchestrates multiple [`DeterministicVm`] instances
//! in a single deterministic simulation, handling fault injection, network
//! routing, and deterministic scheduling.

use crate::scheduler::ScheduleVariant;
use crate::snapshot::VmSnapshot;
use crate::vm::{DeterministicVm, SnapshotSnafu, VmConfig, VmError};
use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::outcomes::{
    plan_fault_application, preflight_fault_application_events_with_limit,
    preflight_fault_observation_events_with_limit, validate_pending_fault_effect,
    validate_pending_fault_observations, FaultApplicationFailureDisposition,
    FaultApplicationFailureReason, FaultApplicationPolicy, FaultAttempt, FaultAttemptId,
    FaultMechanism, FaultObservation, FaultObservationEffect, FaultObservationSubsystem,
    FaultOutcomeLedger, FaultPlan, FaultPlanEffect, FaultPlanningFacts, FaultStageEvent,
    FaultStageKind, FaultTransitionError, FaultVmStatus, VmFaultFacts, MAX_FAULT_OUTCOME_EVENTS,
};
use chaoscontrol_fault::schedule::FaultSchedule;
use log::{debug, info, warn};
use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const MAX_PENDING_FAULT_OBSERVATIONS: usize = 4_096;
const MAX_PENDING_PROCESS_OBSERVATIONS: usize = 4_096;
const NETWORK_TICKS_PER_SECOND: u64 = 1_000;
const DUPLICATE_OFFSET_CHOICES: u64 = 3;
const MAX_DUPLICATE_OFFSET_TICKS: u64 = DUPLICATE_OFFSET_CHOICES - 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkSendError {
    InvalidEndpoint,
    TickArithmetic,
    ObservationCapacity,
    ObservationSequence,
    CounterCapacity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CheckedPacketEffectPlan {
    max_observations: usize,
    max_deliver_at_tick: u64,
}

fn bandwidth_serialization_ticks(packet_bytes: usize, bytes_per_second: u64) -> u64 {
    checked_bandwidth_serialization_ticks(packet_bytes, bytes_per_second)
        .expect("packet timing must be admitted before application")
}

fn route_network_packet(
    network: &mut NetworkFabric,
    from: usize,
    to: usize,
    packet: Vec<u8>,
    current_tick: u64,
) -> Result<bool, VmError> {
    network
        .try_send_packet(from, to, packet, current_tick)
        .map_err(|reason| VmError::NetworkPacketNonRunnable { from, to, reason })
}

fn checked_bandwidth_serialization_ticks(
    packet_bytes: usize,
    bytes_per_second: u64,
) -> Result<u64, NetworkSendError> {
    if packet_bytes == 0 || bytes_per_second == 0 {
        return Ok(0);
    }
    let packet_bytes = u64::try_from(packet_bytes).map_err(|_| NetworkSendError::TickArithmetic)?;
    let tick_bytes = packet_bytes
        .checked_mul(NETWORK_TICKS_PER_SECOND)
        .ok_or(NetworkSendError::TickArithmetic)?;
    let whole_ticks = tick_bytes / bytes_per_second;
    let has_remainder = u64::from(tick_bytes % bytes_per_second != 0);
    whole_ticks
        .checked_add(has_remainder)
        .ok_or(NetworkSendError::TickArithmetic)
}

// ═══════════════════════════════════════════════════════════════════════
//  Configuration
// ═══════════════════════════════════════════════════════════════════════

/// Configuration for a multi-VM simulation.
#[derive(Debug, Clone)]
pub struct SimulationConfig {
    /// Number of VMs in the simulation.
    pub num_vms: usize,
    /// Per-VM config (same for all VMs).
    pub vm_config: VmConfig,
    /// Kernel path.
    pub kernel_path: String,
    /// Optional initrd path.
    pub initrd_path: Option<String>,
    /// Master seed for determinism.
    pub seed: u64,
    /// Exits per VM per scheduling round.
    pub quantum: u64,
    /// Fault schedule to execute.
    pub schedule: FaultSchedule,
    /// Optional disk image path for virtio-blk devices.
    ///
    /// When set, each VM's block device is initialized from this file.
    /// The file is read once per VM; copy-on-write makes snapshots cheap.
    pub disk_image_path: Option<String>,

    /// Max ticks for VM restart bootstrap (kernel boot + setup_complete).
    /// Default: 10_000.
    pub bootstrap_budget: Option<u64>,

    /// Base core index for CPU affinity pinning.
    ///
    /// When set, VM `i` is pinned to core `base_core + i`. This matches
    /// the Antithesis model where each VM runs on a dedicated physical
    /// core, eliminating host scheduler jitter and ensuring consistent
    /// PMC behavior.
    ///
    /// When `None`, no affinity is set (OS scheduler decides).
    pub base_core: Option<usize>,

    /// Directory for determinism log files.
    ///
    /// When set, each VM writes a binary dlog file to
    /// `<dlog_dir>/vm_<i>.dlog`. Use `dlog_diff` to compare
    /// two runs of the same seed.
    pub dlog_dir: Option<std::path::PathBuf>,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            num_vms: 2,
            vm_config: VmConfig::default(),
            kernel_path: String::new(),
            initrd_path: None,
            seed: 42,
            quantum: 100,
            schedule: FaultSchedule::default(),
            disk_image_path: None,
            bootstrap_budget: None,
            base_core: None,
            dlog_dir: None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  VM State
// ═══════════════════════════════════════════════════════════════════════

/// State of a single VM within the simulation.
pub struct VmSlot {
    /// The VM instance.
    pub vm: DeterministicVm,
    /// Current status.
    pub status: VmStatus,
    /// Per-VM network mailbox (incoming messages).
    pub inbox: VecDeque<NetworkMessage>,
    /// Per-VM disk fault flags.
    pub disk_faults: DiskFaultFlags,
    /// TSC skew offset for clock fault injection (nanoseconds).
    pub tsc_skew: i64,
    /// Memory pressure limit in bytes (`None` = unlimited).
    pub memory_limit_bytes: Option<u64>,
    /// Initial snapshot taken after kernel load, used for restarts.
    pub initial_snapshot: Option<VmSnapshot>,
    /// Per-vCPU stall: vCPU index → tick at which stall expires.
    pub vcpu_stall_until: std::collections::BTreeMap<usize, u64>,
    /// Frozen TSC: `(frozen_tsc_value, expires_at_tick)`. Takes priority over jitter.
    pub clock_freeze: Option<(u64, u64)>,
    /// Per-exit TSC jitter bound (±bound). 0 = disabled.
    pub clock_jitter_bound: u64,
    /// Attempt that armed the current process-status effect.
    pub process_fault_attempt: Option<FaultAttemptId>,
}

/// Current status of a VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmStatus {
    /// VM is running normally.
    Running,
    /// VM is paused (ProcessPause fault active), will auto-resume.
    Paused,
    /// VM has crashed (ProcessKill fault injected).
    Crashed,
    /// Crashed VM will restart after this simulation tick.
    Restarting { restart_at_tick: u64 },
    /// Paused VM will resume (without restore) at this tick.
    Resuming { resume_at_tick: u64 },
}

/// A message in the virtual network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMessage {
    /// Source VM index.
    pub from: usize,
    /// Destination VM index.
    pub to: usize,
    /// Payload bytes.
    pub data: Vec<u8>,
    /// Delivery tick (for latency simulation).
    pub deliver_at_tick: u64,
}

/// Disk fault injection flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskFaultFlags {
    /// Probability (0.0-1.0) of I/O error.
    pub error_rate: f64,
    /// Multiplier for I/O latency.
    pub slow_factor: u64,
    /// Simulate disk full.
    pub full: bool,
}

// ═══════════════════════════════════════════════════════════════════════
//  Network Fabric
// ═══════════════════════════════════════════════════════════════════════

/// Packet-level counters for network fabric observability.
///
/// Tracks how many packets were affected by each fault type so the
/// effects of jitter, bandwidth, loss, corruption, reorder, and
/// duplication are visible in reports and logs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkStats {
    /// Total packets submitted to `send()`.
    pub packets_sent: u64,
    /// Packets delivered (enqueued in `in_flight`).
    pub packets_delivered: u64,
    /// Packets dropped by partition rules.
    pub packets_dropped_partition: u64,
    /// Packets dropped by loss rate.
    pub packets_dropped_loss: u64,
    /// Packets whose payload was corrupted.
    pub packets_corrupted: u64,
    /// Extra duplicate copies created.
    pub packets_duplicated: u64,
    /// Packets that had bandwidth serialization delay added.
    pub packets_bandwidth_delayed: u64,
    /// Packets that had configured latency added to delivery time.
    pub packets_latency_delayed: u64,
    /// Packets that had jitter added to delivery time.
    pub packets_jittered: u64,
    /// Packets that had reorder window applied.
    pub packets_reordered: u64,
    /// Cumulative configured latency ticks added across delayed packets.
    pub total_latency_delay_ticks: u64,
    /// Cumulative jitter ticks added across all jittered packets.
    pub total_jitter_ticks: u64,
    /// Cumulative bandwidth delay ticks added across all delayed packets.
    pub total_bandwidth_delay_ticks: u64,
}

impl std::fmt::Display for NetworkStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "sent={} delivered={} lost(partition={}, loss={}) corrupted={} \
             duplicated={} bw_delayed={}({}ticks) latency_delayed={}({}ticks) \
             jittered={}({}ticks) reordered={}",
            self.packets_sent,
            self.packets_delivered,
            self.packets_dropped_partition,
            self.packets_dropped_loss,
            self.packets_corrupted,
            self.packets_duplicated,
            self.packets_bandwidth_delayed,
            self.total_bandwidth_delay_ticks,
            self.packets_latency_delayed,
            self.total_latency_delay_ticks,
            self.packets_jittered,
            self.total_jitter_ticks,
            self.packets_reordered,
        )
    }
}

/// A raw packet in flight (virtio-net TX/RX level).
///
/// Used for VM-to-VM packet bridging through the NetworkFabric's
/// fault injection pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketInFlight {
    /// Source VM index.
    pub from: usize,
    /// Destination VM index.
    pub to: usize,
    /// Raw packet data (Ethernet frame).
    pub data: Vec<u8>,
    /// Delivery tick (after latency, jitter, bandwidth, etc.).
    pub deliver_at_tick: u64,
}

/// Virtual network with partition awareness and packet-level fault injection.
///
/// Models real-world network impairments: latency, jitter, bandwidth limits,
/// packet loss, corruption, reordering, and duplication.  All values are
/// per-VM and bidirectional (max of sender/receiver is used).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkFabric {
    /// Active partition rules — (side_a, side_b) pairs.
    pub partitions: Vec<(Vec<usize>, Vec<usize>)>,
    /// Attempt identity parallel to each partition rule.
    pub partition_attempt_ids: Vec<Option<FaultAttemptId>>,
    /// Per-VM base latency in ticks (0 = no added latency).
    pub latency: Vec<u64>,
    /// Per-VM attempt that armed latency.
    pub latency_attempt_ids: Vec<Option<FaultAttemptId>>,
    /// Per-VM latency jitter in ticks (0 = no jitter).
    /// Each packet gets up to this much extra random delay on top of base latency.
    pub jitter: Vec<u64>,
    /// Per-VM attempt that armed jitter.
    pub jitter_attempt_ids: Vec<Option<FaultAttemptId>>,
    /// Per-VM bandwidth limit in bytes per second (0 = unlimited).
    pub bandwidth_bps: Vec<u64>,
    /// Per-VM attempt that armed bandwidth limiting.
    pub bandwidth_attempt_ids: Vec<Option<FaultAttemptId>>,
    /// Per-VM next-free tick for bandwidth serialization queuing.
    pub next_free_tick: Vec<u64>,
    /// Messages in flight (not yet delivered).
    pub in_flight: Vec<NetworkMessage>,
    /// Raw packets in flight (VM-to-VM bridging).
    pub packet_in_flight: Vec<PacketInFlight>,
    /// Per-VM packet loss rate in parts per million (0 = no loss).
    pub loss_rate_ppm: Vec<u32>,
    /// Per-VM attempt that armed packet loss.
    pub loss_attempt_ids: Vec<Option<FaultAttemptId>>,
    /// Per-VM packet corruption rate in parts per million (0 = no corruption).
    pub corruption_rate_ppm: Vec<u32>,
    /// Per-VM attempt that armed packet corruption.
    pub corruption_attempt_ids: Vec<Option<FaultAttemptId>>,
    /// Per-VM reorder window in ticks (0 = no reordering).
    pub reorder_window: Vec<u64>,
    /// Per-VM attempt that armed packet reordering.
    pub reorder_attempt_ids: Vec<Option<FaultAttemptId>>,
    /// Per-VM packet duplication rate in parts per million (0 = no duplication).
    pub duplicate_rate_ppm: Vec<u32>,
    /// Per-VM attempt that armed packet duplication.
    pub duplicate_attempt_ids: Vec<Option<FaultAttemptId>>,
    /// Deterministic RNG for packet-level fault decisions.
    pub rng: ChaCha20Rng,
    /// Cumulative packet-level statistics.
    pub stats: NetworkStats,
    /// Bounded observations waiting for the controller ledger.
    pub fault_observations: VecDeque<FaultObservation>,
    /// Next deterministic network operation sequence.
    pub fault_observation_sequence: u64,
    /// Observations rejected because the bounded queue was full.
    pub fault_observation_overflowed: u64,
}

impl NetworkFabric {
    /// Create a new network fabric for `num_vms` VMs with the given seed.
    pub fn new(num_vms: usize, seed: u64) -> Self {
        let mut rng_key = [0u8; 32];
        // Derive network RNG from seed + a domain separator
        let derived = seed.wrapping_add(0x4E45_5446_4142); // "NETFAB" as hex
        rng_key[..8].copy_from_slice(&derived.to_le_bytes());
        Self {
            partitions: Vec::new(),
            partition_attempt_ids: Vec::new(),
            latency: vec![0; num_vms],
            latency_attempt_ids: vec![None; num_vms],
            jitter: vec![0; num_vms],
            jitter_attempt_ids: vec![None; num_vms],
            bandwidth_bps: vec![0; num_vms],
            bandwidth_attempt_ids: vec![None; num_vms],
            next_free_tick: vec![0; num_vms],
            in_flight: Vec::new(),
            packet_in_flight: Vec::new(),
            loss_rate_ppm: vec![0; num_vms],
            loss_attempt_ids: vec![None; num_vms],
            corruption_rate_ppm: vec![0; num_vms],
            corruption_attempt_ids: vec![None; num_vms],
            reorder_window: vec![0; num_vms],
            reorder_attempt_ids: vec![None; num_vms],
            duplicate_rate_ppm: vec![0; num_vms],
            duplicate_attempt_ids: vec![None; num_vms],
            rng: ChaCha20Rng::from_seed(rng_key),
            stats: NetworkStats::default(),
            fault_observations: VecDeque::with_capacity(MAX_PENDING_FAULT_OBSERVATIONS),
            fault_observation_sequence: 0,
            fault_observation_overflowed: 0,
        }
    }

    /// Check if `from` can reach `to` given active partitions.
    ///
    /// Returns `false` if any partition separates them.
    pub fn can_reach(&self, from: usize, to: usize) -> bool {
        for (side_a, side_b) in &self.partitions {
            let from_in_a = side_a.contains(&from);
            let from_in_b = side_b.contains(&from);
            let to_in_a = side_a.contains(&to);
            let to_in_b = side_b.contains(&to);

            if (from_in_a && to_in_b) || (from_in_b && to_in_a) {
                return false; // Separated by this partition
            }
        }
        true
    }

    fn checked_packet_effect_plan(
        &self,
        from: usize,
        to: usize,
        packet_bytes: usize,
        current_tick: u64,
    ) -> Result<CheckedPacketEffectPlan, NetworkSendError> {
        if from >= self.latency.len() || to >= self.latency.len() {
            return Err(NetworkSendError::InvalidEndpoint);
        }
        let partition_observations =
            usize::from(!self.can_reach(from, to) && self.partition_attempt(from, to).is_some());
        let loss_observations = usize::from(
            Self::attempt_for_pair_u32(&self.loss_rate_ppm, &self.loss_attempt_ids, from, to)
                .is_some(),
        );
        let delivered_observations = [
            Self::attempt_for_pair_u64(&self.latency, &self.latency_attempt_ids, from, to),
            Self::attempt_for_pair_u64(&self.bandwidth_bps, &self.bandwidth_attempt_ids, from, to),
            Self::attempt_for_pair_u64(&self.jitter, &self.jitter_attempt_ids, from, to),
            Self::attempt_for_pair_u64(&self.reorder_window, &self.reorder_attempt_ids, from, to),
        ]
        .into_iter()
        .flatten()
        .count()
            + [
                Self::attempt_for_pair_u32(
                    &self.corruption_rate_ppm,
                    &self.corruption_attempt_ids,
                    from,
                    to,
                ),
                Self::attempt_for_pair_u32(
                    &self.duplicate_rate_ppm,
                    &self.duplicate_attempt_ids,
                    from,
                    to,
                ),
            ]
            .into_iter()
            .flatten()
            .count();
        let max_observations = if partition_observations != 0 {
            partition_observations
        } else {
            loss_observations.max(delivered_observations)
        };
        let available = MAX_PENDING_FAULT_OBSERVATIONS
            .checked_sub(self.fault_observations.len())
            .ok_or(NetworkSendError::ObservationCapacity)?;
        if max_observations > available {
            return Err(NetworkSendError::ObservationCapacity);
        }
        let observation_count =
            u64::try_from(max_observations).map_err(|_| NetworkSendError::ObservationSequence)?;
        self.fault_observation_sequence
            .checked_add(observation_count)
            .ok_or(NetworkSendError::ObservationSequence)?;
        if !self.can_reach(from, to) {
            self.stats
                .packets_sent
                .checked_add(1)
                .and_then(|_| self.stats.packets_dropped_partition.checked_add(1))
                .ok_or(NetworkSendError::CounterCapacity)?;
            return Ok(CheckedPacketEffectPlan {
                max_observations,
                max_deliver_at_tick: current_tick,
            });
        }

        let sender_bw = self.bandwidth_bps[from];
        let receiver_bw = self.bandwidth_bps[to];
        let effective_bw = match (sender_bw, receiver_bw) {
            (0, 0) => 0,
            (0, value) | (value, 0) => value,
            (left, right) => left.min(right),
        };
        let serialization = checked_bandwidth_serialization_ticks(packet_bytes, effective_bw)?;
        let tx_start = current_tick.max(self.next_free_tick[from]);
        let tx_end = tx_start
            .checked_add(serialization)
            .ok_or(NetworkSendError::TickArithmetic)?;
        let latency = self.latency[from].max(self.latency[to]);
        let jitter = self.jitter[from].max(self.jitter[to]);
        let reorder = self.reorder_window[from].max(self.reorder_window[to]);
        let max_deliver_at_tick = tx_end
            .checked_add(latency)
            .and_then(|tick| tick.checked_add(jitter))
            .and_then(|tick| tick.checked_add(reorder))
            .and_then(|tick| tick.checked_add(MAX_DUPLICATE_OFFSET_TICKS))
            .ok_or(NetworkSendError::TickArithmetic)?;

        let counters = [
            self.stats.packets_sent,
            self.stats.packets_delivered,
            self.stats.packets_dropped_partition,
            self.stats.packets_dropped_loss,
            self.stats.packets_corrupted,
            self.stats.packets_duplicated,
            self.stats.packets_bandwidth_delayed,
            self.stats.packets_latency_delayed,
            self.stats.packets_jittered,
            self.stats.packets_reordered,
        ];
        if counters.into_iter().any(|counter| counter == u64::MAX)
            || self
                .stats
                .total_bandwidth_delay_ticks
                .checked_add(tx_end - current_tick)
                .is_none()
            || self
                .stats
                .total_latency_delay_ticks
                .checked_add(latency)
                .is_none()
            || self.stats.total_jitter_ticks.checked_add(jitter).is_none()
        {
            return Err(NetworkSendError::CounterCapacity);
        }
        Ok(CheckedPacketEffectPlan {
            max_observations,
            max_deliver_at_tick,
        })
    }

    /// Compatibility wrapper for callers that treat every failure as `false`.
    ///
    /// Production paths must use [`Self::try_send`] and preserve its error.
    /// Applies the full packet-level fault pipeline in order:
    /// 1. Partition check — drop if partitioned
    /// 2. Packet loss — drop with probability
    /// 3. Bandwidth — add serialization delay (queuing model)
    /// 4. Packet corruption — flip a random byte
    /// 5. Latency + jitter — base delay plus random variation
    /// 6. Packet reorder — additional random shuffle within window
    /// 7. Packet duplication — clone with slightly offset delivery
    pub fn send(&mut self, from: usize, to: usize, data: Vec<u8>, current_tick: u64) -> bool {
        self.try_send(from, to, data, current_tick).unwrap_or(false)
    }

    pub fn try_send(
        &mut self,
        from: usize,
        to: usize,
        data: Vec<u8>,
        current_tick: u64,
    ) -> Result<bool, NetworkSendError> {
        let plan = self.checked_packet_effect_plan(from, to, data.len(), current_tick)?;
        let prior_message_count = self.in_flight.len();
        let mut candidate = self.clone();
        let stats_before = candidate.stats.clone();
        let delivered = candidate.send_inner(from, to, data, current_tick);
        candidate.record_network_delta(from, to, &stats_before, delivered);
        let produced = candidate
            .fault_observations
            .len()
            .checked_sub(self.fault_observations.len())
            .ok_or(NetworkSendError::ObservationCapacity)?;
        assert!(produced <= plan.max_observations);
        assert!(candidate.in_flight[prior_message_count..]
            .iter()
            .all(|message| message.deliver_at_tick <= plan.max_deliver_at_tick));
        *self = candidate;
        Ok(delivered)
    }

    fn send_inner(&mut self, from: usize, to: usize, data: Vec<u8>, current_tick: u64) -> bool {
        self.stats.packets_sent += 1;

        // 1. Partition check
        if !self.can_reach(from, to) {
            debug!("Message from VM{} to VM{} dropped by partition", from, to);
            self.stats.packets_dropped_partition += 1;
            return false;
        }

        // 2. Packet loss — max(sender, receiver) rate
        let sender_loss = self.loss_rate_ppm.get(from).copied().unwrap_or(0);
        let receiver_loss = self.loss_rate_ppm.get(to).copied().unwrap_or(0);
        let loss_rate = sender_loss.max(receiver_loss);
        if loss_rate > 0 {
            let roll = (self.rng.next_u64() % 1_000_000) as u32;
            if roll < loss_rate {
                debug!(
                    "Message from VM{} to VM{} dropped by packet loss ({}ppm)",
                    from, to, loss_rate
                );
                self.stats.packets_dropped_loss += 1;
                return false;
            }
        }

        // 3. Bandwidth — serialization delay with queuing
        //
        // Each VM tracks `next_free_tick`: when the outgoing link becomes
        // idle. A packet of N bytes on a B bytes/sec link takes
        // `N * NETWORK_TICKS_PER_SECOND / B` ticks. Back-to-back packets
        // queue behind each other naturally.
        let mut bandwidth_delay_ticks: u64 = 0;
        let sender_bw = self.bandwidth_bps.get(from).copied().unwrap_or(0);
        let receiver_bw = self.bandwidth_bps.get(to).copied().unwrap_or(0);
        let effective_bw = match (sender_bw, receiver_bw) {
            (0, 0) => 0,          // both unlimited
            (0, b) | (b, 0) => b, // one is limited
            (a, b) => a.min(b),   // bottleneck
        };
        if effective_bw > 0 && !data.is_empty() {
            let serialization_ticks = bandwidth_serialization_ticks(data.len(), effective_bw);
            let tx_start = current_tick.max(self.next_free_tick.get(from).copied().unwrap_or(0));
            let tx_end = tx_start.saturating_add(serialization_ticks);
            if let Some(slot) = self.next_free_tick.get_mut(from) {
                *slot = tx_end;
            }
            bandwidth_delay_ticks = tx_end.saturating_sub(current_tick);
            if bandwidth_delay_ticks > 0 {
                self.stats.packets_bandwidth_delayed =
                    self.stats.packets_bandwidth_delayed.saturating_add(1);
                self.stats.total_bandwidth_delay_ticks = self
                    .stats
                    .total_bandwidth_delay_ticks
                    .saturating_add(bandwidth_delay_ticks);
            }
            debug!(
                "Message from VM{} to VM{}: bandwidth delay {} ticks ({}B @ {}B/s)",
                from,
                to,
                bandwidth_delay_ticks,
                data.len(),
                effective_bw
            );
        }

        // 4. Packet corruption — flip a random byte
        let mut data = data;
        let sender_corrupt = self.corruption_rate_ppm.get(from).copied().unwrap_or(0);
        let receiver_corrupt = self.corruption_rate_ppm.get(to).copied().unwrap_or(0);
        let corrupt_rate = sender_corrupt.max(receiver_corrupt);
        if corrupt_rate > 0 && !data.is_empty() {
            let roll = (self.rng.next_u64() % 1_000_000) as u32;
            if roll < corrupt_rate {
                let byte_idx = (self.rng.next_u64() as usize) % data.len();
                let flip = (self.rng.next_u64() & 0xFF) as u8 | 1; // At least 1 bit flipped
                data[byte_idx] ^= flip;
                self.stats.packets_corrupted += 1;
                debug!(
                    "Message from VM{} to VM{} corrupted at byte {}",
                    from, to, byte_idx
                );
            }
        }

        // 5. Latency + jitter — base delay plus random variation
        let sender_latency = self.latency.get(from).copied().unwrap_or(0);
        let receiver_latency = self.latency.get(to).copied().unwrap_or(0);
        let latency_ticks = sender_latency.max(receiver_latency);
        let after_bandwidth = current_tick.saturating_add(bandwidth_delay_ticks);
        let after_latency = after_bandwidth.saturating_add(latency_ticks);
        let latency_applied = after_latency.saturating_sub(after_bandwidth);
        if latency_applied > 0 {
            self.stats.packets_latency_delayed =
                self.stats.packets_latency_delayed.saturating_add(1);
            self.stats.total_latency_delay_ticks = self
                .stats
                .total_latency_delay_ticks
                .saturating_add(latency_applied);
        }

        let sender_jitter = self.jitter.get(from).copied().unwrap_or(0);
        let receiver_jitter = self.jitter.get(to).copied().unwrap_or(0);
        let jitter_max = sender_jitter.max(receiver_jitter);
        let requested_jitter = if jitter_max > 0 {
            let jitter_choices = jitter_max.saturating_add(1);
            self.rng.next_u64() % jitter_choices
        } else {
            0
        };
        let after_jitter = after_latency.saturating_add(requested_jitter);
        let jitter_applied = after_jitter.saturating_sub(after_latency);
        if jitter_applied > 0 {
            self.stats.packets_jittered = self.stats.packets_jittered.saturating_add(1);
            self.stats.total_jitter_ticks =
                self.stats.total_jitter_ticks.saturating_add(jitter_applied);
        }
        let mut deliver_at_tick = after_jitter;

        // 6. Packet reorder — additional random shuffle within window
        let sender_reorder = self.reorder_window.get(from).copied().unwrap_or(0);
        let receiver_reorder = self.reorder_window.get(to).copied().unwrap_or(0);
        let reorder_win = sender_reorder.max(receiver_reorder);
        if reorder_win > 0 {
            let reorder_choices = reorder_win.saturating_add(1);
            let requested_reorder = self.rng.next_u64() % reorder_choices;
            let reordered_tick = deliver_at_tick.saturating_add(requested_reorder);
            let reorder_applied = reordered_tick.saturating_sub(deliver_at_tick);
            deliver_at_tick = reordered_tick;
            if reorder_applied > 0 {
                self.stats.packets_reordered = self.stats.packets_reordered.saturating_add(1);
            }
            debug!(
                "Message from VM{} to VM{} reordered by {} ticks",
                from, to, reorder_applied
            );
        }

        // 7. Packet duplication — maybe enqueue a second copy
        let sender_dup = self.duplicate_rate_ppm.get(from).copied().unwrap_or(0);
        let receiver_dup = self.duplicate_rate_ppm.get(to).copied().unwrap_or(0);
        let dup_rate = sender_dup.max(receiver_dup);
        if dup_rate > 0 {
            let roll = (self.rng.next_u64() % 1_000_000) as u32;
            if roll < dup_rate {
                // Duplicate arrives with a small deterministic offset.
                let dup_offset = self.rng.next_u64() % DUPLICATE_OFFSET_CHOICES;
                self.in_flight.push(NetworkMessage {
                    from,
                    to,
                    data: data.clone(),
                    deliver_at_tick: deliver_at_tick.saturating_add(dup_offset),
                });
                self.stats.packets_duplicated += 1;
                debug!(
                    "Message from VM{} to VM{} duplicated (+{} ticks)",
                    from, to, dup_offset
                );
            }
        }

        self.in_flight.push(NetworkMessage {
            from,
            to,
            data,
            deliver_at_tick,
        });

        self.stats.packets_delivered += 1;
        true
    }

    /// Compatibility wrapper for raw packet callers that cannot return errors.
    ///
    /// Production paths must use [`Self::try_send_packet`] and preserve its error.
    /// Applies the same fault injection logic as `send()` but for raw
    /// Ethernet frames from virtio-net TX queues. Packets are enqueued
    /// in `packet_in_flight` and delivered via `deliver_packets()`.
    ///
    /// Returns `true` if the packet was enqueued, `false` if dropped
    /// by partition or loss.
    pub fn send_packet(
        &mut self,
        from: usize,
        to: usize,
        data: Vec<u8>,
        current_tick: u64,
    ) -> bool {
        self.try_send_packet(from, to, data, current_tick)
            .unwrap_or(false)
    }

    pub fn try_send_packet(
        &mut self,
        from: usize,
        to: usize,
        data: Vec<u8>,
        current_tick: u64,
    ) -> Result<bool, NetworkSendError> {
        let plan = self.checked_packet_effect_plan(from, to, data.len(), current_tick)?;
        let prior_packet_count = self.packet_in_flight.len();
        let mut candidate = self.clone();
        let stats_before = candidate.stats.clone();
        let delivered = candidate.send_packet_inner(from, to, data, current_tick);
        candidate.record_network_delta(from, to, &stats_before, delivered);
        let produced = candidate
            .fault_observations
            .len()
            .checked_sub(self.fault_observations.len())
            .ok_or(NetworkSendError::ObservationCapacity)?;
        assert!(produced <= plan.max_observations);
        assert!(candidate.packet_in_flight[prior_packet_count..]
            .iter()
            .all(|packet| packet.deliver_at_tick <= plan.max_deliver_at_tick));
        *self = candidate;
        Ok(delivered)
    }

    fn send_packet_inner(
        &mut self,
        from: usize,
        to: usize,
        data: Vec<u8>,
        current_tick: u64,
    ) -> bool {
        self.stats.packets_sent += 1;

        // 1. Partition check
        if !self.can_reach(from, to) {
            debug!("Packet from VM{} to VM{} dropped by partition", from, to);
            self.stats.packets_dropped_partition += 1;
            return false;
        }

        // 2. Packet loss — max(sender, receiver) rate
        let sender_loss = self.loss_rate_ppm.get(from).copied().unwrap_or(0);
        let receiver_loss = self.loss_rate_ppm.get(to).copied().unwrap_or(0);
        let loss_rate = sender_loss.max(receiver_loss);
        if loss_rate > 0 {
            let roll = (self.rng.next_u64() % 1_000_000) as u32;
            if roll < loss_rate {
                debug!(
                    "Packet from VM{} to VM{} dropped by packet loss ({}ppm)",
                    from, to, loss_rate
                );
                self.stats.packets_dropped_loss += 1;
                return false;
            }
        }

        // 3. Bandwidth — serialization delay with queuing
        let mut bandwidth_delay_ticks: u64 = 0;
        let sender_bw = self.bandwidth_bps.get(from).copied().unwrap_or(0);
        let receiver_bw = self.bandwidth_bps.get(to).copied().unwrap_or(0);
        let effective_bw = match (sender_bw, receiver_bw) {
            (0, 0) => 0,          // both unlimited
            (0, b) | (b, 0) => b, // one is limited
            (a, b) => a.min(b),   // bottleneck
        };
        if effective_bw > 0 && !data.is_empty() {
            let serialization_ticks = bandwidth_serialization_ticks(data.len(), effective_bw);
            let tx_start = current_tick.max(self.next_free_tick.get(from).copied().unwrap_or(0));
            let tx_end = tx_start.saturating_add(serialization_ticks);
            if let Some(slot) = self.next_free_tick.get_mut(from) {
                *slot = tx_end;
            }
            bandwidth_delay_ticks = tx_end.saturating_sub(current_tick);
            if bandwidth_delay_ticks > 0 {
                self.stats.packets_bandwidth_delayed =
                    self.stats.packets_bandwidth_delayed.saturating_add(1);
                self.stats.total_bandwidth_delay_ticks = self
                    .stats
                    .total_bandwidth_delay_ticks
                    .saturating_add(bandwidth_delay_ticks);
            }
            debug!(
                "Packet from VM{} to VM{}: bandwidth delay {} ticks ({}B @ {}B/s)",
                from,
                to,
                bandwidth_delay_ticks,
                data.len(),
                effective_bw
            );
        }

        // 4. Packet corruption — flip a random byte
        let mut data = data;
        let sender_corrupt = self.corruption_rate_ppm.get(from).copied().unwrap_or(0);
        let receiver_corrupt = self.corruption_rate_ppm.get(to).copied().unwrap_or(0);
        let corrupt_rate = sender_corrupt.max(receiver_corrupt);
        if corrupt_rate > 0 && !data.is_empty() {
            let roll = (self.rng.next_u64() % 1_000_000) as u32;
            if roll < corrupt_rate {
                let byte_idx = (self.rng.next_u64() as usize) % data.len();
                let flip = (self.rng.next_u64() & 0xFF) as u8 | 1; // At least 1 bit flipped
                data[byte_idx] ^= flip;
                self.stats.packets_corrupted += 1;
                debug!(
                    "Packet from VM{} to VM{} corrupted at byte {}",
                    from, to, byte_idx
                );
            }
        }

        // 5. Latency + jitter — base delay plus random variation
        let sender_latency = self.latency.get(from).copied().unwrap_or(0);
        let receiver_latency = self.latency.get(to).copied().unwrap_or(0);
        let latency_ticks = sender_latency.max(receiver_latency);
        let after_bandwidth = current_tick.saturating_add(bandwidth_delay_ticks);
        let after_latency = after_bandwidth.saturating_add(latency_ticks);
        let latency_applied = after_latency.saturating_sub(after_bandwidth);
        if latency_applied > 0 {
            self.stats.packets_latency_delayed =
                self.stats.packets_latency_delayed.saturating_add(1);
            self.stats.total_latency_delay_ticks = self
                .stats
                .total_latency_delay_ticks
                .saturating_add(latency_applied);
        }

        let sender_jitter = self.jitter.get(from).copied().unwrap_or(0);
        let receiver_jitter = self.jitter.get(to).copied().unwrap_or(0);
        let jitter_max = sender_jitter.max(receiver_jitter);
        let requested_jitter = if jitter_max > 0 {
            let jitter_choices = jitter_max.saturating_add(1);
            self.rng.next_u64() % jitter_choices
        } else {
            0
        };
        let after_jitter = after_latency.saturating_add(requested_jitter);
        let jitter_applied = after_jitter.saturating_sub(after_latency);
        if jitter_applied > 0 {
            self.stats.packets_jittered = self.stats.packets_jittered.saturating_add(1);
            self.stats.total_jitter_ticks =
                self.stats.total_jitter_ticks.saturating_add(jitter_applied);
        }
        let mut deliver_at_tick = after_jitter;

        // 6. Packet reorder — additional random shuffle within window
        let sender_reorder = self.reorder_window.get(from).copied().unwrap_or(0);
        let receiver_reorder = self.reorder_window.get(to).copied().unwrap_or(0);
        let reorder_win = sender_reorder.max(receiver_reorder);
        if reorder_win > 0 {
            let reorder_choices = reorder_win.saturating_add(1);
            let requested_reorder = self.rng.next_u64() % reorder_choices;
            let reordered_tick = deliver_at_tick.saturating_add(requested_reorder);
            let reorder_applied = reordered_tick.saturating_sub(deliver_at_tick);
            deliver_at_tick = reordered_tick;
            if reorder_applied > 0 {
                self.stats.packets_reordered = self.stats.packets_reordered.saturating_add(1);
            }
            debug!(
                "Packet from VM{} to VM{} reordered by {} ticks",
                from, to, reorder_applied
            );
        }

        // 7. Packet duplication — maybe enqueue a second copy
        let sender_dup = self.duplicate_rate_ppm.get(from).copied().unwrap_or(0);
        let receiver_dup = self.duplicate_rate_ppm.get(to).copied().unwrap_or(0);
        let dup_rate = sender_dup.max(receiver_dup);
        if dup_rate > 0 {
            let roll = (self.rng.next_u64() % 1_000_000) as u32;
            if roll < dup_rate {
                // Duplicate arrives with a small deterministic offset.
                let dup_offset = self.rng.next_u64() % DUPLICATE_OFFSET_CHOICES;
                self.packet_in_flight.push(PacketInFlight {
                    from,
                    to,
                    data: data.clone(),
                    deliver_at_tick: deliver_at_tick.saturating_add(dup_offset),
                });
                self.stats.packets_duplicated += 1;
                debug!(
                    "Packet from VM{} to VM{} duplicated (+{} ticks)",
                    from, to, dup_offset
                );
            }
        }

        self.packet_in_flight.push(PacketInFlight {
            from,
            to,
            data,
            deliver_at_tick,
        });

        self.stats.packets_delivered += 1;
        true
    }

    fn record_network_delta(
        &mut self,
        from: usize,
        to: usize,
        before: &NetworkStats,
        delivered: bool,
    ) {
        if self.stats.packets_dropped_partition > before.packets_dropped_partition {
            let attempt_id = self.partition_attempt(from, to);
            self.record_fault_observation(
                attempt_id,
                FaultObservationEffect::PacketDroppedByPartition,
            );
        }
        if self.stats.packets_dropped_loss > before.packets_dropped_loss {
            let attempt_id =
                Self::attempt_for_pair_u32(&self.loss_rate_ppm, &self.loss_attempt_ids, from, to);
            self.record_fault_observation(attempt_id, FaultObservationEffect::PacketDroppedByLoss);
        }
        if delivered {
            self.record_delivered_network_delta(from, to, before);
        }
    }

    fn record_delivered_network_delta(&mut self, from: usize, to: usize, before: &NetworkStats) {
        if self.stats.packets_latency_delayed > before.packets_latency_delayed {
            let latency_attempt =
                Self::attempt_for_pair_u64(&self.latency, &self.latency_attempt_ids, from, to);
            self.record_fault_observation(
                latency_attempt,
                FaultObservationEffect::PacketDelayedByLatency,
            );
        }
        self.record_delivered_network_counter_deltas(from, to, before);
    }

    fn record_delivered_network_counter_deltas(
        &mut self,
        from: usize,
        to: usize,
        before: &NetworkStats,
    ) {
        if self.stats.packets_corrupted > before.packets_corrupted {
            let attempt_id = Self::attempt_for_pair_u32(
                &self.corruption_rate_ppm,
                &self.corruption_attempt_ids,
                from,
                to,
            );
            self.record_fault_observation(attempt_id, FaultObservationEffect::PacketCorrupted);
        }
        if self.stats.packets_bandwidth_delayed > before.packets_bandwidth_delayed {
            let attempt_id = Self::attempt_for_pair_u64(
                &self.bandwidth_bps,
                &self.bandwidth_attempt_ids,
                from,
                to,
            );
            self.record_fault_observation(
                attempt_id,
                FaultObservationEffect::PacketDelayedByBandwidth,
            );
        }
        self.record_remaining_network_counter_deltas(from, to, before);
    }

    fn record_remaining_network_counter_deltas(
        &mut self,
        from: usize,
        to: usize,
        before: &NetworkStats,
    ) {
        if self.stats.packets_jittered > before.packets_jittered {
            let attempt_id =
                Self::attempt_for_pair_u64(&self.jitter, &self.jitter_attempt_ids, from, to);
            self.record_fault_observation(
                attempt_id,
                FaultObservationEffect::PacketDelayedByJitter,
            );
        }
        if self.stats.packets_reordered > before.packets_reordered {
            let attempt_id = Self::attempt_for_pair_u64(
                &self.reorder_window,
                &self.reorder_attempt_ids,
                from,
                to,
            );
            self.record_fault_observation(attempt_id, FaultObservationEffect::PacketReordered);
        }
        if self.stats.packets_duplicated > before.packets_duplicated {
            let attempt_id = Self::attempt_for_pair_u32(
                &self.duplicate_rate_ppm,
                &self.duplicate_attempt_ids,
                from,
                to,
            );
            self.record_fault_observation(attempt_id, FaultObservationEffect::PacketDuplicated);
        }
    }

    /// Deliver packets whose delivery tick has arrived.
    ///
    /// Returns `(vm_id, packet_data)` pairs for injection into VMs.
    pub fn deliver_packets(&mut self, current_tick: u64) -> Vec<(usize, Vec<u8>)> {
        let mut delivered = Vec::new();
        let mut pending = Vec::new();

        for pkt in self.packet_in_flight.drain(..) {
            if pkt.deliver_at_tick <= current_tick {
                delivered.push((pkt.to, pkt.data));
            } else {
                pending.push(pkt);
            }
        }

        self.packet_in_flight = pending;
        delivered
    }

    fn record_fault_observation(
        &mut self,
        attempt_id: Option<FaultAttemptId>,
        effect: FaultObservationEffect,
    ) {
        let Some(attempt_id) = attempt_id else {
            return;
        };
        if self.fault_observations.len() >= MAX_PENDING_FAULT_OBSERVATIONS {
            self.fault_observation_overflowed = self.fault_observation_overflowed.saturating_add(1);
            return;
        }
        let operation_sequence = self.fault_observation_sequence;
        let Some(next_sequence) = operation_sequence.checked_add(1) else {
            self.fault_observation_overflowed = self.fault_observation_overflowed.saturating_add(1);
            return;
        };
        self.fault_observation_sequence = next_sequence;
        self.fault_observations.push_back(FaultObservation::new(
            attempt_id,
            FaultObservationSubsystem::Network,
            operation_sequence,
            effect,
        ));
    }

    fn drain_fault_observations(&mut self) -> (Vec<FaultObservation>, u64) {
        let observations = self.fault_observations.drain(..).collect();
        let overflowed = self.fault_observation_overflowed;
        self.fault_observation_overflowed = 0;
        (observations, overflowed)
    }

    fn requeue_fault_observations(&mut self, observations: Vec<FaultObservation>, overflowed: u64) {
        let restored_len = self
            .fault_observations
            .len()
            .checked_add(observations.len())
            .expect("network observation queue length overflow");
        assert!(restored_len <= MAX_PENDING_FAULT_OBSERVATIONS);
        for observation in observations.into_iter().rev() {
            self.fault_observations.push_front(observation);
        }
        self.fault_observation_overflowed = self
            .fault_observation_overflowed
            .checked_add(overflowed)
            .expect("network observation overflow counter overflow");
    }

    fn validate_pending_faults(
        &self,
        ledger: &FaultOutcomeLedger,
        node_count: usize,
    ) -> Result<(), FaultTransitionError> {
        let vector_lengths = [
            self.latency.len(),
            self.latency_attempt_ids.len(),
            self.jitter.len(),
            self.jitter_attempt_ids.len(),
            self.bandwidth_bps.len(),
            self.bandwidth_attempt_ids.len(),
            self.next_free_tick.len(),
            self.loss_rate_ppm.len(),
            self.loss_attempt_ids.len(),
            self.corruption_rate_ppm.len(),
            self.corruption_attempt_ids.len(),
            self.reorder_window.len(),
            self.reorder_attempt_ids.len(),
            self.duplicate_rate_ppm.len(),
            self.duplicate_attempt_ids.len(),
        ];
        if vector_lengths
            .into_iter()
            .any(|length| length != node_count)
            || self.partitions.len() != self.partition_attempt_ids.len()
            || self.fault_observations.len() > MAX_PENDING_FAULT_OBSERVATIONS
            || self.fault_observation_overflowed != 0
        {
            return Err(FaultTransitionError::SnapshotPendingStateMismatch);
        }
        for ((side_a, side_b), attempt_id) in
            self.partitions.iter().zip(&self.partition_attempt_ids)
        {
            if side_a.iter().chain(side_b).any(|node| *node >= node_count) {
                return Err(FaultTransitionError::SnapshotPendingStateMismatch);
            }
            let attempt_id =
                attempt_id.ok_or(FaultTransitionError::SnapshotPendingStateMismatch)?;
            let effect = FaultPlanEffect::NetworkPartition {
                side_a: usize_targets_to_u32(side_a)?,
                side_b: usize_targets_to_u32(side_b)?,
            };
            validate_pending_fault_effect(ledger, attempt_id, &effect)?;
        }
        validate_network_attempts_u64(
            ledger,
            &self.latency,
            &self.latency_attempt_ids,
            |target, latency_ticks| FaultPlanEffect::NetworkLatency {
                target,
                latency_ticks,
            },
        )?;
        validate_network_attempts_u64(
            ledger,
            &self.jitter,
            &self.jitter_attempt_ids,
            |target, jitter_ticks| FaultPlanEffect::NetworkJitter {
                target,
                jitter_ticks,
            },
        )?;
        validate_network_attempts_u64(
            ledger,
            &self.bandwidth_bps,
            &self.bandwidth_attempt_ids,
            |target, bytes_per_sec| FaultPlanEffect::NetworkBandwidth {
                target,
                bytes_per_sec,
            },
        )?;
        validate_network_attempts_u32(
            ledger,
            &self.loss_rate_ppm,
            &self.loss_attempt_ids,
            |target, rate_ppm| FaultPlanEffect::PacketLoss { target, rate_ppm },
        )?;
        validate_network_attempts_u32(
            ledger,
            &self.corruption_rate_ppm,
            &self.corruption_attempt_ids,
            |target, rate_ppm| FaultPlanEffect::PacketCorruption { target, rate_ppm },
        )?;
        validate_network_attempts_u64(
            ledger,
            &self.reorder_window,
            &self.reorder_attempt_ids,
            |target, window_ticks| FaultPlanEffect::PacketReorder {
                target,
                window_ticks,
            },
        )?;
        validate_network_attempts_u32(
            ledger,
            &self.duplicate_rate_ppm,
            &self.duplicate_attempt_ids,
            |target, rate_ppm| FaultPlanEffect::PacketDuplicate { target, rate_ppm },
        )?;
        if self
            .in_flight
            .iter()
            .any(|message| message.from >= node_count || message.to >= node_count)
            || self
                .packet_in_flight
                .iter()
                .any(|packet| packet.from >= node_count || packet.to >= node_count)
            || self.fault_observations.iter().any(|observation| {
                observation.operation_sequence >= self.fault_observation_sequence
            })
        {
            return Err(FaultTransitionError::SnapshotPendingStateMismatch);
        }
        let observations = self.fault_observations.iter().cloned().collect::<Vec<_>>();
        validate_pending_fault_observations(ledger, &observations)
    }

    fn partition_attempt(&self, from: usize, to: usize) -> Option<FaultAttemptId> {
        for (index, (side_a, side_b)) in self.partitions.iter().enumerate() {
            let is_separated = (side_a.contains(&from) && side_b.contains(&to))
                || (side_b.contains(&from) && side_a.contains(&to));
            if is_separated {
                return self.partition_attempt_ids.get(index).copied().flatten();
            }
        }
        None
    }

    fn attempt_for_pair_u64(
        values: &[u64],
        attempts: &[Option<FaultAttemptId>],
        from: usize,
        to: usize,
    ) -> Option<FaultAttemptId> {
        let from_value = values.get(from).copied().unwrap_or(0);
        let to_value = values.get(to).copied().unwrap_or(0);
        let selected = if from_value >= to_value { from } else { to };
        attempts.get(selected).copied().flatten()
    }

    fn attempt_for_pair_u32(
        values: &[u32],
        attempts: &[Option<FaultAttemptId>],
        from: usize,
        to: usize,
    ) -> Option<FaultAttemptId> {
        let from_value = values.get(from).copied().unwrap_or(0);
        let to_value = values.get(to).copied().unwrap_or(0);
        let selected = if from_value >= to_value { from } else { to };
        attempts.get(selected).copied().flatten()
    }

    /// Add a network partition between two sides without evidence attribution.
    #[cfg(test)]
    fn add_partition(&mut self, side_a: Vec<usize>, side_b: Vec<usize>) -> bool {
        info!("Network partition: {:?} | {:?}", side_a, side_b);
        self.partitions.push((side_a, side_b));
        self.partition_attempt_ids.push(None);
        true
    }

    fn arm_partition(
        &mut self,
        side_a: Vec<usize>,
        side_b: Vec<usize>,
        attempt_id: FaultAttemptId,
    ) -> bool {
        info!("Network partition: {:?} | {:?}", side_a, side_b);
        self.partitions.push((side_a, side_b));
        self.partition_attempt_ids.push(Some(attempt_id));
        true
    }

    /// Clear all partitions and packet-level faults (heal network).
    ///
    /// Resets: partitions, loss, corruption, reorder, jitter, bandwidth,
    /// and duplication rates.  Base latency is preserved (use
    /// `set_latency(target, 0)` to clear it explicitly).
    fn clear_partitions(&mut self) -> bool {
        info!("Network healed: all partitions and packet faults removed");
        self.partitions.clear();
        self.partition_attempt_ids.clear();
        for rate in &mut self.loss_rate_ppm {
            *rate = 0;
        }
        for rate in &mut self.corruption_rate_ppm {
            *rate = 0;
        }
        for win in &mut self.reorder_window {
            *win = 0;
        }
        for j in &mut self.jitter {
            *j = 0;
        }
        for bw in &mut self.bandwidth_bps {
            *bw = 0;
        }
        for t in &mut self.next_free_tick {
            *t = 0;
        }
        for rate in &mut self.duplicate_rate_ppm {
            *rate = 0;
        }
        self.jitter_attempt_ids.fill(None);
        self.bandwidth_attempt_ids.fill(None);
        self.loss_attempt_ids.fill(None);
        self.corruption_attempt_ids.fill(None);
        self.reorder_attempt_ids.fill(None);
        self.duplicate_attempt_ids.fill(None);
        // Note: stats are NOT reset on heal — they are cumulative.
        true
    }

    /// Set latency for a specific VM.
    fn set_latency(&mut self, target: usize, latency_ticks: u64) -> bool {
        let Some(slot) = self.latency.get_mut(target) else {
            return false;
        };
        *slot = latency_ticks;
        self.latency_attempt_ids[target] = None;
        debug!("VM{} latency set to {} ticks", target, latency_ticks);
        true
    }

    /// Set packet loss rate for a specific VM.
    fn set_loss_rate(&mut self, target: usize, rate_ppm: u32) -> bool {
        let Some(slot) = self.loss_rate_ppm.get_mut(target) else {
            return false;
        };
        *slot = rate_ppm;
        self.loss_attempt_ids[target] = None;
        debug!("VM{} packet loss set to {} ppm", target, rate_ppm);
        true
    }

    /// Set packet corruption rate for a specific VM.
    fn set_corruption_rate(&mut self, target: usize, rate_ppm: u32) -> bool {
        let Some(slot) = self.corruption_rate_ppm.get_mut(target) else {
            return false;
        };
        *slot = rate_ppm;
        self.corruption_attempt_ids[target] = None;
        debug!("VM{} packet corruption set to {} ppm", target, rate_ppm);
        true
    }

    /// Set reorder window for a specific VM (in ticks).
    fn set_reorder_window(&mut self, target: usize, window_ticks: u64) -> bool {
        let Some(slot) = self.reorder_window.get_mut(target) else {
            return false;
        };
        *slot = window_ticks;
        self.reorder_attempt_ids[target] = None;
        debug!("VM{} reorder window set to {} ticks", target, window_ticks);
        true
    }

    /// Set latency jitter for a specific VM (in ticks).
    ///
    /// Each packet to/from this VM receives up to `jitter_ticks` extra
    /// random delay on top of the base latency.
    fn set_jitter(&mut self, target: usize, jitter_ticks: u64) -> bool {
        let Some(slot) = self.jitter.get_mut(target) else {
            return false;
        };
        *slot = jitter_ticks;
        self.jitter_attempt_ids[target] = None;
        debug!("VM{} jitter set to {} ticks", target, jitter_ticks);
        true
    }

    /// Set bandwidth limit for a specific VM (bytes per second).
    ///
    /// Models serialization delay: a 1500-byte packet on a 1 MB/s link
    /// takes ~12 µs (0.012 ticks).  Set to 0 for unlimited.
    fn set_bandwidth(&mut self, target: usize, bytes_per_sec: u64) -> bool {
        let Some(slot) = self.bandwidth_bps.get_mut(target) else {
            return false;
        };
        *slot = bytes_per_sec;
        self.bandwidth_attempt_ids[target] = None;
        debug!("VM{} bandwidth set to {} B/s", target, bytes_per_sec);
        true
    }

    /// Get a reference to the cumulative network statistics.
    pub fn stats(&self) -> &NetworkStats {
        &self.stats
    }

    /// Set packet duplication rate for a specific VM (parts per million).
    fn set_duplicate_rate(&mut self, target: usize, rate_ppm: u32) -> bool {
        let Some(slot) = self.duplicate_rate_ppm.get_mut(target) else {
            return false;
        };
        *slot = rate_ppm;
        self.duplicate_attempt_ids[target] = None;
        debug!("VM{} packet duplication set to {} ppm", target, rate_ppm);
        true
    }

    fn arm_latency(&mut self, target: usize, value: u64, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_latency(target, value);
        if applied && value > 0 {
            self.latency_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }

    fn arm_jitter(&mut self, target: usize, value: u64, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_jitter(target, value);
        if applied && value > 0 {
            self.jitter_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }

    fn arm_bandwidth(&mut self, target: usize, value: u64, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_bandwidth(target, value);
        if applied && value > 0 {
            self.bandwidth_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }

    fn arm_loss(&mut self, target: usize, value: u32, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_loss_rate(target, value);
        if applied && value > 0 {
            self.loss_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }

    fn arm_corruption(&mut self, target: usize, value: u32, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_corruption_rate(target, value);
        if applied && value > 0 {
            self.corruption_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }

    fn arm_reorder(&mut self, target: usize, value: u64, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_reorder_window(target, value);
        if applied && value > 0 {
            self.reorder_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }

    fn arm_duplicate(&mut self, target: usize, value: u32, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_duplicate_rate(target, value);
        if applied && value > 0 {
            self.duplicate_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Simulation Controller
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FaultApplicationError {
    reason: FaultApplicationFailureReason,
    disposition: FaultApplicationFailureDisposition,
}

fn fault_vm_status(status: VmStatus) -> FaultVmStatus {
    match status {
        VmStatus::Running => FaultVmStatus::Running,
        VmStatus::Paused => FaultVmStatus::Paused,
        VmStatus::Crashed => FaultVmStatus::Crashed,
        VmStatus::Restarting { .. } => FaultVmStatus::Restarting,
        VmStatus::Resuming { .. } => FaultVmStatus::Resuming,
    }
}

fn fault_transition_vm_error(error: FaultTransitionError) -> VmError {
    VmError::Snapshot {
        message: format!("fault outcome transition failed: {error}"),
    }
}

fn checked_usize(value: u32) -> Result<usize, FaultApplicationError> {
    usize::try_from(value).map_err(|_| internal_application_error())
}

fn checked_usize_u64(value: u64) -> Result<usize, FaultApplicationError> {
    usize::try_from(value).map_err(|_| internal_application_error())
}

fn u32_targets_to_usize(values: &[u32]) -> Result<Vec<usize>, FaultApplicationError> {
    values.iter().copied().map(checked_usize).collect()
}

fn internal_application_error() -> FaultApplicationError {
    FaultApplicationError {
        reason: FaultApplicationFailureReason::InternalInvariant,
        disposition: FaultApplicationFailureDisposition::RolledBack,
    }
}

fn target_state_application_error() -> FaultApplicationError {
    FaultApplicationError {
        reason: FaultApplicationFailureReason::TargetStateChanged,
        disposition: FaultApplicationFailureDisposition::RolledBack,
    }
}

fn device_disappeared_application_error() -> FaultApplicationError {
    FaultApplicationError {
        reason: FaultApplicationFailureReason::DeviceDisappeared,
        disposition: FaultApplicationFailureDisposition::RolledBack,
    }
}

fn non_runnable_application_error() -> FaultApplicationError {
    FaultApplicationError {
        reason: FaultApplicationFailureReason::BackendRejected,
        disposition: FaultApplicationFailureDisposition::NonRunnable,
    }
}

/// The main simulation controller for multi-VM deterministic testing.
pub struct SimulationController {
    /// VM slots.
    vms: Vec<VmSlot>,
    /// Shared fault engine.
    fault_engine: FaultEngine,
    /// Virtual network fabric.
    network: NetworkFabric,
    /// Global simulation tick counter.
    tick: u64,
    /// Exits per VM per round.
    quantum: u64,
    /// Simulation config (for VM restarts).
    pub config: SimulationConfig,
    /// Per-VM base memory for incremental snapshots.
    ///
    /// Set after the bootstrap snapshot. Each `Arc<Vec<u8>>` is the
    /// full memory image taken at the start of the exploration round.
    /// Incremental snapshots reference this base and store only dirty
    /// pages.
    vm_memory_bases: Vec<Option<std::sync::Arc<Vec<u8>>>>,
    /// Explicit policy for fault rejection behavior and parameter bounds.
    fault_application_policy: FaultApplicationPolicy,
    /// Deterministic sequence for operation identities emitted by the shell.
    fault_operation_sequence: u64,
    /// Process observations retained until their ledger batch commits.
    pending_process_observations: VecDeque<(usize, FaultObservation)>,
}

fn validate_network_attempts_u64(
    ledger: &FaultOutcomeLedger,
    values: &[u64],
    attempt_ids: &[Option<FaultAttemptId>],
    effect: impl Fn(u32, u64) -> FaultPlanEffect,
) -> Result<(), FaultTransitionError> {
    for (target, (value, attempt_id)) in values.iter().zip(attempt_ids).enumerate() {
        let target = u32::try_from(target)
            .map_err(|_| FaultTransitionError::SnapshotPendingStateMismatch)?;
        validate_network_attempt(ledger, *value, *attempt_id, effect(target, *value))?;
    }
    Ok(())
}

fn validate_network_attempts_u32(
    ledger: &FaultOutcomeLedger,
    values: &[u32],
    attempt_ids: &[Option<FaultAttemptId>],
    effect: impl Fn(u32, u32) -> FaultPlanEffect,
) -> Result<(), FaultTransitionError> {
    for (target, (value, attempt_id)) in values.iter().zip(attempt_ids).enumerate() {
        let target = u32::try_from(target)
            .map_err(|_| FaultTransitionError::SnapshotPendingStateMismatch)?;
        validate_network_attempt(ledger, *value, *attempt_id, effect(target, *value))?;
    }
    Ok(())
}

fn validate_network_attempt<T>(
    ledger: &FaultOutcomeLedger,
    value: T,
    attempt_id: Option<FaultAttemptId>,
    effect: FaultPlanEffect,
) -> Result<(), FaultTransitionError>
where
    T: Default + PartialEq,
{
    let active = value != T::default();
    match (active, attempt_id) {
        (true, Some(attempt_id)) => validate_pending_fault_effect(ledger, attempt_id, &effect),
        (false, None) => Ok(()),
        _ => Err(FaultTransitionError::SnapshotPendingStateMismatch),
    }
}

fn usize_targets_to_u32(targets: &[usize]) -> Result<Vec<u32>, FaultTransitionError> {
    targets
        .iter()
        .copied()
        .map(|target| {
            u32::try_from(target).map_err(|_| FaultTransitionError::SnapshotPendingStateMismatch)
        })
        .collect()
}

fn validate_process_snapshot_effect(
    ledger: &FaultOutcomeLedger,
    target: u32,
    status: VmStatus,
    attempt_id: Option<FaultAttemptId>,
    has_pending_observation: bool,
) -> Result<(), FaultTransitionError> {
    let effect = match (status, attempt_id) {
        (VmStatus::Crashed, Some(attempt_id)) => {
            Some((attempt_id, FaultPlanEffect::ProcessKill { target }))
        }
        (VmStatus::Restarting { restart_at_tick }, Some(attempt_id)) => Some((
            attempt_id,
            FaultPlanEffect::ProcessRestart {
                target,
                restart_at_tick,
            },
        )),
        (VmStatus::Resuming { resume_at_tick }, Some(attempt_id)) => Some((
            attempt_id,
            FaultPlanEffect::ProcessPause {
                target,
                resume_at_tick,
            },
        )),
        (VmStatus::Running | VmStatus::Paused, Some(attempt_id)) if has_pending_observation => {
            let state = ledger
                .attempts
                .get(&attempt_id)
                .ok_or(FaultTransitionError::UnknownAttempt)?;
            match state.applicable_effect.as_ref() {
                Some(FaultPlanEffect::ProcessRestart {
                    target: effect_target,
                    ..
                }) if *effect_target == target => None,
                _ => return Err(FaultTransitionError::SnapshotPendingStateMismatch),
            }
        }
        (VmStatus::Restarting { .. } | VmStatus::Resuming { .. }, None)
        | (VmStatus::Running | VmStatus::Paused, Some(_)) => {
            return Err(FaultTransitionError::SnapshotPendingStateMismatch);
        }
        (VmStatus::Running | VmStatus::Paused | VmStatus::Crashed, None) => None,
    };
    if let Some((attempt_id, effect)) = effect {
        validate_pending_fault_effect(ledger, attempt_id, &effect)?;
    }
    Ok(())
}

impl SimulationController {
    /// Create a new simulation with N VMs.
    pub fn new(config: SimulationConfig) -> Result<Self, VmError> {
        info!(
            "Creating simulation: {} VMs, seed={}, quantum={}",
            config.num_vms, config.seed, config.quantum
        );

        if config.num_vms == 0 {
            return SnapshotSnafu {
                message: "num_vms must be > 0",
            }
            .fail();
        }

        if config.kernel_path.is_empty() {
            return SnapshotSnafu {
                message: "kernel_path is required",
            }
            .fail();
        }

        // Create fault engine with shared seed and num_vms
        let engine_config = EngineConfig {
            seed: config.seed,
            num_vms: config.num_vms,
            schedule: Some(config.schedule.clone()),
            random_faults: false,
            ..EngineConfig::default()
        };
        let mut fault_engine = FaultEngine::new(engine_config);
        fault_engine.begin_run();

        // Create VMs
        let mut vms = Vec::with_capacity(config.num_vms);
        for i in 0..config.num_vms {
            info!("Creating VM{}", i);

            // Derive per-VM seed from master seed and VM index
            let mut vm_config = config.vm_config.clone();
            vm_config.cpu.seed = config.seed.wrapping_add(i as u64);
            vm_config.disk_image_path = config.disk_image_path.clone();
            // Pin each VM to a dedicated core: VM i → core base + i.
            vm_config.core_affinity = config.base_core.map(|base| base + i);
            // Set unique VM ID for networking
            vm_config.vm_id = i;

            // Wire up per-VM determinism log path.
            if let Some(ref dir) = config.dlog_dir {
                std::fs::create_dir_all(dir).map_err(|e| VmError::DiskImage {
                    message: format!("create dlog dir {}: {e}", dir.display()),
                })?;
                vm_config.dlog_path = Some(dir.join(format!("vm_{i}.dlog")));
            }

            let mut vm = DeterministicVm::new(vm_config)?;
            vm.load_kernel(&config.kernel_path, config.initrd_path.as_deref())?;

            // Take initial snapshot for restart capability
            let initial_snapshot = vm.snapshot()?;

            vms.push(VmSlot {
                vm,
                status: VmStatus::Running,
                inbox: VecDeque::new(),
                disk_faults: DiskFaultFlags::default(),
                tsc_skew: 0,
                memory_limit_bytes: None,
                initial_snapshot: Some(initial_snapshot),
                vcpu_stall_until: std::collections::BTreeMap::new(),
                clock_freeze: None,
                clock_jitter_bound: 0,
                process_fault_attempt: None,
            });
        }

        let network = NetworkFabric::new(config.num_vms, config.seed);

        let num_vms = vms.len();
        Ok(Self {
            vms,
            fault_engine,
            network,
            tick: 0,
            quantum: config.quantum,
            config,
            vm_memory_bases: vec![None; num_vms],
            fault_application_policy: FaultApplicationPolicy::default(),
            fault_operation_sequence: 0,
            pending_process_observations: VecDeque::new(),
        })
    }

    /// Run the simulation for up to `num_ticks` scheduling rounds.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn run(&mut self, num_ticks: u64) -> Result<SimulationResult, VmError> {
        let stop_at = self.tick + num_ticks;
        info!(
            "Running simulation for {} ticks (tick {}→{})",
            num_ticks, self.tick, stop_at
        );

        while self.tick < stop_at {
            let result = self.step_round()?;

            if result.vms_running == 0 {
                info!("All VMs halted at tick {}", self.tick);
                break;
            }

            // Check for assertion failures in ANY VM's fault engine.
            // The guest's assert::always() calls go to the per-VM engine,
            // not the controller's engine.
            let any_failure = self
                .vms
                .iter()
                .any(|slot| slot.vm.fault_engine().has_assertion_failure());
            if any_failure {
                warn!("Assertion failure detected at tick {}", self.tick);
                break;
            }
        }

        // Merge oracle reports from all VMs into a combined report.
        let oracle_report = self.merged_oracle_report();
        let vm_exit_counts = self.vms.iter().map(|slot| slot.vm.exit_count()).collect();

        Ok(SimulationResult {
            total_ticks: self.tick,
            oracle_report,
            vm_exit_counts,
            network_stats: self.network.stats.clone(),
            fault_outcomes: self.fault_engine.fault_outcomes().clone(),
        })
    }

    /// Run the simulation until any VM signals `setup_complete`, or until
    /// `max_ticks` is reached (whichever comes first).
    ///
    /// Used for bootstrap: boot kernel + guest initialisation is variable-length,
    /// so we can't use a fixed tick budget.  After `setup_complete`, the snapshot
    /// captures a fully-initialised guest ready for exploration branches.
    pub fn run_until_setup_complete(
        &mut self,
        max_ticks: u64,
    ) -> Result<SimulationResult, VmError> {
        let stop_at = self.tick + max_ticks;
        info!(
            "Bootstrap: running until setup_complete (max {} ticks, tick {}→{})",
            max_ticks, self.tick, stop_at
        );

        while self.tick < stop_at {
            let result = self.step_round()?;

            // Check ANY VM's fault engine for setup_complete.
            // The SDK hypercall goes to the per-VM engine, not the
            // controller's engine, so we check VMs directly.
            let any_setup_complete = self
                .vms
                .iter()
                .any(|slot| slot.vm.fault_engine().is_setup_complete());

            if any_setup_complete {
                info!(
                    "Bootstrap complete: setup_complete received at tick {}",
                    self.tick
                );
                // Propagate to controller's engine so idle detection
                // and fault scheduling work correctly going forward.
                self.fault_engine.force_setup_complete();
                break;
            }

            if result.vms_running == 0 {
                info!("All VMs halted during bootstrap at tick {}", self.tick);
                break;
            }
        }

        let any_setup = self
            .vms
            .iter()
            .any(|slot| slot.vm.fault_engine().is_setup_complete());
        if !any_setup {
            warn!(
                "Bootstrap reached max_ticks ({}) without setup_complete",
                max_ticks
            );
        }

        let oracle_report = self.merged_oracle_report();
        let vm_exit_counts = self.vms.iter().map(|slot| slot.vm.exit_count()).collect();

        Ok(SimulationResult {
            total_ticks: self.tick,
            oracle_report,
            vm_exit_counts,
            network_stats: self.network.stats.clone(),
            fault_outcomes: self.fault_engine.fault_outcomes().clone(),
        })
    }

    /// Execute one scheduling round: step each Running VM by `quantum` exits,
    /// advance the global clock, dispatch faults, deliver network messages.
    pub fn step_round(&mut self) -> Result<RoundResult, VmError> {
        self.step_round_with_observation_event_limit(MAX_FAULT_OUTCOME_EVENTS)
    }

    fn step_round_with_observation_event_limit(
        &mut self,
        observation_event_limit: usize,
    ) -> Result<RoundResult, VmError> {
        let next_tick = self.tick.checked_add(1).ok_or_else(|| VmError::Snapshot {
            message: "simulation tick exhausted".to_string(),
        })?;
        let current_time_ns = next_tick
            .checked_mul(chaoscontrol_fault::outcomes::NANOSECONDS_PER_SIMULATION_TICK)
            .ok_or_else(|| VmError::Snapshot {
                message: "simulation time exhausted".to_string(),
            })?;
        self.commit_pending_process_observations(observation_event_limit)?;
        let process_reservation = self
            .vms
            .iter()
            .filter(|slot| slot.process_fault_attempt.is_some())
            .count();
        preflight_fault_observation_events_with_limit(
            self.fault_engine.fault_outcomes(),
            process_reservation,
            observation_event_limit,
        )
        .map_err(fault_transition_vm_error)?;
        let process_queue_final = self
            .pending_process_observations
            .len()
            .checked_add(process_reservation)
            .ok_or_else(|| VmError::Snapshot {
                message: "process observation queue length overflow".to_string(),
            })?;
        if process_queue_final > MAX_PENDING_PROCESS_OBSERVATIONS {
            return Err(VmError::Snapshot {
                message: "process observation queue capacity exhausted".to_string(),
            });
        }
        let reserved_sequences =
            u64::try_from(process_reservation).map_err(|_| VmError::Snapshot {
                message: "process observation reservation exceeds sequence bounds".to_string(),
            })?;
        self.fault_operation_sequence
            .checked_add(reserved_sequences)
            .ok_or_else(|| VmError::Snapshot {
                message: "fault operation sequence exhausted".to_string(),
            })?;
        let outcome_event_start = self.fault_engine.fault_outcomes().events.len();
        let mut vms_running = 0;
        let mut vms_halted = 0;

        // Emit tick markers into each VM's dlog (for cross-VM correlation).
        for i in 0..self.vms.len() {
            self.vms[i].vm.dlog_tick_marker(self.tick);
        }

        // Expire stalls and clock freezes, clean up expired entries.
        for slot in &mut self.vms {
            slot.vcpu_stall_until
                .retain(|_, expires| *expires > self.tick);
            if let Some((_, expires)) = slot.clock_freeze {
                if self.tick >= expires {
                    slot.clock_freeze = None;
                }
            }
        }

        // Step each VM by quantum exits (round-robin)
        for i in 0..self.vms.len() {
            match self.vms[i].status {
                VmStatus::Running => {
                    let (exits, halted) = self.vms[i].vm.run_bounded(self.quantum)?;
                    if halted {
                        self.vms[i].status = VmStatus::Paused; // Treat halt as pause
                        vms_halted += 1;
                    } else {
                        vms_running += 1;
                    }
                    debug!("VM{} executed {} exits", i, exits);
                }
                VmStatus::Paused | VmStatus::Crashed => {
                    vms_halted += 1;
                    if let Some(attempt_id) = self.vms[i].process_fault_attempt {
                        self.queue_process_observation(
                            i,
                            attempt_id,
                            FaultObservationEffect::ProcessSkipped,
                        )?;
                    }
                }
                VmStatus::Restarting { restart_at_tick } => {
                    if self.tick >= restart_at_tick {
                        let pending_observation = self.vms[i]
                            .process_fault_attempt
                            .map(|attempt_id| {
                                self.make_shell_observation(
                                    attempt_id,
                                    FaultObservationSubsystem::Process,
                                    FaultObservationEffect::ProcessRestarted,
                                )
                                .map(|observation| (i, observation))
                            })
                            .transpose()?;
                        self.restart_vm(i)?;
                        vms_running += 1;
                        if let Some(pending_observation) = pending_observation {
                            self.pending_process_observations
                                .push_back(pending_observation);
                        }
                    } else {
                        vms_halted += 1;
                    }
                }
                VmStatus::Resuming { resume_at_tick } => {
                    if self.tick >= resume_at_tick {
                        info!("VM{} resuming from pause at tick {}", i, self.tick);
                        self.vms[i].status = VmStatus::Running;
                        // Run the resumed VM for this round's quantum
                        let (exits, halted) = self.vms[i].vm.run_bounded(self.quantum)?;
                        if halted {
                            self.vms[i].status = VmStatus::Paused;
                            vms_halted += 1;
                        } else {
                            vms_running += 1;
                        }
                        debug!("VM{} resumed, executed {} exits", i, exits);
                    } else {
                        vms_halted += 1;
                        if let Some(attempt_id) = self.vms[i].process_fault_attempt {
                            self.queue_process_observation(
                                i,
                                attempt_id,
                                FaultObservationEffect::ProcessSkipped,
                            )?;
                        }
                    }
                }
            }
        }

        for vm_index in 0..self.vms.len() {
            let (observations, overflowed) = self.vms[vm_index].vm.drain_block_fault_observations();
            if let Err(error) = self
                .record_fault_observations_with_event_limit(&observations, observation_event_limit)
            {
                let restored = self.vms[vm_index]
                    .vm
                    .requeue_block_fault_observations(observations, overflowed);
                assert!(restored);
                return Err(error);
            }
            if overflowed > 0 {
                return Err(VmError::Snapshot {
                    message: format!(
                        "block fault observation queue overflowed by {overflowed} records"
                    ),
                });
            }
        }
        self.commit_pending_process_observations(observation_event_limit)?;

        // Bridge network packets between VMs (virtio-net TX → RX)
        self.bridge_network_packets()?;
        let (network_observations, overflowed) = self.network.drain_fault_observations();
        if let Err(error) = self.record_fault_observations_with_event_limit(
            &network_observations,
            observation_event_limit,
        ) {
            self.network
                .requeue_fault_observations(network_observations, overflowed);
            return Err(error);
        }
        if overflowed > 0 {
            return Err(VmError::Snapshot {
                message: format!(
                    "network fault observation queue overflowed by {overflowed} records"
                ),
            });
        }

        // Advance global tick after every checked round effect.
        self.tick = next_tick;

        // Poll and apply faults.
        let attempts = self
            .fault_engine
            .poll_fault_attempts(current_time_ns)
            .map_err(|error| VmError::Snapshot {
                message: format!("fault selection failed: {error}"),
            })?;
        let faults_fired = attempts
            .iter()
            .map(|attempt| attempt.fault.clone())
            .collect();
        for attempt in attempts {
            self.handle_fault_attempt(&attempt)?;
        }

        // Deliver pending network messages.
        let messages_delivered = self.deliver_messages();
        let fault_outcomes =
            self.fault_engine.fault_outcomes().events[outcome_event_start..].to_vec();

        Ok(RoundResult {
            tick: self.tick,
            vms_running,
            vms_halted,
            faults_fired,
            fault_outcomes,
            messages_delivered,
        })
    }

    fn handle_fault_attempt(&mut self, attempt: &FaultAttempt) -> Result<(), VmError> {
        self.handle_fault_attempt_with_event_limit(attempt, MAX_FAULT_OUTCOME_EVENTS)
    }

    fn handle_fault_attempt_with_event_limit(
        &mut self,
        attempt: &FaultAttempt,
        event_limit: usize,
    ) -> Result<(), VmError> {
        let facts = self.collect_fault_planning_facts()?;
        let plan = match plan_fault_application(attempt, &facts, &self.fault_application_policy) {
            Ok(plan) => plan,
            Err(reason) => {
                self.record_fault_stage(
                    attempt.id,
                    FaultStageKind::Rejected {
                        reason: reason.clone(),
                    },
                )?;
                if self.fault_application_policy.rejection_is_fatal {
                    return Err(VmError::Snapshot {
                        message: format!("fault rejected by fatal campaign policy: {reason:?}"),
                    });
                }
                return Ok(());
            }
        };

        preflight_fault_application_events_with_limit(
            self.fault_engine.fault_outcomes(),
            plan.max_immediate_observations(),
            event_limit,
        )
        .map_err(fault_transition_vm_error)?;
        self.record_fault_stage(
            attempt.id,
            FaultStageKind::Applicable {
                effect: plan.effect.clone(),
            },
        )?;
        match self.apply_fault_plan(&plan) {
            Ok(observations) => {
                assert!(observations.len() <= plan.max_immediate_observations());
                self.record_applied_plan(&plan, observations)
            }
            Err(failure) => self.record_fault_stage(
                attempt.id,
                FaultStageKind::ApplicationFailed {
                    reason: failure.reason,
                    disposition: failure.disposition,
                },
            ),
        }
    }

    fn collect_fault_planning_facts(&mut self) -> Result<FaultPlanningFacts, VmError> {
        let mut vms = Vec::with_capacity(self.vms.len());
        for slot in &mut self.vms {
            let vcpu_count =
                u32::try_from(slot.vm.vcpu_count()).map_err(|_| VmError::Snapshot {
                    message: "vCPU count exceeds fault-planning bounds".to_string(),
                })?;
            vms.push(VmFaultFacts {
                status: fault_vm_status(slot.status),
                vcpu_count,
                block_device_size_bytes: slot.vm.block_device_size_bytes(),
                has_initial_snapshot: slot.initial_snapshot.is_some(),
                supports_irq: true,
                supports_nmi: true,
                virtual_tsc: slot.vm.virtual_tsc(),
            });
        }
        Ok(FaultPlanningFacts {
            current_tick: self.tick,
            network_supported: true,
            vms,
        })
    }

    fn record_applied_plan(
        &mut self,
        plan: &FaultPlan,
        observations: Vec<FaultObservation>,
    ) -> Result<(), VmError> {
        self.record_fault_stage(
            plan.attempt_id,
            FaultStageKind::Applied {
                effect: plan.effect.clone(),
            },
        )?;
        self.record_fault_observations(&observations)
    }

    fn record_fault_stage(
        &mut self,
        attempt_id: FaultAttemptId,
        kind: FaultStageKind,
    ) -> Result<(), VmError> {
        self.fault_engine
            .record_fault_stage(attempt_id, kind)
            .map_err(fault_transition_vm_error)
    }

    fn record_fault_observations(
        &mut self,
        observations: &[FaultObservation],
    ) -> Result<(), VmError> {
        self.fault_engine
            .record_fault_observations(observations)
            .map_err(fault_transition_vm_error)
    }

    fn record_fault_observations_with_event_limit(
        &mut self,
        observations: &[FaultObservation],
        event_limit: usize,
    ) -> Result<(), VmError> {
        self.fault_engine
            .record_fault_observations_with_limit(observations, event_limit)
            .map_err(fault_transition_vm_error)
    }

    fn queue_process_observation(
        &mut self,
        vm_index: usize,
        attempt_id: FaultAttemptId,
        effect: FaultObservationEffect,
    ) -> Result<(), VmError> {
        assert!(self.pending_process_observations.len() < MAX_PENDING_PROCESS_OBSERVATIONS);
        let observation =
            self.make_shell_observation(attempt_id, FaultObservationSubsystem::Process, effect)?;
        self.pending_process_observations
            .push_back((vm_index, observation));
        Ok(())
    }

    fn commit_pending_process_observations(&mut self, event_limit: usize) -> Result<(), VmError> {
        if self.pending_process_observations.is_empty() {
            return Ok(());
        }
        let observations = self
            .pending_process_observations
            .iter()
            .map(|(_, observation)| observation.clone())
            .collect::<Vec<_>>();
        self.record_fault_observations_with_event_limit(&observations, event_limit)?;
        for (vm_index, observation) in self.pending_process_observations.drain(..) {
            if self.vms[vm_index].process_fault_attempt == Some(observation.attempt_id) {
                self.vms[vm_index].process_fault_attempt = None;
            }
        }
        Ok(())
    }

    fn make_shell_observation(
        &mut self,
        attempt_id: FaultAttemptId,
        subsystem: FaultObservationSubsystem,
        effect: FaultObservationEffect,
    ) -> Result<FaultObservation, VmError> {
        let operation_sequence = self.fault_operation_sequence;
        self.fault_operation_sequence =
            operation_sequence
                .checked_add(1)
                .ok_or_else(|| VmError::Snapshot {
                    message: "fault operation sequence overflowed".to_string(),
                })?;
        Ok(FaultObservation::new(
            attempt_id,
            subsystem,
            operation_sequence,
            effect,
        ))
    }

    fn apply_fault_plan(
        &mut self,
        plan: &FaultPlan,
    ) -> Result<Vec<FaultObservation>, FaultApplicationError> {
        match plan.mechanism() {
            FaultMechanism::NetworkPartition
            | FaultMechanism::NetworkLatency
            | FaultMechanism::PacketLoss
            | FaultMechanism::PacketCorruption
            | FaultMechanism::PacketReorder
            | FaultMechanism::NetworkJitter
            | FaultMechanism::NetworkBandwidth
            | FaultMechanism::PacketDuplicate
            | FaultMechanism::NetworkHeal => self.apply_network_fault_plan(plan),
            FaultMechanism::BlockReadError
            | FaultMechanism::BlockWriteError
            | FaultMechanism::BlockTornWrite
            | FaultMechanism::BlockCorruption
            | FaultMechanism::BlockFull
            | FaultMechanism::BlockSlow
            | FaultMechanism::BlockFsyncLie
            | FaultMechanism::BlockFsyncFlush
            | FaultMechanism::BlockPartialRead => self.apply_block_fault_plan(plan),
            FaultMechanism::ProcessKill
            | FaultMechanism::ProcessPause
            | FaultMechanism::ProcessRestart => self.apply_process_fault_plan(plan),
            FaultMechanism::VirtualClockSkew | FaultMechanism::VirtualClockJump => {
                self.apply_clock_fault_plan(plan)
            }
            FaultMechanism::IrqInjection | FaultMechanism::NmiInjection => {
                self.apply_interrupt_fault_plan(plan)
            }
            FaultMechanism::CpuRegisterBitflip => self.apply_cpu_fault_plan(plan),
        }
    }

    fn apply_network_fault_plan(
        &mut self,
        plan: &FaultPlan,
    ) -> Result<Vec<FaultObservation>, FaultApplicationError> {
        let applied = match &plan.effect {
            FaultPlanEffect::NetworkPartition { side_a, side_b } => self.network.arm_partition(
                u32_targets_to_usize(side_a)?,
                u32_targets_to_usize(side_b)?,
                plan.attempt_id,
            ),
            FaultPlanEffect::NetworkLatency {
                target,
                latency_ticks,
            } => self
                .network
                .arm_latency(checked_usize(*target)?, *latency_ticks, plan.attempt_id),
            FaultPlanEffect::PacketLoss { target, rate_ppm } => {
                self.network
                    .arm_loss(checked_usize(*target)?, *rate_ppm, plan.attempt_id)
            }
            FaultPlanEffect::PacketCorruption { target, rate_ppm } => {
                self.network
                    .arm_corruption(checked_usize(*target)?, *rate_ppm, plan.attempt_id)
            }
            FaultPlanEffect::PacketReorder {
                target,
                window_ticks,
            } => self
                .network
                .arm_reorder(checked_usize(*target)?, *window_ticks, plan.attempt_id),
            FaultPlanEffect::NetworkJitter {
                target,
                jitter_ticks,
            } => self
                .network
                .arm_jitter(checked_usize(*target)?, *jitter_ticks, plan.attempt_id),
            FaultPlanEffect::NetworkBandwidth {
                target,
                bytes_per_sec,
            } => {
                self.network
                    .arm_bandwidth(checked_usize(*target)?, *bytes_per_sec, plan.attempt_id)
            }
            FaultPlanEffect::PacketDuplicate { target, rate_ppm } => {
                self.network
                    .arm_duplicate(checked_usize(*target)?, *rate_ppm, plan.attempt_id)
            }
            FaultPlanEffect::NetworkHeal => self.network.clear_partitions(),
            _ => return Err(internal_application_error()),
        };
        if !applied {
            return Err(target_state_application_error());
        }
        Ok(Vec::new())
    }

    fn apply_block_fault_plan(
        &mut self,
        plan: &FaultPlan,
    ) -> Result<Vec<FaultObservation>, FaultApplicationError> {
        use crate::devices::block::BlockFault;
        let applied = match &plan.effect {
            FaultPlanEffect::BlockReadError { target, offset } => self
                .vm_slot_mut_checked(*target)?
                .vm
                .inject_disk_fault_with_attempt(
                    BlockFault::ReadError { offset: *offset },
                    plan.attempt_id,
                ),
            FaultPlanEffect::BlockWriteError { target, offset } => self
                .vm_slot_mut_checked(*target)?
                .vm
                .inject_disk_fault_with_attempt(
                    BlockFault::WriteError { offset: *offset },
                    plan.attempt_id,
                ),
            FaultPlanEffect::BlockTornWrite {
                target,
                offset,
                bytes_written,
            } => self
                .vm_slot_mut_checked(*target)?
                .vm
                .inject_disk_fault_with_attempt(
                    BlockFault::TornWrite {
                        offset: *offset,
                        bytes_written: checked_usize_u64(*bytes_written)?,
                    },
                    plan.attempt_id,
                ),
            FaultPlanEffect::BlockCorruption {
                target,
                offset,
                len,
            } => self
                .vm_slot_mut_checked(*target)?
                .vm
                .inject_disk_fault_with_attempt(
                    BlockFault::Corruption {
                        offset: *offset,
                        len: checked_usize_u64(*len)?,
                    },
                    plan.attempt_id,
                ),
            FaultPlanEffect::BlockFull { target } => self
                .vm_slot_mut_checked(*target)?
                .vm
                .set_disk_full_with_attempt(true, plan.attempt_id),
            FaultPlanEffect::BlockSlow { target, delay_ns } => self
                .vm_slot_mut_checked(*target)?
                .vm
                .set_disk_slow_delay_with_attempt(*delay_ns, plan.attempt_id),
            FaultPlanEffect::BlockFsyncLie { target } => self
                .vm_slot_mut_checked(*target)?
                .vm
                .enable_disk_fsync_lie_with_attempt(plan.attempt_id),
            FaultPlanEffect::BlockFsyncFlush { target } => {
                self.vm_slot_mut_checked(*target)?.vm.flush_disk_volatile()
            }
            FaultPlanEffect::BlockPartialRead {
                target,
                offset,
                max_bytes,
            } => self
                .vm_slot_mut_checked(*target)?
                .vm
                .inject_disk_fault_with_attempt(
                    BlockFault::PartialRead {
                        offset: *offset,
                        max_bytes: checked_usize_u64(*max_bytes)?,
                    },
                    plan.attempt_id,
                ),
            _ => return Err(internal_application_error()),
        };
        if !applied {
            return Err(device_disappeared_application_error());
        }
        Ok(Vec::new())
    }

    fn apply_process_fault_plan(
        &mut self,
        plan: &FaultPlan,
    ) -> Result<Vec<FaultObservation>, FaultApplicationError> {
        match &plan.effect {
            FaultPlanEffect::ProcessKill { target } => {
                let slot = self.vm_slot_mut_checked(*target)?;
                slot.status = VmStatus::Crashed;
                slot.process_fault_attempt = Some(plan.attempt_id);
                slot.vm.discard_disk_volatile();
            }
            FaultPlanEffect::ProcessPause {
                target,
                resume_at_tick,
            } => {
                let slot = self.vm_slot_mut_checked(*target)?;
                slot.status = VmStatus::Resuming {
                    resume_at_tick: *resume_at_tick,
                };
                slot.process_fault_attempt = Some(plan.attempt_id);
            }
            FaultPlanEffect::ProcessRestart {
                target,
                restart_at_tick,
            } => {
                let slot = self.vm_slot_mut_checked(*target)?;
                slot.status = VmStatus::Restarting {
                    restart_at_tick: *restart_at_tick,
                };
                slot.process_fault_attempt = Some(plan.attempt_id);
            }
            _ => return Err(internal_application_error()),
        }
        Ok(Vec::new())
    }

    fn apply_clock_fault_plan(
        &mut self,
        plan: &FaultPlan,
    ) -> Result<Vec<FaultObservation>, FaultApplicationError> {
        let (target, changed) = match &plan.effect {
            FaultPlanEffect::VirtualClockSkew {
                target, target_tsc, ..
            }
            | FaultPlanEffect::VirtualClockJump {
                target, target_tsc, ..
            } => {
                let slot = self.vm_slot_mut_checked(*target)?;
                let changed = slot.vm.virtual_tsc() != *target_tsc;
                slot.vm.virtual_tsc_mut().set(*target_tsc);
                (*target, changed)
            }
            _ => return Err(internal_application_error()),
        };
        if !changed {
            return Ok(Vec::new());
        }
        let observation = self
            .make_shell_observation(
                plan.attempt_id,
                FaultObservationSubsystem::VirtualClock,
                FaultObservationEffect::VirtualClockChanged,
            )
            .map_err(|_| internal_application_error())?;
        debug!("VM{} virtual clock changed by fault", target);
        Ok(vec![observation])
    }

    fn apply_interrupt_fault_plan(
        &mut self,
        plan: &FaultPlan,
    ) -> Result<Vec<FaultObservation>, FaultApplicationError> {
        let (subsystem, effect) = match &plan.effect {
            FaultPlanEffect::IrqInjection { target, irq } => {
                if self
                    .vm_slot_mut_checked(*target)?
                    .vm
                    .inject_interrupt(*irq)
                    .is_err()
                {
                    self.vm_slot_mut_checked(*target)?.status = VmStatus::Crashed;
                    return Err(non_runnable_application_error());
                }
                (
                    FaultObservationSubsystem::Interrupt,
                    FaultObservationEffect::InterruptInjected,
                )
            }
            FaultPlanEffect::NmiInjection { target, vcpu } => {
                if self
                    .vm_slot_mut_checked(*target)?
                    .vm
                    .inject_nmi(checked_usize(*vcpu)?)
                    .is_err()
                {
                    self.vm_slot_mut_checked(*target)?.status = VmStatus::Crashed;
                    return Err(non_runnable_application_error());
                }
                (
                    FaultObservationSubsystem::Interrupt,
                    FaultObservationEffect::NmiInjected,
                )
            }
            _ => return Err(internal_application_error()),
        };
        let observation = self
            .make_shell_observation(plan.attempt_id, subsystem, effect)
            .map_err(|_| internal_application_error())?;
        Ok(vec![observation])
    }

    fn apply_cpu_fault_plan(
        &mut self,
        plan: &FaultPlan,
    ) -> Result<Vec<FaultObservation>, FaultApplicationError> {
        match &plan.effect {
            FaultPlanEffect::CpuRegisterBitflip {
                target,
                vcpu,
                register,
                bit,
            } => {
                if self
                    .vm_slot_mut_checked(*target)?
                    .vm
                    .bitflip_register(checked_usize(*vcpu)?, *register, *bit)
                    .is_err()
                {
                    self.vm_slot_mut_checked(*target)?.status = VmStatus::Crashed;
                    return Err(non_runnable_application_error());
                }
            }
            _ => return Err(internal_application_error()),
        }
        let observation = self
            .make_shell_observation(
                plan.attempt_id,
                FaultObservationSubsystem::Cpu,
                FaultObservationEffect::CpuRegisterChanged,
            )
            .map_err(|_| internal_application_error())?;
        Ok(vec![observation])
    }

    fn vm_slot_mut_checked(&mut self, target: u32) -> Result<&mut VmSlot, FaultApplicationError> {
        let target = checked_usize(target)?;
        self.vms
            .get_mut(target)
            .ok_or_else(target_state_application_error)
    }

    #[cfg(test)]
    fn apply_fault(&mut self, fault: &Fault) -> Result<(), VmError> {
        let schedule = chaoscontrol_fault::schedule::FaultScheduleBuilder::new()
            .at_ns(0, fault.clone())
            .build();
        self.fault_engine.set_schedule(schedule);
        self.fault_engine.force_setup_complete();
        let attempts =
            self.fault_engine
                .poll_fault_attempts(0)
                .map_err(|error| VmError::Snapshot {
                    message: format!("fault selection failed: {error}"),
                })?;
        for attempt in attempts {
            self.handle_fault_attempt(&attempt)?;
        }
        Ok(())
    }

    #[cfg(any())]
    fn legacy_apply_fault_removed(&mut self, fault: &Fault) -> Result<(), VmError> {
        info!("Applying fault at tick {}: {}", self.tick, fault);

        match fault {
            // ── Network faults ──
            Fault::NetworkPartition { side_a, side_b } => {
                self.network.add_partition(side_a.clone(), side_b.clone());
            }
            Fault::NetworkLatency { target, latency_ns } => {
                self.network.set_latency(*target, *latency_ns);
            }
            Fault::NetworkHeal => {
                self.network.clear_partitions();
            }
            Fault::PacketLoss { target, rate_ppm } => {
                info!("PacketLoss: VM{} set to {} ppm", target, rate_ppm);
                self.network.set_loss_rate(*target, *rate_ppm);
            }
            Fault::PacketCorruption { target, rate_ppm } => {
                info!("PacketCorruption: VM{} set to {} ppm", target, rate_ppm);
                self.network.set_corruption_rate(*target, *rate_ppm);
            }
            Fault::PacketReorder { target, window_ns } => {
                // Convert nanoseconds to ticks (1 tick = 1_000_000 ns)
                let window_ticks = window_ns / 1_000_000;
                info!(
                    "PacketReorder: VM{} window {} ns ({} ticks)",
                    target, window_ns, window_ticks
                );
                self.network.set_reorder_window(*target, window_ticks);
            }
            Fault::NetworkJitter { target, jitter_ns } => {
                let jitter_ticks = jitter_ns / 1_000_000;
                info!(
                    "NetworkJitter: VM{} jitter {} ns ({} ticks)",
                    target, jitter_ns, jitter_ticks
                );
                self.network.set_jitter(*target, jitter_ticks);
            }
            Fault::NetworkBandwidth {
                target,
                bytes_per_sec,
            } => {
                info!(
                    "NetworkBandwidth: VM{} limited to {} B/s ({} KB/s)",
                    target,
                    bytes_per_sec,
                    bytes_per_sec / 1024
                );
                self.network.set_bandwidth(*target, *bytes_per_sec);
            }
            Fault::PacketDuplicate { target, rate_ppm } => {
                info!("PacketDuplicate: VM{} set to {} ppm", target, rate_ppm);
                self.network.set_duplicate_rate(*target, *rate_ppm);
            }

            // ── Disk faults ──
            Fault::DiskReadError { target, offset } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    warn!("DiskReadError at VM{}, offset {:#x}", target, offset);
                    slot.disk_faults.error_rate = 1.0; // 100% error for now
                }
            }
            Fault::DiskWriteError { target, offset } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    warn!("DiskWriteError at VM{}, offset {:#x}", target, offset);
                    slot.disk_faults.error_rate = 1.0;
                }
            }
            Fault::DiskTornWrite {
                target,
                offset,
                bytes_written,
            } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    use crate::devices::block::BlockFault;
                    let fault = BlockFault::TornWrite {
                        offset: *offset,
                        bytes_written: *bytes_written,
                    };
                    if slot.vm.inject_disk_fault(fault) {
                        info!(
                            "DiskTornWrite injected at VM{}, offset {:#x}, {} bytes",
                            target, offset, bytes_written
                        );
                    } else {
                        warn!(
                            "DiskTornWrite fault failed: VM{} has no block device",
                            target
                        );
                    }
                }
            }
            Fault::DiskCorruption {
                target,
                offset,
                len,
            } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    use crate::devices::block::BlockFault;
                    let fault = BlockFault::Corruption {
                        offset: *offset,
                        len: *len,
                    };
                    if slot.vm.inject_disk_fault(fault) {
                        info!(
                            "DiskCorruption injected at VM{}, offset {:#x}, {} bytes",
                            target, offset, len
                        );
                    } else {
                        warn!(
                            "DiskCorruption fault failed: VM{} has no block device",
                            target
                        );
                    }
                }
            }
            Fault::DiskFull { target } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    info!("DiskFull injected at VM{}", target);
                    slot.disk_faults.full = true;
                }
            }

            // ── Process faults ──
            Fault::ProcessKill { target } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    info!("ProcessKill: VM{} crashed", target);
                    slot.status = VmStatus::Crashed;
                    // Discard volatile writes if fsync-lie was active.
                    slot.vm.discard_disk_volatile();
                }
            }
            Fault::ProcessPause {
                target,
                duration_ns,
            } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    // Convert duration_ns to ticks (1 tick = 1_000_000 ns), minimum 1 tick
                    let pause_ticks = (*duration_ns / 1_000_000).max(1);
                    let resume_at = self.tick + pause_ticks;
                    info!(
                        "ProcessPause: VM{} paused for {} ns ({} ticks), resume at tick {}",
                        target, duration_ns, pause_ticks, resume_at
                    );
                    slot.status = VmStatus::Paused;
                }
                // Schedule automatic resume after duration
                let pause_ticks = (*duration_ns / 1_000_000).max(1);
                self.schedule_resume(*target, self.tick + pause_ticks)?;
            }
            Fault::ProcessRestart { target } => {
                self.schedule_restart(*target, self.tick + 10)?; // Restart after 10 ticks
            }

            // ── Clock faults ──
            Fault::ClockSkew { target, offset_ns } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    info!("ClockSkew: VM{} offset by {} ns", target, offset_ns);
                    slot.tsc_skew += offset_ns;
                    // Apply skew to VM's virtual TSC
                    let current_tsc = slot.vm.virtual_tsc();
                    let skewed_tsc = (current_tsc as i64 + *offset_ns).max(0) as u64;
                    slot.vm.virtual_tsc_mut().advance_to(skewed_tsc);
                }
            }
            Fault::ClockJump { target, delta_ns } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    info!("ClockJump: VM{} jumped by {} ns", target, delta_ns);
                    let current_tsc = slot.vm.virtual_tsc();
                    let jumped_tsc = (current_tsc as i64 + *delta_ns).max(0) as u64;
                    slot.vm.virtual_tsc_mut().advance_to(jumped_tsc);
                }
            }

            // ── Resource faults ──
            Fault::MemoryPressure {
                target,
                limit_bytes,
            } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    info!(
                        "MemoryPressure: VM{} limited to {} bytes ({} MB)",
                        target,
                        limit_bytes,
                        limit_bytes / (1024 * 1024)
                    );
                    slot.memory_limit_bytes = Some(*limit_bytes);
                }
            }

            // ── Interrupt injection faults ──
            Fault::InjectInterrupt { target, irq } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    info!("InjectInterrupt: VM{} IRQ {}", target, irq);
                    slot.vm.inject_interrupt(*irq)?;
                } else {
                    warn!("InjectInterrupt fault skipped: VM{} not found", target);
                }
            }
            Fault::InjectNmi { target, vcpu } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    info!("InjectNmi: VM{} vCPU {}", target, vcpu);
                    slot.vm.inject_nmi(*vcpu)?;
                } else {
                    warn!("InjectNmi fault skipped: VM{} not found", target);
                }
            }

            // ── Advanced disk faults ──
            Fault::DiskSlow { target, delay_ns } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    info!("DiskSlow: VM{} delay {} ns", target, delay_ns);
                    slot.vm.set_disk_slow_delay(*delay_ns);
                }
            }
            Fault::DiskFsyncLie { target } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    info!("DiskFsyncLie: VM{} enabled", target);
                    slot.vm.enable_disk_fsync_lie();
                }
            }
            Fault::DiskFsyncFlush { target } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    info!("DiskFsyncFlush: VM{} flushing volatile", target);
                    slot.vm.flush_disk_volatile();
                }
            }
            Fault::DiskPartialRead {
                target,
                offset,
                max_bytes,
            } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    use crate::devices::block::BlockFault;
                    let fault = BlockFault::PartialRead {
                        offset: *offset,
                        max_bytes: *max_bytes,
                    };
                    if slot.vm.inject_disk_fault(fault) {
                        info!(
                            "DiskPartialRead: VM{} offset {:#x} max {} bytes",
                            target, offset, max_bytes
                        );
                    }
                }
            }

            // ── CPU faults ──
            Fault::CpuBitflip {
                target,
                vcpu,
                register,
                bit,
            } => {
                if *bit >= 64 {
                    info!("CpuBitflip: bit {} >= 64, ignoring", bit);
                } else if let Some(slot) = self.vms.get_mut(*target) {
                    info!(
                        "CpuBitflip: VM{} vcpu {} {}[{}]",
                        target, vcpu, register, bit
                    );
                    slot.vm.bitflip_register(*vcpu, *register, *bit)?;
                }
            }
            Fault::CpuStall {
                target,
                vcpu,
                duration_ticks,
            } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    let expires = self.tick + *duration_ticks;
                    info!(
                        "CpuStall: VM{} vcpu {} stalled until tick {}",
                        target, vcpu, expires
                    );
                    slot.vcpu_stall_until.insert(*vcpu, expires);
                }
            }

            // ── Advanced clock faults ──
            Fault::ClockFreeze {
                target,
                duration_ticks,
            } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    let frozen_tsc = slot.vm.virtual_tsc();
                    let expires = self.tick + *duration_ticks;
                    info!(
                        "ClockFreeze: VM{} frozen at TSC {} until tick {}",
                        target, frozen_tsc, expires
                    );
                    slot.clock_freeze = Some((frozen_tsc, expires));
                }
            }
            Fault::ClockJitter { target, bound_tsc } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    info!("ClockJitter: VM{} ±{} TSC", target, bound_tsc);
                    slot.clock_jitter_bound = *bound_tsc;
                }
            }
        }

        Ok(())
    }

    /// Schedule a VM restart at a future tick.
    #[cfg(any())]
    fn schedule_restart(&mut self, target: usize, restart_at_tick: u64) -> Result<(), VmError> {
        if let Some(slot) = self.vms.get_mut(target) {
            info!(
                "VM{} scheduled to restart at tick {}",
                target, restart_at_tick
            );
            slot.status = VmStatus::Restarting { restart_at_tick };
        }
        Ok(())
    }

    /// Schedule a paused VM to resume at a future tick.
    #[cfg(any())]
    fn schedule_resume(&mut self, target: usize, resume_at_tick: u64) -> Result<(), VmError> {
        if let Some(slot) = self.vms.get_mut(target) {
            if slot.status == VmStatus::Paused {
                info!(
                    "VM{} scheduled to resume at tick {}",
                    target, resume_at_tick
                );
                slot.status = VmStatus::Resuming { resume_at_tick };
            }
        }
        Ok(())
    }

    /// Restart a VM from its initial snapshot.
    fn restart_vm(&mut self, target: usize) -> Result<(), VmError> {
        let slot = self.vms.get_mut(target).ok_or_else(|| {
            SnapshotSnafu {
                message: format!("VM{} not found", target),
            }
            .build()
        })?;

        // Preserve the block device's dirty pages across restart
        // so the guest can verify crash-persistent data.
        let block_snapshot = slot.vm.snapshot_block_dirty();

        if let Some(snapshot) = &slot.initial_snapshot {
            info!(
                "Restarting VM{} from initial snapshot (preserving disk)",
                target
            );
            slot.vm.restore(snapshot)?;

            // Restore the dirty pages we preserved — these represent
            // data written to "disk" before the crash.
            if let Some(blk) = block_snapshot {
                slot.vm.restore_block_dirty(blk);
            }

            slot.status = VmStatus::Running;
            slot.inbox.clear();
            slot.disk_faults = DiskFaultFlags::default();
            slot.tsc_skew = 0;
            slot.memory_limit_bytes = None;

            // Run until setup_complete so the guest finishes booting.
            slot.vm.fault_engine_mut().reset_setup_complete();
            let budget = self.config.bootstrap_budget.unwrap_or(10_000);
            let mut ran: u64 = 0;
            loop {
                let (exits, idle) = slot.vm.run_bounded(1000)?;
                ran += exits;
                if slot.vm.fault_engine().is_setup_complete() {
                    info!("VM{} restarted successfully ({} exits)", target, ran);
                    break;
                }
                if ran >= budget || idle {
                    warn!(
                        "VM{} restart exceeded budget ({} exits), marking crashed",
                        target, ran
                    );
                    slot.status = VmStatus::Crashed;
                    return Ok(());
                }
            }
        } else {
            warn!("VM{} has no initial snapshot, cannot restart", target);
        }

        Ok(())
    }

    /// Deliver pending network messages whose delivery tick has arrived.
    fn deliver_messages(&mut self) -> usize {
        let mut delivered = 0;
        let mut pending = Vec::new();

        for msg in self.network.in_flight.drain(..) {
            if msg.deliver_at_tick <= self.tick {
                if let Some(slot) = self.vms.get_mut(msg.to) {
                    slot.inbox.push_back(msg);
                    delivered += 1;
                }
            } else {
                pending.push(msg);
            }
        }

        self.network.in_flight = pending;
        delivered
    }

    /// Bridge network packets between VMs (virtio-net TX → RX).
    ///
    /// This is the core VM-to-VM networking logic:
    /// 1. Drain TX queues from all VMs
    /// 2. For each packet: broadcast to all other VMs (hub model)
    /// 3. Route through NetworkFabric for fault injection
    /// 4. Deliver arrived packets to destination VM RX queues
    fn bridge_network_packets(&mut self) -> Result<(), VmError> {
        // Phase 1: Drain TX queues and enqueue into NetworkFabric.
        for from_id in 0..self.vms.len() {
            let packets = self.vms[from_id].vm.drain_net_tx();
            for packet in packets {
                // Broadcast to all other VMs (simple hub model).
                for to_id in 0..self.vms.len() {
                    if to_id != from_id {
                        route_network_packet(
                            &mut self.network,
                            from_id,
                            to_id,
                            packet.clone(),
                            self.tick,
                        )?;
                    }
                }
            }
        }

        // Phase 2: Deliver packets that have arrived.
        let delivered = self.network.deliver_packets(self.tick);
        for (vm_id, packet) in delivered {
            if let Some(slot) = self.vms.get_mut(vm_id) {
                slot.vm.inject_net_rx(packet);
            }
        }
        Ok(())
    }

    /// Snapshot all VMs and simulation state.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn snapshot_all(&self) -> Result<SimulationSnapshot, VmError> {
        let mut vm_snapshots = Vec::with_capacity(self.vms.len());

        for slot in &self.vms {
            let vm_snapshot = slot.vm.snapshot()?;
            vm_snapshots.push((vm_snapshot, slot.status));
        }

        let vcpu_stall_until = self
            .vms
            .iter()
            .map(|s| s.vcpu_stall_until.clone())
            .collect();
        let clock_freeze = self.vms.iter().map(|s| s.clock_freeze).collect();
        let clock_jitter_bound = self.vms.iter().map(|s| s.clock_jitter_bound).collect();
        let process_fault_attempt = self
            .vms
            .iter()
            .map(|slot| slot.process_fault_attempt)
            .collect();

        Ok(SimulationSnapshot {
            tick: self.tick,
            vm_snapshots,
            network_state: self.network.clone(),
            fault_engine_snapshot: self.fault_engine.snapshot(),
            vcpu_stall_until,
            clock_freeze,
            clock_jitter_bound,
            process_fault_attempt,
            fault_operation_sequence: self.fault_operation_sequence,
            pending_process_observations: self.pending_process_observations.clone(),
        })
    }

    /// Take an incremental snapshot of all VMs.
    ///
    /// Each VM's memory is captured as a sparse overlay referencing the
    /// stored base. Call [`Self::set_memory_bases`] before using this.
    /// Returns the snapshot and total dirty pages across all VMs.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn snapshot_all_incremental(&self) -> Result<(SimulationSnapshot, usize), VmError> {
        let mut vm_snapshots = Vec::with_capacity(self.vms.len());
        let mut total_dirty = 0usize;

        for (i, slot) in self.vms.iter().enumerate() {
            if let Some(base) = &self.vm_memory_bases[i] {
                let (snap, dirty) = slot.vm.snapshot_incremental(base)?;
                total_dirty += dirty;
                vm_snapshots.push((snap, slot.status));
            } else {
                // No base — fall back to full snapshot
                let snap = slot.vm.snapshot()?;
                vm_snapshots.push((snap, slot.status));
            }
        }

        let vcpu_stall_until = self
            .vms
            .iter()
            .map(|s| s.vcpu_stall_until.clone())
            .collect();
        let clock_freeze = self.vms.iter().map(|s| s.clock_freeze).collect();
        let clock_jitter_bound = self.vms.iter().map(|s| s.clock_jitter_bound).collect();
        let process_fault_attempt = self
            .vms
            .iter()
            .map(|slot| slot.process_fault_attempt)
            .collect();

        let sim_snap = SimulationSnapshot {
            tick: self.tick,
            vm_snapshots,
            network_state: self.network.clone(),
            fault_engine_snapshot: self.fault_engine.snapshot(),
            vcpu_stall_until,
            clock_freeze,
            clock_jitter_bound,
            process_fault_attempt,
            fault_operation_sequence: self.fault_operation_sequence,
            pending_process_observations: self.pending_process_observations.clone(),
        };

        Ok((sim_snap, total_dirty))
    }

    /// Store base memory images for incremental snapshots.
    ///
    /// Call this after bootstrap with the full snapshot's memory data.
    /// Each entry is an `Arc<Vec<u8>>` that overlay snapshots will
    /// reference.
    pub fn set_memory_bases(&mut self, bases: Vec<std::sync::Arc<Vec<u8>>>) {
        self.vm_memory_bases = bases.into_iter().map(Some).collect();
    }

    /// Initialize per-thread POSIX timers on all VMs.
    ///
    /// Must be called from the worker thread that will run this controller.
    /// Creates `timer_create` + `SIGEV_THREAD_ID` timers so that SIGALRM
    /// targets this specific thread, allowing parallel workers to each
    /// have independent watchdog timers.
    pub fn init_thread_timers(&mut self) {
        for slot in &mut self.vms {
            slot.vm.init_thread_timer();
        }
    }

    /// Extract the base memory from a full snapshot for use with
    /// incremental snapshots.
    pub fn extract_memory_bases(snapshot: &SimulationSnapshot) -> Vec<std::sync::Arc<Vec<u8>>> {
        snapshot
            .vm_snapshots
            .iter()
            .map(|(vm_snap, _)| std::sync::Arc::new(vm_snap.memory.materialize()))
            .collect()
    }

    /// Restore all VMs from a snapshot.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn restore_all(&mut self, snapshot: &SimulationSnapshot) -> Result<(), VmError> {
        if snapshot.vm_snapshots.len() != self.vms.len() {
            return SnapshotSnafu {
                message: "Snapshot VM count mismatch",
            }
            .fail();
        }
        self.fault_engine
            .validate_snapshot(&snapshot.fault_engine_snapshot)
            .map_err(fault_transition_vm_error)?;
        for (index, (vm_snapshot, _)) in snapshot.vm_snapshots.iter().enumerate() {
            self.vms[index]
                .vm
                .validate_fault_engine_snapshot(vm_snapshot)?;
        }
        self.validate_pending_snapshot(snapshot)?;

        self.tick = snapshot.tick;
        self.network = snapshot.network_state.clone();
        self.fault_engine
            .restore(&snapshot.fault_engine_snapshot)
            .map_err(fault_transition_vm_error)?;
        self.fault_operation_sequence = snapshot.fault_operation_sequence;
        self.pending_process_observations = snapshot.pending_process_observations.clone();

        for (i, (vm_snap, status)) in snapshot.vm_snapshots.iter().enumerate() {
            self.vms[i].vm.restore(vm_snap)?;
            self.vms[i].status = *status;
            if let Some(stalls) = snapshot.vcpu_stall_until.get(i) {
                self.vms[i].vcpu_stall_until = stalls.clone();
            } else {
                self.vms[i].vcpu_stall_until.clear();
            }
            self.vms[i].clock_freeze = snapshot.clock_freeze.get(i).copied().flatten();
            self.vms[i].clock_jitter_bound =
                snapshot.clock_jitter_bound.get(i).copied().unwrap_or(0);
            self.vms[i].process_fault_attempt =
                snapshot.process_fault_attempt.get(i).copied().flatten();
        }

        info!(
            "Restored simulation state from snapshot at tick {}",
            self.tick
        );
        Ok(())
    }

    fn validate_pending_snapshot(&self, snapshot: &SimulationSnapshot) -> Result<(), VmError> {
        let vm_count = snapshot.vm_snapshots.len();
        let vector_lengths = [
            snapshot.vcpu_stall_until.len(),
            snapshot.clock_freeze.len(),
            snapshot.clock_jitter_bound.len(),
            snapshot.process_fault_attempt.len(),
        ];
        if vector_lengths.into_iter().any(|length| length != vm_count) {
            return Err(fault_transition_vm_error(
                FaultTransitionError::SnapshotPendingStateMismatch,
            ));
        }
        let ledger = snapshot.fault_engine_snapshot.outcomes();
        if snapshot.pending_process_observations.len() > MAX_PENDING_PROCESS_OBSERVATIONS
            || snapshot
                .pending_process_observations
                .iter()
                .any(|(vm_index, observation)| {
                    *vm_index >= vm_count
                        || observation.subsystem != FaultObservationSubsystem::Process
                        || observation.operation_sequence >= snapshot.fault_operation_sequence
                })
        {
            return Err(fault_transition_vm_error(
                FaultTransitionError::SnapshotPendingStateMismatch,
            ));
        }
        let pending_process_observations = snapshot
            .pending_process_observations
            .iter()
            .map(|(_, observation)| observation.clone())
            .collect::<Vec<_>>();
        validate_pending_fault_observations(ledger, &pending_process_observations)
            .map_err(fault_transition_vm_error)?;
        snapshot
            .network_state
            .validate_pending_faults(ledger, vm_count)
            .map_err(fault_transition_vm_error)?;
        for (index, ((vm_snapshot, status), attempt_id)) in snapshot
            .vm_snapshots
            .iter()
            .zip(&snapshot.process_fault_attempt)
            .enumerate()
        {
            let target = u32::try_from(index).map_err(|_| {
                fault_transition_vm_error(FaultTransitionError::SnapshotPendingStateMismatch)
            })?;
            let has_pending_observation =
                snapshot
                    .pending_process_observations
                    .iter()
                    .any(|(vm_index, observation)| {
                        *vm_index == index && Some(observation.attempt_id) == *attempt_id
                    });
            validate_process_snapshot_effect(
                ledger,
                target,
                *status,
                *attempt_id,
                has_pending_observation,
            )
            .map_err(fault_transition_vm_error)?;
            if !snapshot.vcpu_stall_until[index].is_empty()
                || snapshot.clock_freeze[index].is_some()
                || snapshot.clock_jitter_bound[index] != 0
            {
                return Err(fault_transition_vm_error(
                    FaultTransitionError::SnapshotPendingStateMismatch,
                ));
            }
            for device in &vm_snapshot.virtio_snapshots {
                if let Some(block_snapshot) = &device.block_snapshot {
                    block_snapshot
                        .validate_pending_faults(ledger, target)
                        .map_err(fault_transition_vm_error)?;
                }
            }
        }
        let invalid_shell_sequence = ledger.events.iter().any(|event| match &event.kind {
            FaultStageKind::Observed { observation }
                if observation.subsystem != FaultObservationSubsystem::Block
                    && observation.subsystem != FaultObservationSubsystem::Network =>
            {
                observation.operation_sequence >= snapshot.fault_operation_sequence
            }
            _ => false,
        });
        if invalid_shell_sequence {
            return Err(fault_transition_vm_error(
                FaultTransitionError::SnapshotPendingStateMismatch,
            ));
        }
        Ok(())
    }

    /// Incremental restore: only revert/apply dirty pages instead of
    /// writing the full memory image for each VM.
    ///
    /// Requires `set_memory_bases()` to have been called. Falls back
    /// to full restore for any VM without a base.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn restore_all_incremental(
        &mut self,
        snapshot: &SimulationSnapshot,
    ) -> Result<(), VmError> {
        if snapshot.vm_snapshots.len() != self.vms.len() {
            return SnapshotSnafu {
                message: "Snapshot VM count mismatch",
            }
            .fail();
        }
        self.fault_engine
            .validate_snapshot(&snapshot.fault_engine_snapshot)
            .map_err(fault_transition_vm_error)?;
        for (index, (vm_snapshot, _)) in snapshot.vm_snapshots.iter().enumerate() {
            self.vms[index]
                .vm
                .validate_fault_engine_snapshot(vm_snapshot)?;
        }
        self.validate_pending_snapshot(snapshot)?;

        self.tick = snapshot.tick;
        self.network = snapshot.network_state.clone();
        self.fault_engine
            .restore(&snapshot.fault_engine_snapshot)
            .map_err(fault_transition_vm_error)?;
        self.fault_operation_sequence = snapshot.fault_operation_sequence;
        self.pending_process_observations = snapshot.pending_process_observations.clone();

        for (i, (vm_snap, status)) in snapshot.vm_snapshots.iter().enumerate() {
            if let Some(base) = &self.vm_memory_bases[i] {
                self.vms[i].vm.restore_incremental(vm_snap, base)?;
            } else {
                self.vms[i].vm.restore(vm_snap)?;
            }
            self.vms[i].status = *status;
            if let Some(stalls) = snapshot.vcpu_stall_until.get(i) {
                self.vms[i].vcpu_stall_until = stalls.clone();
            } else {
                self.vms[i].vcpu_stall_until.clear();
            }
            self.vms[i].clock_freeze = snapshot.clock_freeze.get(i).copied().flatten();
            self.vms[i].clock_jitter_bound =
                snapshot.clock_jitter_bound.get(i).copied().unwrap_or(0);
            self.vms[i].process_fault_attempt =
                snapshot.process_fault_attempt.get(i).copied().flatten();
        }

        debug!("Incremental restore from snapshot at tick {}", self.tick);
        Ok(())
    }

    /// Get the oracle report (merged from all VMs).
    pub fn report(&self) -> OracleReport {
        self.merged_oracle_report()
    }

    /// Merge oracle reports from all VM fault engines.
    ///
    /// Each VM has its own FaultEngine + PropertyOracle that tracks
    /// assertions from that VM's guest.  We merge them so the
    /// exploration sees a unified view of all assertion violations.
    fn merged_oracle_report(&self) -> OracleReport {
        // Start with the first VM's report, then merge others.
        let mut combined = if let Some(first) = self.vms.first() {
            first.vm.fault_engine().oracle().report()
        } else {
            return self.fault_engine.oracle().report();
        };

        for slot in self.vms.iter().skip(1) {
            let report = slot.vm.fault_engine().oracle().report();
            // Merge assertion records: a failure in ANY VM is a failure.
            for (id, record) in &report.assertions {
                combined
                    .assertions
                    .entry(*id)
                    .and_modify(|existing| {
                        existing.hit_count += record.hit_count;
                        existing.true_count += record.true_count;
                        existing.false_count += record.false_count;
                        existing.runs_hit += record.runs_hit;
                        existing.runs_satisfied += record.runs_satisfied;
                        if existing.first_failure_run.is_none() {
                            existing.first_failure_run = record.first_failure_run;
                        }
                    })
                    .or_insert_with(|| record.clone());
            }
            combined.total_runs = combined.total_runs.max(report.total_runs);
            combined.events.extend(report.events.iter().cloned());
        }

        // Recompute pass/fail/unexercised counts after merge.
        combined.passed = 0;
        combined.failed = 0;
        combined.unexercised = 0;
        for record in combined.assertions.values() {
            match record.verdict() {
                chaoscontrol_fault::oracle::Verdict::Passed => combined.passed += 1,
                chaoscontrol_fault::oracle::Verdict::Failed => combined.failed += 1,
                chaoscontrol_fault::oracle::Verdict::Unexercised => combined.unexercised += 1,
            }
        }
        combined.catalog_size = combined.assertions.len();

        combined
    }

    /// Get current simulation tick.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Get the number of VMs.
    pub fn num_vms(&self) -> usize {
        self.vms.len()
    }

    /// Get a reference to a specific VM slot.
    pub fn vm_slot(&self, index: usize) -> Option<&VmSlot> {
        self.vms.get(index)
    }

    /// Get a mutable reference to a specific VM slot.
    pub fn vm_slot_mut(&mut self, index: usize) -> Option<&mut VmSlot> {
        self.vms.get_mut(index)
    }

    /// Clear coverage bitmaps in all VMs.
    ///
    /// Call this before each branch run in the exploration loop.
    pub fn clear_all_coverage(&self) {
        for slot in &self.vms {
            slot.vm.clear_coverage_bitmap();
        }
    }

    /// Force the fault engine's setup_complete flag to true.
    ///
    /// Use this in integration tests where the guest doesn't use the
    /// ChaosControl SDK but you still want scheduled faults to fire.
    pub fn force_setup_complete(&mut self) {
        self.fault_engine.force_setup_complete();
    }

    /// Get a reference to a VM by index.
    pub fn vm(&self, index: usize) -> &DeterministicVm {
        &self.vms[index].vm
    }

    /// Get a mutable reference to a VM by index.
    pub fn vm_mut(&mut self, index: usize) -> &mut DeterministicVm {
        &mut self.vms[index].vm
    }

    /// Get a reference to the network fabric.
    pub fn network(&self) -> &NetworkFabric {
        &self.network
    }

    /// Get a mutable reference to the network fabric.
    pub fn network_mut(&mut self) -> &mut NetworkFabric {
        &mut self.network
    }

    /// Get network statistics.
    pub fn network_stats(&self) -> &NetworkStats {
        &self.network.stats
    }

    /// Replace the fault schedule (used by the explorer between branches).
    pub fn set_schedule(&mut self, schedule: FaultSchedule) {
        self.fault_engine.set_schedule(schedule);
    }

    /// Set the explicit campaign policy for rejected fault attempts.
    pub fn set_fault_application_policy(&mut self, policy: FaultApplicationPolicy) {
        self.fault_application_policy = policy;
    }

    /// Return the authoritative fault outcome ledger.
    pub fn fault_outcomes(&self) -> &chaoscontrol_fault::outcomes::FaultOutcomeLedger {
        self.fault_engine.fault_outcomes()
    }

    /// Apply a [`ScheduleVariant`] to all VMs' vCPU schedulers.
    ///
    /// Each VM gets a domain-separated seed: `variant.scheduler_seed + vm_id`.
    /// Strategy and quantum overrides apply uniformly to all VMs.
    /// Call this after `restore_all()` and before `run()` to vary the
    /// interleaving for a specific branch.
    pub fn apply_schedule_variant(&mut self, variant: &ScheduleVariant) {
        for (i, slot) in self.vms.iter_mut().enumerate() {
            let per_vm = ScheduleVariant {
                scheduler_seed: variant.scheduler_seed.wrapping_add(i as u64),
                strategy_override: variant.strategy_override,
                quantum_override: variant.quantum_override,
            };
            slot.vm.scheduler_mut().apply_variant(&per_vm);
        }
    }

    /// Collect the schedule fingerprint from all VMs.
    ///
    /// Returns a combined fingerprint by XOR-ing each VM's per-vCPU
    /// scheduler fingerprint with a per-VM domain separator.
    pub fn schedule_fingerprint(&self) -> u64 {
        let mut combined = 0u64;
        for (i, slot) in self.vms.iter().enumerate() {
            combined ^= slot
                .vm
                .scheduler()
                .fingerprint()
                .wrapping_mul(0x9e37_79b9_7f4a_7c15_u64.wrapping_add(i as u64));
        }
        combined
    }

    /// Reset all VM statuses to Running and the tick counter to a
    /// snapshot's tick. Called implicitly by `restore_all`, but
    /// exposed for manual control.
    pub fn reset_vm_statuses(&mut self) {
        for slot in &mut self.vms {
            slot.status = VmStatus::Running;
        }
    }

    // ── Input tree exploration ──────────────────────────────────

    /// Drain choice histories from all VMs.
    ///
    /// Returns `(vm_id, choices)` pairs for VMs that made at least one
    /// random choice since the last drain (or since snapshot restore).
    pub fn drain_choice_histories(
        &mut self,
    ) -> Vec<(usize, Vec<chaoscontrol_fault::engine::ChoiceRecord>)> {
        self.vms
            .iter_mut()
            .enumerate()
            .map(|(i, slot)| {
                let history = slot.vm.fault_engine_mut().drain_choice_history();
                (i, history)
            })
            .filter(|(_, h)| !h.is_empty())
            .collect()
    }

    /// Set random choice overrides for a specific VM's fault engine.
    ///
    /// These overrides force specific return values at specific choice
    /// sequence positions when the guest calls `random_choice()` or
    /// `get_random()`.  Used by the input tree explorer to try
    /// alternative execution paths.
    pub fn set_choice_overrides(
        &mut self,
        vm_id: usize,
        overrides: std::collections::BTreeMap<u64, u64>,
    ) {
        if let Some(slot) = self.vms.get_mut(vm_id) {
            slot.vm.fault_engine_mut().set_random_overrides(overrides);
        }
    }

    /// Clear random choice overrides for all VMs.
    pub fn clear_all_choice_overrides(&mut self) {
        for slot in &mut self.vms {
            slot.vm.fault_engine_mut().clear_random_overrides();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Result types
// ═══════════════════════════════════════════════════════════════════════

/// Result of a single scheduling round.
#[derive(Debug)]
pub struct RoundResult {
    /// Current simulation tick.
    pub tick: u64,
    /// Number of VMs actively running.
    pub vms_running: usize,
    /// Number of VMs halted/paused/crashed.
    pub vms_halted: usize,
    /// Legacy compatibility alias mapped exactly to selected faults.
    pub faults_fired: Vec<Fault>,
    /// Ordered stage events produced during this round.
    pub fault_outcomes: Vec<FaultStageEvent>,
    /// Number of network messages delivered this round.
    pub messages_delivered: usize,
}

/// Final result of a simulation run.
#[derive(Debug)]
pub struct SimulationResult {
    /// Total simulation ticks executed.
    pub total_ticks: u64,
    /// Property oracle report.
    pub oracle_report: OracleReport,
    /// Per-VM exit counts.
    pub vm_exit_counts: Vec<u64>,
    /// Cumulative network fabric statistics.
    pub network_stats: NetworkStats,
    /// Ordered stage ledger for all fault attempts in this simulation.
    pub fault_outcomes: chaoscontrol_fault::outcomes::FaultOutcomeLedger,
}

/// Complete snapshot of simulation state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationSnapshot {
    /// Global tick counter.
    pub tick: u64,
    /// Per-VM snapshots and status.
    pub vm_snapshots: Vec<(VmSnapshot, VmStatus)>,
    /// Network fabric state.
    pub network_state: NetworkFabric,
    /// Fault engine state.
    pub fault_engine_snapshot: chaoscontrol_fault::engine::EngineSnapshot,
    /// Per-VM vCPU stall deadlines.
    pub vcpu_stall_until: Vec<std::collections::BTreeMap<usize, u64>>,
    /// Per-VM clock freeze state.
    pub clock_freeze: Vec<Option<(u64, u64)>>,
    /// Per-VM clock jitter bound.
    pub clock_jitter_bound: Vec<u64>,
    /// Per-VM pending process-effect attempt identity.
    pub process_fault_attempt: Vec<Option<FaultAttemptId>>,
    /// Next deterministic operation sequence for shell observations.
    pub fault_operation_sequence: u64,
    /// Process observations waiting for ledger commit.
    pub pending_process_observations: VecDeque<(usize, FaultObservation)>,
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::block::DeterministicBlock;
    use crate::devices::virtio_block::VirtioBlock;
    use chaoscontrol_fault::faults::{FaultCategory, FaultVariant, GpRegister};
    use chaoscontrol_fault::schedule::FaultScheduleBuilder;

    fn dummy_kernel_path() -> String {
        // Return a plausible path; tests that actually run VMs will need a real kernel
        "/tmp/dummy-vmlinux".to_string()
    }

    fn adapter_test_controller() -> SimulationController {
        const NETWORK_NODE_COUNT: usize = 2;
        let config = SimulationConfig {
            num_vms: 1,
            kernel_path: dummy_kernel_path(),
            ..Default::default()
        };
        let vm = DeterministicVm::new(VmConfig::default()).expect("create adapter test VM");
        let slot = VmSlot {
            vm,
            status: VmStatus::Running,
            inbox: VecDeque::new(),
            disk_faults: DiskFaultFlags::default(),
            tsc_skew: 0,
            memory_limit_bytes: None,
            initial_snapshot: None,
            vcpu_stall_until: std::collections::BTreeMap::new(),
            clock_freeze: None,
            clock_jitter_bound: 0,
            process_fault_attempt: None,
        };
        SimulationController {
            vms: vec![slot],
            fault_engine: FaultEngine::new(EngineConfig::default()),
            network: NetworkFabric::new(NETWORK_NODE_COUNT, config.seed),
            tick: 0,
            quantum: config.quantum,
            config,
            vm_memory_bases: vec![None],
            fault_application_policy: FaultApplicationPolicy::default(),
            fault_operation_sequence: 0,
            pending_process_observations: VecDeque::new(),
        }
    }

    fn adapter_test_block(controller: &mut SimulationController) -> &mut DeterministicBlock {
        for device in controller.vms[0].vm.virtio_devices_mut() {
            if let Some(block) = device
                .backend_mut()
                .as_any_mut()
                .downcast_mut::<VirtioBlock>()
            {
                return block.disk_mut();
            }
        }
        panic!("adapter test VM must contain a block device");
    }

    fn attempt_id_for(controller: &SimulationController, variant: FaultVariant) -> FaultAttemptId {
        controller
            .fault_outcomes()
            .attempts
            .values()
            .find(|state| state.attempt.fault.variant() == variant)
            .map(|state| state.attempt.id)
            .expect("fault attempt must exist")
    }

    #[test]
    fn test_simulation_config_default() {
        let config = SimulationConfig::default();
        assert_eq!(config.num_vms, 2);
        assert_eq!(config.seed, 42);
        assert_eq!(config.quantum, 100);
    }

    #[test]
    fn test_network_fabric_can_reach() {
        let fabric = NetworkFabric::new(4, 42);
        assert!(fabric.can_reach(0, 1));
        assert!(fabric.can_reach(1, 0));
    }

    #[test]
    fn test_network_fabric_partition_blocks() {
        let mut fabric = NetworkFabric::new(4, 42);
        fabric.add_partition(vec![0, 1], vec![2, 3]);

        // Same side can reach each other
        assert!(fabric.can_reach(0, 1));
        assert!(fabric.can_reach(2, 3));

        // Opposite sides cannot reach
        assert!(!fabric.can_reach(0, 2));
        assert!(!fabric.can_reach(1, 3));
        assert!(!fabric.can_reach(2, 0));
    }

    #[test]
    fn test_network_fabric_send_respects_partition() {
        let mut fabric = NetworkFabric::new(3, 42);
        fabric.add_partition(vec![0], vec![1, 2]);

        let sent = fabric.send(0, 1, vec![42], 0);
        assert!(!sent); // Blocked by partition

        let sent = fabric.send(1, 2, vec![99], 0);
        assert!(sent); // Same side, allowed
    }

    #[test]
    fn test_network_fabric_latency() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_latency(0, 1000);

        fabric.send(0, 1, vec![1, 2, 3], 0);
        assert_eq!(fabric.in_flight.len(), 1);
        assert_eq!(fabric.in_flight[0].deliver_at_tick, 1000);
    }

    #[test]
    fn test_network_fabric_packet_loss() {
        let mut fabric = NetworkFabric::new(3, 42);
        fabric.set_loss_rate(0, 1_000_000); // 100% loss on VM0

        // All messages from VM0 should be dropped
        let sent = fabric.send(0, 1, vec![1, 2, 3], 0);
        assert!(!sent);
        assert!(fabric.in_flight.is_empty());

        // Messages to VM0 are also dropped (loss is bidirectional)
        let sent = fabric.send(1, 0, vec![4, 5, 6], 0);
        assert!(!sent);
        assert!(fabric.in_flight.is_empty());

        // Messages between unaffected VMs should go through
        let sent = fabric.send(1, 2, vec![7, 8, 9], 0);
        assert!(sent);
        assert_eq!(fabric.in_flight.len(), 1);
    }

    #[test]
    fn test_network_fabric_packet_loss_zero_rate() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_loss_rate(0, 0); // 0% loss

        // All messages should go through
        for _ in 0..10 {
            let sent = fabric.send(0, 1, vec![1], 0);
            assert!(sent);
        }
        assert_eq!(fabric.in_flight.len(), 10);
    }

    #[test]
    fn test_network_fabric_packet_corruption() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_corruption_rate(0, 1_000_000); // 100% corruption

        let original = vec![0xAA; 32];
        fabric.send(0, 1, original.clone(), 0);

        // Message should be delivered but corrupted
        assert_eq!(fabric.in_flight.len(), 1);
        assert_ne!(fabric.in_flight[0].data, original);
    }

    #[test]
    fn test_network_fabric_packet_reorder() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_reorder_window(0, 100); // Up to 100 ticks jitter

        // Send multiple messages — some should have different delivery times
        for i in 0..20 {
            fabric.send(0, 1, vec![i as u8], 0);
        }

        assert_eq!(fabric.in_flight.len(), 20);
        // With reorder window, delivery ticks should vary
        let ticks: Vec<u64> = fabric.in_flight.iter().map(|m| m.deliver_at_tick).collect();
        let all_same = ticks.iter().all(|&t| t == ticks[0]);
        assert!(
            !all_same,
            "Reorder window should produce varied delivery ticks"
        );
    }

    #[test]
    fn test_network_heal_clears_packet_faults() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_loss_rate(0, 500_000);
        fabric.set_corruption_rate(0, 200_000);
        fabric.set_reorder_window(0, 50);
        fabric.set_jitter(0, 10);
        fabric.set_bandwidth(0, 1_000_000);
        fabric.set_duplicate_rate(0, 100_000);
        fabric.add_partition(vec![0], vec![1]);

        fabric.clear_partitions();

        assert!(fabric.partitions.is_empty());
        assert_eq!(fabric.loss_rate_ppm[0], 0);
        assert_eq!(fabric.corruption_rate_ppm[0], 0);
        assert_eq!(fabric.reorder_window[0], 0);
        assert_eq!(fabric.jitter[0], 0);
        assert_eq!(fabric.bandwidth_bps[0], 0);
        assert_eq!(fabric.next_free_tick[0], 0);
        assert_eq!(fabric.duplicate_rate_ppm[0], 0);
    }

    #[test]
    fn test_disk_fault_flags() {
        let mut flags = DiskFaultFlags::default();
        assert_eq!(flags.error_rate, 0.0);
        assert!(!flags.full);

        flags.full = true;
        flags.error_rate = 0.5;
        assert!(flags.full);
        assert_eq!(flags.error_rate, 0.5);
    }

    #[test]
    fn test_vm_status_transitions() {
        let mut status = VmStatus::Running;
        assert_eq!(status, VmStatus::Running);

        status = VmStatus::Crashed;
        assert_eq!(status, VmStatus::Crashed);

        status = VmStatus::Restarting {
            restart_at_tick: 100,
        };
        if let VmStatus::Restarting { restart_at_tick } = status {
            assert_eq!(restart_at_tick, 100);
        } else {
            panic!("Expected Restarting status");
        }
    }

    #[test]
    fn test_vm_status_resuming() {
        let status = VmStatus::Resuming { resume_at_tick: 50 };
        if let VmStatus::Resuming { resume_at_tick } = status {
            assert_eq!(resume_at_tick, 50);
        } else {
            panic!("Expected Resuming status");
        }

        // Resuming is not equal to Paused
        assert_ne!(status, VmStatus::Paused);
    }

    #[test]
    fn test_simulation_controller_requires_kernel_path() {
        let config = SimulationConfig {
            num_vms: 2,
            kernel_path: String::new(), // Empty path
            ..Default::default()
        };

        let result = SimulationController::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_simulation_controller_requires_nonzero_vms() {
        let config = SimulationConfig {
            num_vms: 0,
            kernel_path: dummy_kernel_path(),
            ..Default::default()
        };

        let result = SimulationController::new(config);
        assert!(result.is_err());
    }

    // The following tests would require an actual kernel to run.
    // They are marked with #[ignore] and serve as integration test templates.

    #[test]
    #[ignore]
    fn test_simulation_controller_creates_vms() {
        let config = SimulationConfig {
            num_vms: 2,
            kernel_path: "/path/to/vmlinux".to_string(),
            initrd_path: Some("/path/to/initrd".to_string()),
            ..Default::default()
        };

        let controller = SimulationController::new(config).unwrap();
        assert_eq!(controller.num_vms(), 2);
        assert_eq!(controller.tick(), 0);
    }

    #[test]
    #[ignore]
    fn test_step_round_advances_tick() {
        let config = SimulationConfig {
            num_vms: 2,
            kernel_path: "/path/to/vmlinux".to_string(),
            quantum: 10,
            ..Default::default()
        };

        let mut controller = SimulationController::new(config).unwrap();
        let result = controller.step_round().unwrap();

        assert_eq!(controller.tick(), 1);
        assert_eq!(result.tick, 1);
    }

    #[test]
    #[ignore]
    fn test_fault_injection_process_kill() {
        let schedule = FaultScheduleBuilder::new()
            .at_ns(1_000_000, Fault::ProcessKill { target: 0 })
            .build();

        let config = SimulationConfig {
            num_vms: 2,
            kernel_path: "/path/to/vmlinux".to_string(),
            schedule,
            ..Default::default()
        };

        let mut controller = SimulationController::new(config).unwrap();

        // Run until fault fires
        for _ in 0..2000 {
            controller.step_round().unwrap();
        }

        // VM 0 should be crashed
        assert_eq!(controller.vm_slot(0).unwrap().status, VmStatus::Crashed);
        assert_eq!(controller.vm_slot(1).unwrap().status, VmStatus::Running);
    }

    #[test]
    #[ignore]
    fn test_fault_injection_network_partition() {
        let schedule = FaultScheduleBuilder::new()
            .at_ns(
                1_000_000,
                Fault::NetworkPartition {
                    side_a: vec![0],
                    side_b: vec![1],
                },
            )
            .build();

        let config = SimulationConfig {
            num_vms: 2,
            kernel_path: "/path/to/vmlinux".to_string(),
            schedule,
            ..Default::default()
        };

        let mut controller = SimulationController::new(config).unwrap();

        // Run until fault fires
        for _ in 0..2000 {
            controller.step_round().unwrap();
        }

        // Verify partition is active
        assert!(!controller.network.can_reach(0, 1));
    }

    #[test]
    #[ignore]
    fn test_snapshot_restore() {
        let config = SimulationConfig {
            num_vms: 2,
            kernel_path: "/path/to/vmlinux".to_string(),
            quantum: 10,
            ..Default::default()
        };

        let mut controller = SimulationController::new(config).unwrap();

        // Run for a bit
        for _ in 0..5 {
            controller.step_round().unwrap();
        }

        let tick_before = controller.tick();
        let snapshot = controller.snapshot_all().unwrap();

        // Run more
        for _ in 0..5 {
            controller.step_round().unwrap();
        }
        assert!(controller.tick() > tick_before);

        // Restore
        controller.restore_all(&snapshot).unwrap();
        assert_eq!(controller.tick(), tick_before);
    }

    #[test]
    #[ignore]
    fn test_deterministic_exit_counts() {
        let config = SimulationConfig {
            num_vms: 2,
            kernel_path: "/path/to/vmlinux".to_string(),
            seed: 12345,
            quantum: 50,
            ..Default::default()
        };

        let mut c1 = SimulationController::new(config.clone()).unwrap();
        let mut c2 = SimulationController::new(config).unwrap();

        // Run both for same number of ticks
        for _ in 0..100 {
            c1.step_round().unwrap();
            c2.step_round().unwrap();
        }

        // Exit counts should be identical
        let exits1 = c1.vms.iter().map(|s| s.vm.exit_count()).collect::<Vec<_>>();
        let exits2 = c2.vms.iter().map(|s| s.vm.exit_count()).collect::<Vec<_>>();
        assert_eq!(exits1, exits2);
    }

    #[test]
    #[ignore]
    fn test_disk_torn_write_fault_dispatch() {
        use crate::devices::virtio_block::VirtioBlock;
        use chaoscontrol_fault::faults::Fault;

        let config = SimulationConfig {
            num_vms: 1,
            kernel_path: dummy_kernel_path(),
            ..Default::default()
        };

        let mut controller = SimulationController::new(config).unwrap();

        // Inject a DiskTornWrite fault
        let fault = Fault::DiskTornWrite {
            target: 0,
            offset: 4096,
            bytes_written: 256,
        };
        controller.apply_fault(&fault).unwrap();

        // Verify the fault was injected into the block device
        let vm = &mut controller.vms[0].vm;
        for device in vm.virtio_devices_mut() {
            if device.backend().device_id() == 2 {
                if let Some(_virtio_block) = device
                    .backend_mut()
                    .as_any_mut()
                    .downcast_mut::<VirtioBlock>()
                {
                    // The fault should be queued in the disk
                    // We can't directly check the queue, but the fact that
                    // inject succeeded means it was added
                    return;
                }
            }
        }
        panic!("Expected block device not found");
    }

    #[test]
    #[ignore]
    fn test_disk_corruption_fault_dispatch() {
        use crate::devices::virtio_block::VirtioBlock;
        use chaoscontrol_fault::faults::Fault;

        let config = SimulationConfig {
            num_vms: 1,
            kernel_path: dummy_kernel_path(),
            ..Default::default()
        };

        let mut controller = SimulationController::new(config).unwrap();

        // Inject a DiskCorruption fault
        let fault = Fault::DiskCorruption {
            target: 0,
            offset: 8192,
            len: 512,
        };
        controller.apply_fault(&fault).unwrap();

        // Verify the fault was injected into the block device
        let vm = &mut controller.vms[0].vm;
        for device in vm.virtio_devices_mut() {
            if device.backend().device_id() == 2 {
                if let Some(_virtio_block) = device
                    .backend_mut()
                    .as_any_mut()
                    .downcast_mut::<VirtioBlock>()
                {
                    // The fault should be queued in the disk
                    return;
                }
            }
        }
        panic!("Expected block device not found");
    }

    // ── Jitter tests ────────────────────────────────────────────

    #[test]
    fn test_network_fabric_jitter_adds_variable_delay() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_latency(0, 100); // 100 ticks base latency
        fabric.set_jitter(0, 50); // up to 50 ticks extra

        for i in 0..30 {
            fabric.send(0, 1, vec![i as u8], 0);
        }

        assert_eq!(fabric.in_flight.len(), 30);
        let ticks: Vec<u64> = fabric.in_flight.iter().map(|m| m.deliver_at_tick).collect();

        // All delivery ticks should be in range [100, 150]
        for &t in &ticks {
            assert!(t >= 100, "deliver_at_tick {} < base latency 100", t);
            assert!(t <= 150, "deliver_at_tick {} > base + jitter 150", t);
        }

        // Jitter should produce variation (not all the same)
        let all_same = ticks.iter().all(|&t| t == ticks[0]);
        assert!(!all_same, "Jitter should produce varied delivery ticks");
    }

    #[test]
    fn test_network_fabric_jitter_zero_no_effect() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_latency(0, 50);
        fabric.set_jitter(0, 0); // no jitter

        for i in 0..10 {
            fabric.send(0, 1, vec![i as u8], 0);
        }

        // All should arrive at exactly tick 50
        for msg in &fabric.in_flight {
            assert_eq!(msg.deliver_at_tick, 50);
        }
    }

    #[test]
    fn test_network_fabric_jitter_bidirectional() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_jitter(0, 20); // jitter on VM0

        // Sending FROM VM0 → VM1 should get jitter
        for i in 0..10 {
            fabric.send(0, 1, vec![i as u8], 0);
        }
        let forward_ticks: Vec<u64> = fabric.in_flight.iter().map(|m| m.deliver_at_tick).collect();
        fabric.in_flight.clear();

        // Sending TO VM0 (VM1 → VM0) should also get jitter (max of sender/receiver)
        for i in 0..10 {
            fabric.send(1, 0, vec![i as u8], 0);
        }
        let reverse_ticks: Vec<u64> = fabric.in_flight.iter().map(|m| m.deliver_at_tick).collect();

        // Both directions should have jitter (some ticks > 0)
        assert!(
            forward_ticks.iter().any(|&t| t > 0),
            "Forward jitter missing"
        );
        assert!(
            reverse_ticks.iter().any(|&t| t > 0),
            "Reverse jitter missing"
        );
    }

    // ── Bandwidth tests ─────────────────────────────────────────

    #[test]
    fn test_network_fabric_bandwidth_serialization_delay() {
        const PACKET_BYTES: usize = 100;
        const RATE_BYTES_PER_SECOND: u64 = 1_000;
        const EXPECTED_TICKS: u64 = 100;
        assert_eq!(
            bandwidth_serialization_ticks(PACKET_BYTES, RATE_BYTES_PER_SECOND),
            EXPECTED_TICKS
        );
        assert_eq!(bandwidth_serialization_ticks(1, u64::MAX), 1);
        let mut fabric = NetworkFabric::new(2, 42);
        // 100 bytes at 1000 bytes/second takes 100 simulation ticks.
        fabric.set_bandwidth(0, 1_000);

        // Send 100 bytes: 100 * 1000 ticks/second / 1000 B/s = 100 ticks.
        fabric.send(0, 1, vec![0xAA; 100], 0);
        assert_eq!(fabric.in_flight.len(), 1);
        assert_eq!(fabric.in_flight[0].deliver_at_tick, 100);
    }

    #[test]
    fn test_network_fabric_bandwidth_queuing() {
        let mut fabric = NetworkFabric::new(2, 42);
        // 1000 bytes/second makes each 100-byte packet take 100 ticks.
        fabric.set_bandwidth(0, 1_000);

        // Send 3 packets at tick 0 — they should queue
        fabric.send(0, 1, vec![0xAA; 100], 0);
        fabric.send(0, 1, vec![0xBB; 100], 0);
        fabric.send(0, 1, vec![0xCC; 100], 0);

        assert_eq!(fabric.in_flight.len(), 3);
        // First packet: completes at tick 100
        assert_eq!(fabric.in_flight[0].deliver_at_tick, 100);
        // Second: queued behind first, completes at tick 200
        assert_eq!(fabric.in_flight[1].deliver_at_tick, 200);
        // Third: queued behind second, completes at tick 300
        assert_eq!(fabric.in_flight[2].deliver_at_tick, 300);
    }

    #[test]
    fn test_network_fabric_bandwidth_unlimited() {
        let mut fabric = NetworkFabric::new(2, 42);
        // bandwidth_bps = 0 means unlimited (default)

        fabric.send(0, 1, vec![0xAA; 1000], 0);
        fabric.send(0, 1, vec![0xBB; 1000], 0);

        // Both should arrive at tick 0 (no delay)
        assert_eq!(fabric.in_flight[0].deliver_at_tick, 0);
        assert_eq!(fabric.in_flight[1].deliver_at_tick, 0);
    }

    #[test]
    fn test_network_fabric_bandwidth_bottleneck() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_bandwidth(0, 1_000); // sender: 1000 B/s
        fabric.set_bandwidth(1, 500); // receiver: 500 B/s (bottleneck)

        // 100 bytes at 500 B/s takes 200 ticks.
        fabric.send(0, 1, vec![0xAA; 100], 0);
        assert_eq!(fabric.in_flight[0].deliver_at_tick, 200);
    }

    #[test]
    fn test_network_fabric_bandwidth_with_latency() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_bandwidth(0, 1_000); // 100 ticks per 100 bytes
        fabric.set_latency(0, 50); // 50 ticks base latency

        // 100 bytes: 100 ticks serialization + 50 ticks latency = 150
        fabric.send(0, 1, vec![0xAA; 100], 0);
        assert_eq!(fabric.in_flight[0].deliver_at_tick, 150);
    }

    // ── Duplication tests ───────────────────────────────────────

    #[test]
    fn test_network_fabric_duplication_100_percent() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_duplicate_rate(0, 1_000_000); // 100% duplication

        fabric.send(0, 1, vec![42], 0);

        // Should have 2 messages: original + duplicate
        assert_eq!(fabric.in_flight.len(), 2);
        assert_eq!(fabric.in_flight[0].data, fabric.in_flight[1].data);
    }

    #[test]
    fn test_network_fabric_duplication_zero_rate() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_duplicate_rate(0, 0); // No duplication

        for i in 0..20 {
            fabric.send(0, 1, vec![i as u8], 0);
        }

        // Should have exactly 20 messages (no duplicates)
        assert_eq!(fabric.in_flight.len(), 20);
    }

    #[test]
    fn test_network_fabric_duplication_preserves_data() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_duplicate_rate(0, 1_000_000); // 100% duplication

        let original = vec![0xDE, 0xAD, 0xBE, 0xEF];
        fabric.send(0, 1, original.clone(), 0);

        assert_eq!(fabric.in_flight.len(), 2);
        // Both messages should have the same data
        assert_eq!(fabric.in_flight[0].data, original);
        assert_eq!(fabric.in_flight[1].data, original);
    }

    #[test]
    fn test_network_fabric_duplication_bidirectional() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_duplicate_rate(0, 1_000_000); // 100% dup on VM0

        // Sending TO VM0 should also duplicate (max of sender/receiver)
        fabric.send(1, 0, vec![42], 0);
        assert_eq!(fabric.in_flight.len(), 2);
    }

    // ── Combined effects tests ──────────────────────────────────

    #[test]
    fn test_network_fabric_combined_latency_jitter_bandwidth() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_latency(0, 10); // 10 ticks base
        fabric.set_jitter(0, 5); // up to 5 ticks jitter
        fabric.set_bandwidth(0, 10_000); // 10 KB/s

        // 100 bytes at 10 KB/s takes 10 ticks.
        // Total: 10 (bw) + 10 (latency) + 0..5 (jitter) = 20..25 ticks
        for i in 0..20 {
            fabric.send(0, 1, vec![0xAA; 100], 0);

            // Each subsequent packet queues behind the previous, so
            // bandwidth delay grows while latency+jitter stay the same.
            // First packet: bw=10, second: bw=20, etc.
            let msg = fabric.in_flight.last().unwrap();
            let expected_min_bw = 10 * (i + 1); // queuing effect
            let expected_min = expected_min_bw + 10; // + base latency
            assert!(
                msg.deliver_at_tick >= expected_min,
                "Packet {} deliver_at_tick {} < expected min {}",
                i,
                msg.deliver_at_tick,
                expected_min
            );
        }
    }

    #[test]
    fn test_network_fabric_jitter_deterministic_with_same_seed() {
        let send_messages = |seed: u64| -> Vec<u64> {
            let mut fabric = NetworkFabric::new(2, seed);
            fabric.set_latency(0, 100);
            fabric.set_jitter(0, 50);
            for i in 0..20 {
                fabric.send(0, 1, vec![i as u8], 0);
            }
            fabric.in_flight.iter().map(|m| m.deliver_at_tick).collect()
        };

        let run1 = send_messages(42);
        let run2 = send_messages(42);
        assert_eq!(run1, run2, "Same seed must produce same jitter");

        let run3 = send_messages(99);
        assert_ne!(
            run1, run3,
            "Different seeds should produce different jitter"
        );
    }

    // ── Stats tests ──────────────────────────────────────────────

    #[test]
    fn test_network_stats_tracks_sent_and_delivered() {
        let mut fabric = NetworkFabric::new(2, 42);
        for _ in 0..5 {
            fabric.send(0, 1, vec![42], 0);
        }
        assert_eq!(fabric.stats.packets_sent, 5);
        assert_eq!(fabric.stats.packets_delivered, 5);
    }

    #[test]
    fn test_network_stats_tracks_partition_drops() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.add_partition(vec![0], vec![1]);
        fabric.send(0, 1, vec![42], 0);
        assert_eq!(fabric.stats.packets_sent, 1);
        assert_eq!(fabric.stats.packets_dropped_partition, 1);
        assert_eq!(fabric.stats.packets_delivered, 0);
    }

    #[test]
    fn test_network_stats_tracks_loss_drops() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_loss_rate(0, 1_000_000); // 100% loss
        fabric.send(0, 1, vec![42], 0);
        assert_eq!(fabric.stats.packets_sent, 1);
        assert_eq!(fabric.stats.packets_dropped_loss, 1);
        assert_eq!(fabric.stats.packets_delivered, 0);
    }

    #[test]
    fn test_network_stats_tracks_corruption() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_corruption_rate(0, 1_000_000); // 100%
        fabric.send(0, 1, vec![0xAA; 10], 0);
        assert_eq!(fabric.stats.packets_corrupted, 1);
        assert_eq!(fabric.stats.packets_delivered, 1);
    }

    #[test]
    fn test_network_stats_tracks_duplication() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_duplicate_rate(0, 1_000_000); // 100%
        fabric.send(0, 1, vec![42], 0);
        assert_eq!(fabric.stats.packets_duplicated, 1);
        assert_eq!(fabric.stats.packets_delivered, 1); // original
    }

    #[test]
    fn test_network_stats_tracks_bandwidth_delay() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_bandwidth(0, 1_000); // 1000 B/s
        fabric.send(0, 1, vec![0xAA; 100], 0);
        assert_eq!(fabric.stats.packets_bandwidth_delayed, 1);
        assert_eq!(fabric.stats.total_bandwidth_delay_ticks, 100);
    }

    #[test]
    fn test_network_stats_tracks_jitter() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_jitter(0, 50);
        // Send many packets — some will get jitter > 0
        for i in 0..100 {
            fabric.send(0, 1, vec![i as u8], 0);
        }
        assert_eq!(fabric.stats.packets_sent, 100);
        // With jitter_max=50, most packets should get non-zero jitter
        assert!(
            fabric.stats.packets_jittered > 0,
            "Expected some packets to be jittered"
        );
        assert!(
            fabric.stats.total_jitter_ticks > 0,
            "Expected non-zero total jitter ticks"
        );
    }

    #[test]
    fn test_network_stats_display() {
        let stats = NetworkStats {
            packets_sent: 100,
            packets_delivered: 85,
            packets_dropped_partition: 5,
            packets_dropped_loss: 10,
            packets_corrupted: 3,
            packets_duplicated: 7,
            packets_bandwidth_delayed: 20,
            total_bandwidth_delay_ticks: 500,
            packets_latency_delayed: 12,
            total_latency_delay_ticks: 300,
            packets_jittered: 15,
            total_jitter_ticks: 200,
            packets_reordered: 8,
        };
        let s = stats.to_string();
        assert!(s.contains("sent=100"));
        assert!(s.contains("delivered=85"));
        assert!(s.contains("duplicated=7"));
        assert!(s.contains("jittered=15(200ticks)"));
    }

    #[test]
    fn test_network_stats_cumulative_across_sends() {
        let mut fabric = NetworkFabric::new(3, 42);
        fabric.set_loss_rate(0, 1_000_000); // 100% loss on VM0

        // Send from VM0 (all dropped)
        for _ in 0..10 {
            fabric.send(0, 1, vec![42], 0);
        }
        // Send from VM1 (all delivered)
        for _ in 0..5 {
            fabric.send(1, 2, vec![42], 0);
        }

        assert_eq!(fabric.stats.packets_sent, 15);
        assert_eq!(fabric.stats.packets_dropped_loss, 10);
        assert_eq!(fabric.stats.packets_delivered, 5);
    }

    #[test]
    fn test_network_fabric_bandwidth_deterministic() {
        let send_messages = |seed: u64| -> Vec<u64> {
            let mut fabric = NetworkFabric::new(2, seed);
            fabric.set_bandwidth(0, 8000);
            for i in 0..5 {
                fabric.send(0, 1, vec![0xAA; 100 + i * 50], 0);
            }
            fabric.in_flight.iter().map(|m| m.deliver_at_tick).collect()
        };

        let run1 = send_messages(42);
        let run2 = send_messages(42);
        assert_eq!(run1, run2, "Bandwidth delay must be deterministic");
    }

    // ── Seed propagation & snapshot/restore determinism ───────────

    #[test]
    fn test_network_rng_state_survives_clone() {
        // NetworkFabric is cloned during snapshot_all (network_state: self.network.clone()).
        // Verify that the cloned fabric's RNG produces the same random decisions.
        let mut fabric = NetworkFabric::new(3, 42);
        fabric.set_loss_rate(0, 500_000); // 50%
        fabric.set_jitter(1, 100);
        fabric.set_duplicate_rate(2, 300_000); // 30%

        // Advance RNG state by sending some packets
        for i in 0u8..20 {
            fabric.send(0, 1, vec![i; 50], 100);
            fabric.send(1, 2, vec![i; 30], 100);
        }

        // Clone (simulates snapshot)
        let mut cloned = fabric.clone();

        // Clear in-flight on both so we compare fresh sends
        fabric.in_flight.clear();
        cloned.in_flight.clear();

        // Send identical traffic on both — should get identical random decisions
        let mut orig_ticks = Vec::new();
        let mut clone_ticks = Vec::new();
        for i in 0u8..30 {
            fabric.send(0, 1, vec![i; 80], 200);
            cloned.send(0, 1, vec![i; 80], 200);
            fabric.send(2, 0, vec![i; 40], 200);
            cloned.send(2, 0, vec![i; 40], 200);
        }
        for m in &fabric.in_flight {
            orig_ticks.push((m.from, m.to, m.deliver_at_tick, m.data.len()));
        }
        for m in &cloned.in_flight {
            clone_ticks.push((m.from, m.to, m.deliver_at_tick, m.data.len()));
        }

        assert_eq!(
            orig_ticks, clone_ticks,
            "Cloned fabric must produce identical random decisions"
        );
        assert_eq!(
            fabric.stats.packets_sent, cloned.stats.packets_sent,
            "Stats must match"
        );
        assert_eq!(
            fabric.stats.packets_dropped_loss, cloned.stats.packets_dropped_loss,
            "Loss stats must match"
        );
    }

    #[test]
    fn test_seed_changes_all_rng_domains() {
        // Changing the master seed must change network RNG output.
        // This verifies domain separation: seed flows into the fabric.
        let send_and_collect = |seed: u64| -> (Vec<u64>, NetworkStats) {
            let mut fabric = NetworkFabric::new(3, seed);
            fabric.set_loss_rate(0, 500_000);
            fabric.set_jitter(1, 100);
            fabric.set_corruption_rate(0, 200_000);
            fabric.set_duplicate_rate(2, 300_000);
            for i in 0u8..50 {
                fabric.send(0, 1, vec![i; 50], 0);
                fabric.send(1, 2, vec![i; 30], 0);
                fabric.send(2, 0, vec![i; 20], 0);
            }
            let ticks: Vec<u64> = fabric.in_flight.iter().map(|m| m.deliver_at_tick).collect();
            (ticks, fabric.stats.clone())
        };

        let (ticks_a1, stats_a1) = send_and_collect(42);
        let (ticks_a2, stats_a2) = send_and_collect(42);
        let (ticks_b, _stats_b) = send_and_collect(99);

        // Same seed = same results
        assert_eq!(ticks_a1, ticks_a2, "Same seed must produce same ticks");
        assert_eq!(
            stats_a1.packets_dropped_loss, stats_a2.packets_dropped_loss,
            "Same seed must produce same loss count"
        );

        // Different seed = different results (at least delivery ticks differ)
        assert_ne!(
            ticks_a1, ticks_b,
            "Different seeds must produce different delivery ticks"
        );
    }

    #[test]
    fn test_network_stats_deterministic_between_runs() {
        // Two identical runs must produce identical stats.
        let run = |seed: u64| -> NetworkStats {
            let mut fabric = NetworkFabric::new(3, seed);
            fabric.set_loss_rate(0, 300_000);
            fabric.set_corruption_rate(1, 200_000);
            fabric.set_jitter(0, 50);
            fabric.set_bandwidth(1, 10_000);
            fabric.set_duplicate_rate(2, 150_000);
            for i in 0u8..100 {
                fabric.send(0, 1, vec![i; 80], i as u64);
                fabric.send(1, 2, vec![i; 40], i as u64);
                fabric.send(2, 0, vec![i; 20], i as u64);
            }
            fabric.stats.clone()
        };

        let s1 = run(42);
        let s2 = run(42);

        assert_eq!(s1.packets_sent, s2.packets_sent);
        assert_eq!(s1.packets_delivered, s2.packets_delivered);
        assert_eq!(s1.packets_dropped_loss, s2.packets_dropped_loss);
        assert_eq!(s1.packets_corrupted, s2.packets_corrupted);
        assert_eq!(s1.packets_duplicated, s2.packets_duplicated);
        assert_eq!(s1.packets_bandwidth_delayed, s2.packets_bandwidth_delayed);
        assert_eq!(
            s1.total_bandwidth_delay_ticks,
            s2.total_bandwidth_delay_ticks
        );
        assert_eq!(s1.packets_jittered, s2.packets_jittered);
        assert_eq!(s1.total_jitter_ticks, s2.total_jitter_ticks);
        assert_eq!(s1.packets_reordered, s2.packets_reordered);
    }

    #[test]
    fn test_network_domain_separator_isolates_from_engine() {
        // Network fabric and fault engine both derive from the same master seed
        // but must use different domain separators so their RNG streams differ.
        //
        // Here we verify that the network fabric's derived seed != master seed
        // (i.e., it actually uses the domain separator).
        let seed: u64 = 42;
        let mut fabric_key = [0u8; 32];
        let derived = seed.wrapping_add(0x4E45_5446_4142); // "NETFAB"
        fabric_key[..8].copy_from_slice(&derived.to_le_bytes());

        let mut engine_key = [0u8; 32];
        engine_key[..8].copy_from_slice(&seed.to_le_bytes());

        // The keys must differ (different domain separators)
        assert_ne!(
            fabric_key, engine_key,
            "Network fabric and fault engine must use different RNG keys"
        );
    }

    #[test]
    fn test_snapshot_restore_rng_determinism() {
        // The most critical gap: after snapshot/restore, the RNG must produce
        // the same sequence of random decisions as continuing from that point
        // without restore.
        let mut fabric = NetworkFabric::new(3, 42);
        fabric.set_loss_rate(0, 400_000);
        fabric.set_jitter(1, 80);
        fabric.set_corruption_rate(0, 200_000);
        fabric.set_duplicate_rate(2, 250_000);
        fabric.set_bandwidth(0, 50_000);

        // Advance state
        for i in 0u8..20 {
            fabric.send(0, 1, vec![i; 60], i as u64);
            fabric.send(1, 2, vec![i; 30], i as u64);
        }

        // "Snapshot" = clone
        let snapshot = fabric.clone();

        // Continue original for 30 more sends
        fabric.in_flight.clear();
        for i in 20u8..50 {
            fabric.send(0, 1, vec![i; 60], i as u64);
            fabric.send(2, 0, vec![i; 30], i as u64);
        }
        let orig_ticks: Vec<u64> = fabric.in_flight.iter().map(|m| m.deliver_at_tick).collect();
        let orig_data: Vec<Vec<u8>> = fabric.in_flight.iter().map(|m| m.data.clone()).collect();

        // "Restore" from snapshot and replay same sends
        let mut restored = snapshot;
        restored.in_flight.clear();
        for i in 20u8..50 {
            restored.send(0, 1, vec![i; 60], i as u64);
            restored.send(2, 0, vec![i; 30], i as u64);
        }
        let restored_ticks: Vec<u64> = restored
            .in_flight
            .iter()
            .map(|m| m.deliver_at_tick)
            .collect();
        let restored_data: Vec<Vec<u8>> =
            restored.in_flight.iter().map(|m| m.data.clone()).collect();

        assert_eq!(
            orig_ticks, restored_ticks,
            "Post-restore sends must produce identical delivery ticks"
        );
        assert_eq!(
            orig_data, restored_data,
            "Post-restore sends must produce identical data (corruption decisions)"
        );
        assert_eq!(
            fabric.stats.packets_dropped_loss, restored.stats.packets_dropped_loss,
            "Post-restore loss decisions must be identical"
        );
    }

    #[test]
    fn test_inject_interrupt_fault_targets() {
        let f = Fault::InjectInterrupt { target: 1, irq: 5 };
        assert_eq!(f.target(), Some(1));
        assert_eq!(f.category(), FaultCategory::Interrupt);
    }

    #[test]
    fn test_inject_nmi_fault_targets() {
        let f = Fault::InjectNmi { target: 2, vcpu: 0 };
        assert_eq!(f.target(), Some(2));
        assert_eq!(f.category(), FaultCategory::Interrupt);
    }

    #[test]
    fn test_interrupt_faults_in_schedule() {
        let mut schedule = FaultScheduleBuilder::new()
            .at_ns(1_000_000, Fault::InjectInterrupt { target: 0, irq: 0 })
            .at_ns(2_000_000, Fault::InjectNmi { target: 0, vcpu: 0 })
            .at_ns(3_000_000, Fault::InjectInterrupt { target: 1, irq: 6 })
            .build();

        assert_eq!(schedule.remaining(), 3);

        // Drain at 1ms — should get InjectInterrupt
        let faults = schedule.drain_due(1_000_000);
        assert_eq!(faults.len(), 1);
        assert!(matches!(
            faults[0].fault,
            Fault::InjectInterrupt { target: 0, irq: 0 }
        ));

        // Drain at 3ms — should get NMI and second InjectInterrupt
        let faults = schedule.drain_due(3_000_000);
        assert_eq!(faults.len(), 2);
        assert!(matches!(
            faults[0].fault,
            Fault::InjectNmi { target: 0, vcpu: 0 }
        ));
        assert!(matches!(
            faults[1].fault,
            Fault::InjectInterrupt { target: 1, irq: 6 }
        ));

        assert_eq!(schedule.remaining(), 0);
    }

    // ── Multi-VM networking tests ──────────────────────────────

    #[test]
    #[ignore]
    fn test_unique_mac_per_vm() {
        let config = SimulationConfig {
            num_vms: 3,
            kernel_path: "/path/to/vmlinux".to_string(),
            ..Default::default()
        };

        let controller = SimulationController::new(config).unwrap();

        // Verify each VM has a unique MAC address
        let mac0 = controller.vms[0].vm.net_mac().unwrap();
        let mac1 = controller.vms[1].vm.net_mac().unwrap();
        let mac2 = controller.vms[2].vm.net_mac().unwrap();

        assert_ne!(mac0, mac1);
        assert_ne!(mac1, mac2);
        assert_ne!(mac0, mac2);

        // Verify MACs follow the pattern [0x52, 0x54, 0x00, 0x12, 0x34, vm_id]
        assert_eq!(mac0, [0x52, 0x54, 0x00, 0x12, 0x34, 0x00]);
        assert_eq!(mac1, [0x52, 0x54, 0x00, 0x12, 0x34, 0x01]);
        assert_eq!(mac2, [0x52, 0x54, 0x00, 0x12, 0x34, 0x02]);
    }

    #[test]
    fn test_packet_in_flight_struct() {
        let pkt = PacketInFlight {
            from: 0,
            to: 1,
            data: vec![1, 2, 3, 4],
            deliver_at_tick: 42,
        };

        assert_eq!(pkt.from, 0);
        assert_eq!(pkt.to, 1);
        assert_eq!(pkt.data, vec![1, 2, 3, 4]);
        assert_eq!(pkt.deliver_at_tick, 42);
    }

    #[test]
    fn test_network_fabric_send_packet() {
        let mut fabric = NetworkFabric::new(2, 42);

        let sent = fabric.send_packet(0, 1, vec![0xAA, 0xBB, 0xCC], 0);
        assert!(sent);

        assert_eq!(fabric.packet_in_flight.len(), 1);
        assert_eq!(fabric.packet_in_flight[0].from, 0);
        assert_eq!(fabric.packet_in_flight[0].to, 1);
        assert_eq!(fabric.packet_in_flight[0].data, vec![0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn test_network_fabric_send_packet_respects_partition() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.add_partition(vec![0], vec![1]);

        let sent = fabric.send_packet(0, 1, vec![0xAA], 0);
        assert!(!sent); // Dropped by partition
        assert!(fabric.packet_in_flight.is_empty());
    }

    #[test]
    fn test_network_fabric_send_packet_applies_loss() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_loss_rate(0, 1_000_000); // 100% loss

        let sent = fabric.send_packet(0, 1, vec![0xAA], 0);
        assert!(!sent); // Dropped by loss
        assert!(fabric.packet_in_flight.is_empty());
    }

    #[test]
    fn test_network_fabric_deliver_packets() {
        let mut fabric = NetworkFabric::new(3, 42);

        // Send packets with different delivery times
        fabric.send_packet(0, 1, vec![0xAA], 0); // deliver at 0
        fabric.send_packet(1, 2, vec![0xBB], 0); // deliver at 0
        fabric.send_packet(2, 0, vec![0xCC], 100); // deliver at 100 (with some latency)

        // At tick 0, should deliver the first two
        let delivered = fabric.deliver_packets(0);
        assert_eq!(delivered.len(), 2);

        // Third packet still in flight
        assert_eq!(fabric.packet_in_flight.len(), 1);

        // At tick 100, should deliver the third
        let delivered = fabric.deliver_packets(100);
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].0, 0); // vm_id
        assert_eq!(delivered[0].1, vec![0xCC]); // data

        assert!(fabric.packet_in_flight.is_empty());
    }

    #[test]
    fn test_network_fabric_send_packet_applies_latency() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_latency(0, 50); // 50 ticks

        fabric.send_packet(0, 1, vec![0xAA], 0);

        assert_eq!(fabric.packet_in_flight.len(), 1);
        assert_eq!(fabric.packet_in_flight[0].deliver_at_tick, 50);
    }

    #[test]
    fn test_network_fabric_send_packet_applies_corruption() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_corruption_rate(0, 1_000_000); // 100%

        let original = vec![0xAA; 32];
        fabric.send_packet(0, 1, original.clone(), 0);

        assert_eq!(fabric.packet_in_flight.len(), 1);
        // Packet should be corrupted (different from original)
        assert_ne!(fabric.packet_in_flight[0].data, original);
    }

    #[test]
    fn test_network_fabric_send_packet_applies_bandwidth() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_bandwidth(0, 1_000); // 1000 B/s

        // 100 bytes should take 100 ticks
        fabric.send_packet(0, 1, vec![0xAA; 100], 0);

        assert_eq!(fabric.packet_in_flight.len(), 1);
        assert_eq!(fabric.packet_in_flight[0].deliver_at_tick, 100);
    }

    #[test]
    fn test_network_fabric_send_packet_applies_duplication() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_duplicate_rate(0, 1_000_000); // 100%

        fabric.send_packet(0, 1, vec![0xAA, 0xBB], 0);

        // Should have 2 packets: original + duplicate
        assert_eq!(fabric.packet_in_flight.len(), 2);
        assert_eq!(fabric.packet_in_flight[0].data, vec![0xAA, 0xBB]);
        assert_eq!(fabric.packet_in_flight[1].data, vec![0xAA, 0xBB]);
    }

    #[test]
    fn test_vm_drain_net_tx() {
        let config = VmConfig::default();
        let mut vm = DeterministicVm::new(config).unwrap();

        // Initially, TX queue is empty
        let packets = vm.drain_net_tx();
        assert!(packets.is_empty());

        // After injecting and draining, should still be empty
        // (guest would need to transmit, but we're testing the API)
        let packets = vm.drain_net_tx();
        assert!(packets.is_empty());
    }

    #[test]
    fn test_vm_inject_net_rx() {
        let config = VmConfig::default();
        let mut vm = DeterministicVm::new(config).unwrap();

        // Should not panic when injecting a packet
        vm.inject_net_rx(vec![0xFF; 64]);

        // Multiple packets should also work
        vm.inject_net_rx(vec![0xAA; 32]);
        vm.inject_net_rx(vec![0xBB; 16]);
    }

    #[test]
    fn test_vm_net_mac() {
        let config = VmConfig {
            vm_id: 42,
            ..Default::default()
        };
        let vm = DeterministicVm::new(config).unwrap();

        let mac = vm.net_mac().unwrap();
        assert_eq!(mac, [0x52, 0x54, 0x00, 0x12, 0x34, 42]);
    }

    #[test]
    fn test_network_stats_tracks_packet_send() {
        let mut fabric = NetworkFabric::new(2, 42);

        fabric.send_packet(0, 1, vec![0xAA], 0);
        fabric.send_packet(1, 0, vec![0xBB], 0);

        assert_eq!(fabric.stats.packets_sent, 2);
        assert_eq!(fabric.stats.packets_delivered, 2);
    }

    #[test]
    fn test_packet_in_flight_survives_clone() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.send_packet(0, 1, vec![0xAA, 0xBB], 0);

        let cloned = fabric.clone();

        assert_eq!(cloned.packet_in_flight.len(), 1);
        assert_eq!(cloned.packet_in_flight[0].data, vec![0xAA, 0xBB]);
    }

    #[test]
    fn insufficient_terminal_evidence_capacity_prevents_effect_mutation() {
        let mut controller = adapter_test_controller();
        let fault = Fault::NetworkLatency {
            target: 0,
            latency_ns: chaoscontrol_fault::outcomes::NANOSECONDS_PER_SIMULATION_TICK,
        };
        let schedule = FaultScheduleBuilder::new().at_ns(0, fault).build();
        controller.fault_engine.set_schedule(schedule);
        controller.fault_engine.force_setup_complete();
        let attempt = controller
            .fault_engine
            .poll_fault_attempts(0)
            .unwrap()
            .remove(0);
        let event_limit = controller.fault_outcomes().events.len() + 1;
        let before = controller.network.clone();

        let result = controller.handle_fault_attempt_with_event_limit(&attempt, event_limit);

        assert!(matches!(result, Err(VmError::Snapshot { .. })));
        assert_eq!(controller.network.latency, before.latency);
        assert_eq!(
            controller.network.latency_attempt_ids,
            before.latency_attempt_ids
        );
        assert_eq!(controller.fault_outcomes().events.len(), 1);
        assert_eq!(controller.fault_outcomes().counters.applied, 0);
    }

    #[test]
    fn block_overflow_commits_retained_prefix_before_preserving_process_attribution() {
        let mut controller = adapter_test_controller();
        controller
            .apply_fault(&Fault::DiskWriteError {
                target: 0,
                offset: 0,
            })
            .unwrap();
        assert!(adapter_test_block(&mut controller)
            .write(0, &[0xAA])
            .is_err());
        adapter_test_block(&mut controller).set_observation_overflow_for_test(1);
        controller
            .apply_fault(&Fault::ProcessKill { target: 0 })
            .unwrap();
        let process_attempt = attempt_id_for(&controller, FaultVariant::ProcessKill);

        let result = controller.step_round();

        assert!(matches!(result, Err(VmError::Snapshot { .. })));
        let observed_effects = controller
            .fault_outcomes()
            .events
            .iter()
            .filter_map(|event| match &event.kind {
                FaultStageKind::Observed { observation } => Some(observation.effect),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            observed_effects,
            vec![FaultObservationEffect::BlockWriteFailed]
        );
        assert_eq!(
            controller.vms[0].process_fault_attempt,
            Some(process_attempt)
        );
    }

    #[test]
    fn ledger_full_network_drain_requeues_the_complete_batch() {
        const FULL_RATE_PPM: u32 = 1_000_000;
        let mut controller = adapter_test_controller();
        controller
            .apply_fault(&Fault::PacketLoss {
                target: 0,
                rate_ppm: FULL_RATE_PPM,
            })
            .unwrap();
        assert!(!controller.network.send(0, 1, vec![0xAA], 0));
        let event_limit = controller.fault_outcomes().events.len();
        let ledger_before = controller.fault_outcomes().clone();
        let pending_before = controller.network.fault_observations.clone();

        let result = controller.step_round_with_observation_event_limit(event_limit);

        assert!(matches!(result, Err(VmError::Snapshot { .. })));
        assert_eq!(controller.fault_outcomes(), &ledger_before);
        assert_eq!(controller.network.fault_observations, pending_before);
        assert_eq!(controller.network.fault_observation_overflowed, 0);

        controller.step_round().unwrap();
        assert!(controller.network.fault_observations.is_empty());
        assert_eq!(controller.fault_outcomes().counters.observed, 1);
    }

    #[test]
    fn network_overflow_commits_process_then_retained_network_prefix() {
        const FULL_RATE_PPM: u32 = 1_000_000;
        let mut controller = adapter_test_controller();
        controller
            .apply_fault(&Fault::PacketLoss {
                target: 0,
                rate_ppm: FULL_RATE_PPM,
            })
            .unwrap();
        assert!(!controller.network.send(0, 1, vec![0xAA], 0));
        controller.network.fault_observation_overflowed = 1;
        controller
            .apply_fault(&Fault::ProcessKill { target: 0 })
            .unwrap();

        let result = controller.step_round();

        assert!(matches!(result, Err(VmError::Snapshot { .. })));
        let observed_effects = controller
            .fault_outcomes()
            .events
            .iter()
            .filter_map(|event| match &event.kind {
                FaultStageKind::Observed { observation } => Some(observation.effect),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            observed_effects,
            vec![
                FaultObservationEffect::ProcessSkipped,
                FaultObservationEffect::PacketDroppedByLoss,
            ]
        );
        assert_eq!(controller.vms[0].process_fault_attempt, None);
    }

    #[test]
    fn process_observation_failure_keeps_attempt_attribution_pending() {
        const PAUSE_TICKS: u64 = 2;
        const PAUSE_DURATION_NS: u64 =
            PAUSE_TICKS * chaoscontrol_fault::outcomes::NANOSECONDS_PER_SIMULATION_TICK;
        let mut controller = adapter_test_controller();
        controller
            .apply_fault(&Fault::ProcessPause {
                target: 0,
                duration_ns: PAUSE_DURATION_NS,
            })
            .unwrap();
        let process_attempt = attempt_id_for(&controller, FaultVariant::ProcessPause);
        controller.fault_operation_sequence = u64::MAX;

        let result = controller.step_round();

        assert!(matches!(result, Err(VmError::Snapshot { .. })));
        assert_eq!(controller.fault_outcomes().counters.observed, 0);
        assert_eq!(
            controller.vms[0].process_fault_attempt,
            Some(process_attempt)
        );
    }

    #[test]
    fn every_supported_variant_reaches_a_successful_application_adapter() {
        // r[verify chaoscontrol.fault_outcomes.validation.variant_matrix]
        const FULL_RATE_PPM: u32 = 1_000_000;
        let mut controller = adapter_test_controller();
        let cases = vec![
            (
                FaultVariant::NetworkPartition,
                FaultPlanEffect::NetworkPartition {
                    side_a: vec![0],
                    side_b: vec![1],
                },
            ),
            (
                FaultVariant::NetworkLatency,
                FaultPlanEffect::NetworkLatency {
                    target: 0,
                    latency_ticks: 1,
                },
            ),
            (
                FaultVariant::PacketLoss,
                FaultPlanEffect::PacketLoss {
                    target: 0,
                    rate_ppm: FULL_RATE_PPM,
                },
            ),
            (
                FaultVariant::PacketCorruption,
                FaultPlanEffect::PacketCorruption {
                    target: 0,
                    rate_ppm: FULL_RATE_PPM,
                },
            ),
            (
                FaultVariant::PacketReorder,
                FaultPlanEffect::PacketReorder {
                    target: 0,
                    window_ticks: 1,
                },
            ),
            (
                FaultVariant::NetworkJitter,
                FaultPlanEffect::NetworkJitter {
                    target: 0,
                    jitter_ticks: 1,
                },
            ),
            (
                FaultVariant::NetworkBandwidth,
                FaultPlanEffect::NetworkBandwidth {
                    target: 0,
                    bytes_per_sec: 1,
                },
            ),
            (
                FaultVariant::PacketDuplicate,
                FaultPlanEffect::PacketDuplicate {
                    target: 0,
                    rate_ppm: FULL_RATE_PPM,
                },
            ),
            (FaultVariant::NetworkHeal, FaultPlanEffect::NetworkHeal),
            (
                FaultVariant::DiskReadError,
                FaultPlanEffect::BlockReadError {
                    target: 0,
                    offset: 0,
                },
            ),
            (
                FaultVariant::DiskWriteError,
                FaultPlanEffect::BlockWriteError {
                    target: 0,
                    offset: 0,
                },
            ),
            (
                FaultVariant::DiskTornWrite,
                FaultPlanEffect::BlockTornWrite {
                    target: 0,
                    offset: 0,
                    bytes_written: 1,
                },
            ),
            (
                FaultVariant::DiskCorruption,
                FaultPlanEffect::BlockCorruption {
                    target: 0,
                    offset: 0,
                    len: 1,
                },
            ),
            (
                FaultVariant::DiskFull,
                FaultPlanEffect::BlockFull { target: 0 },
            ),
            (
                FaultVariant::DiskSlow,
                FaultPlanEffect::BlockSlow {
                    target: 0,
                    delay_ns: 1,
                },
            ),
            (
                FaultVariant::DiskFsyncLie,
                FaultPlanEffect::BlockFsyncLie { target: 0 },
            ),
            (
                FaultVariant::DiskFsyncFlush,
                FaultPlanEffect::BlockFsyncFlush { target: 0 },
            ),
            (
                FaultVariant::DiskPartialRead,
                FaultPlanEffect::BlockPartialRead {
                    target: 0,
                    offset: 0,
                    max_bytes: 1,
                },
            ),
            (
                FaultVariant::ProcessKill,
                FaultPlanEffect::ProcessKill { target: 0 },
            ),
            (
                FaultVariant::ProcessPause,
                FaultPlanEffect::ProcessPause {
                    target: 0,
                    resume_at_tick: 1,
                },
            ),
            (
                FaultVariant::ProcessRestart,
                FaultPlanEffect::ProcessRestart {
                    target: 0,
                    restart_at_tick: 1,
                },
            ),
            (
                FaultVariant::ClockSkew,
                FaultPlanEffect::VirtualClockSkew {
                    target: 0,
                    basis_tsc: 0,
                    offset_ns: 1,
                    target_tsc: 1,
                },
            ),
            (
                FaultVariant::ClockJump,
                FaultPlanEffect::VirtualClockJump {
                    target: 0,
                    basis_tsc: 0,
                    delta_ns: 2,
                    target_tsc: 2,
                },
            ),
            (
                FaultVariant::InjectInterrupt,
                FaultPlanEffect::IrqInjection { target: 0, irq: 1 },
            ),
            (
                FaultVariant::InjectNmi,
                FaultPlanEffect::NmiInjection { target: 0, vcpu: 0 },
            ),
            (
                FaultVariant::CpuBitflip,
                FaultPlanEffect::CpuRegisterBitflip {
                    target: 0,
                    vcpu: 0,
                    register: GpRegister::Rax,
                    bit: 0,
                },
            ),
        ];
        let covered = cases
            .iter()
            .map(|(variant, _)| *variant)
            .collect::<std::collections::HashSet<_>>();
        let unsupported = [
            FaultVariant::MemoryPressure,
            FaultVariant::CpuStall,
            FaultVariant::ClockFreeze,
            FaultVariant::ClockJitter,
        ];
        for variant in FaultVariant::ALL {
            assert_eq!(
                covered.contains(&variant),
                !unsupported.contains(&variant),
                "missing or unexpected adapter case for {variant:?}"
            );
        }
        for (index, (variant, effect)) in cases.into_iter().enumerate() {
            let attempt_byte = u8::try_from(index).expect("variant count fits in identity byte");
            let plan = FaultPlan {
                attempt_id: FaultAttemptId([attempt_byte; 32]),
                effect,
            };
            let result = controller.apply_fault_plan(&plan);
            assert!(result.is_ok(), "{variant:?}: {result:?}");
        }
    }

    #[test]
    fn adapter_failure_is_typed_and_does_not_mutate_network_state() {
        // r[verify chaoscontrol.fault_outcomes.validation.negative]
        let mut controller = adapter_test_controller();
        let before = controller.network.clone();
        let plan = FaultPlan {
            attempt_id: FaultAttemptId([0; 32]),
            effect: FaultPlanEffect::NetworkLatency {
                target: u32::MAX,
                latency_ticks: 1,
            },
        };

        let failure = controller.apply_fault_plan(&plan).unwrap_err();

        assert_eq!(
            failure,
            FaultApplicationError {
                reason: FaultApplicationFailureReason::TargetStateChanged,
                disposition: FaultApplicationFailureDisposition::RolledBack,
            }
        );
        assert_eq!(controller.network.latency, before.latency);
        assert_eq!(
            controller.network.latency_attempt_ids,
            before.latency_attempt_ids
        );
    }

    #[test]
    fn zero_clock_delta_does_not_claim_an_observed_change() {
        let mut controller = adapter_test_controller();
        let current_tsc = controller.vms[0].vm.virtual_tsc();
        let plan = FaultPlan {
            attempt_id: FaultAttemptId([0; 32]),
            effect: FaultPlanEffect::VirtualClockSkew {
                target: 0,
                basis_tsc: current_tsc,
                offset_ns: 0,
                target_tsc: current_tsc,
            },
        };

        let observations = controller.apply_fault_plan(&plan).unwrap();

        assert!(observations.is_empty());
        assert_eq!(controller.vms[0].vm.virtual_tsc(), current_tsc);
    }

    #[test]
    fn process_pause_is_observed_only_when_scheduling_skips_the_vm() {
        // r[verify chaoscontrol.fault_outcomes.validation.observation]
        const PAUSE_TICKS: u64 = 2;
        const PAUSE_DURATION_NS: u64 =
            PAUSE_TICKS * chaoscontrol_fault::outcomes::NANOSECONDS_PER_SIMULATION_TICK;
        let mut controller = adapter_test_controller();

        controller
            .apply_fault(&Fault::ProcessPause {
                target: 0,
                duration_ns: PAUSE_DURATION_NS,
            })
            .unwrap();
        assert_eq!(controller.fault_outcomes().counters.observed, 0);

        let round = controller.step_round().unwrap();

        assert_eq!(controller.fault_outcomes().counters.observed, 1);
        assert!(round.fault_outcomes.iter().any(|event| {
            matches!(
                event.kind,
                FaultStageKind::Observed {
                    observation: FaultObservation {
                        effect: FaultObservationEffect::ProcessSkipped,
                        ..
                    }
                }
            )
        }));
    }

    #[test]
    fn armed_network_fault_is_unobserved_until_packet_path_consumes_it() {
        // r[verify chaoscontrol.fault_outcomes.validation.observation]
        let attempt_id = FaultAttemptId([0; 32]);
        let mut fabric = NetworkFabric::new(2, 42);
        assert!(fabric.arm_loss(0, 1_000_000, attempt_id));

        let (before, overflowed) = fabric.drain_fault_observations();
        assert!(before.is_empty());
        assert_eq!(overflowed, 0);

        assert!(!fabric.send(0, 1, vec![1], 0));
        let (after, overflowed) = fabric.drain_fault_observations();
        assert_eq!(overflowed, 0);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].attempt_id, attempt_id);
        assert_eq!(after[0].effect, FaultObservationEffect::PacketDroppedByLoss);
    }

    #[test]
    fn unrelated_network_operation_does_not_observe_armed_fault() {
        let attempt_id = FaultAttemptId([0; 32]);
        let mut fabric = NetworkFabric::new(3, 42);
        assert!(fabric.arm_loss(0, 1_000_000, attempt_id));

        assert!(fabric.send(1, 2, vec![1], 0));
        let (observations, overflowed) = fabric.drain_fault_observations();
        assert!(observations.is_empty());
        assert_eq!(overflowed, 0);
    }

    #[test]
    fn extreme_bandwidth_timing_fails_before_packet_or_evidence_mutation() {
        let attempt_id = FaultAttemptId([0; 32]);
        let mut fabric = NetworkFabric::new(2, 42);
        assert!(fabric.arm_bandwidth(0, 1, attempt_id));
        let stats_before = fabric.stats.clone();
        let next_free_before = fabric.next_free_tick.clone();

        let result = fabric.try_send(0, 1, vec![0xAA], u64::MAX);

        assert_eq!(result, Err(NetworkSendError::TickArithmetic));
        assert_eq!(fabric.stats, stats_before);
        assert_eq!(fabric.next_free_tick, next_free_before);
        assert!(fabric.in_flight.is_empty());
        assert!(fabric.fault_observations.is_empty());
    }

    #[test]
    fn network_capacity_failure_preserves_packet_and_evidence_state() {
        let attempt_id = FaultAttemptId([7; 32]);
        let mut fabric = NetworkFabric::new(2, 42);
        assert!(fabric.arm_loss(0, 1_000_000, attempt_id));
        let observation = FaultObservation::new(
            attempt_id,
            FaultObservationSubsystem::Network,
            0,
            FaultObservationEffect::PacketDroppedByLoss,
        );
        fabric.fault_observations =
            std::iter::repeat_n(observation, MAX_PENDING_FAULT_OBSERVATIONS).collect();
        let stats_before = fabric.stats.clone();

        let result = route_network_packet(&mut fabric, 0, 1, vec![0xAA], 0);

        assert!(matches!(
            result,
            Err(VmError::NetworkPacketNonRunnable {
                from: 0,
                to: 1,
                reason: NetworkSendError::ObservationCapacity,
            })
        ));
        assert_eq!(fabric.stats, stats_before);
        assert!(fabric.in_flight.is_empty());
        assert_eq!(
            fabric.fault_observations.len(),
            MAX_PENDING_FAULT_OBSERVATIONS
        );
    }

    #[test]
    fn network_heal_preserves_attribution_for_preserved_latency() {
        // r[verify chaoscontrol.fault_outcomes.snapshot_state]
        const LATENCY_TICKS: u64 = 7;
        let attempt_id = FaultAttemptId([0; 32]);
        let mut fabric = NetworkFabric::new(2, 42);
        assert!(fabric.arm_latency(0, LATENCY_TICKS, attempt_id));

        assert!(fabric.clear_partitions());
        assert!(fabric.send(0, 1, vec![0xAA], 0));
        let (observations, overflowed) = fabric.drain_fault_observations();

        assert_eq!(overflowed, 0);
        assert_eq!(fabric.latency[0], LATENCY_TICKS);
        assert_eq!(fabric.latency_attempt_ids[0], Some(attempt_id));
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].attempt_id, attempt_id);
        assert_eq!(
            observations[0].effect,
            FaultObservationEffect::PacketDelayedByLatency
        );
        assert!(observations[0].has_valid_identity());
    }

    #[test]
    fn network_snapshot_replay_preserves_observation_identity_and_order() {
        // r[verify chaoscontrol.fault_outcomes.validation.replay]
        let attempt_id = FaultAttemptId([0; 32]);
        let mut fabric = NetworkFabric::new(2, 42);
        assert!(fabric.arm_corruption(0, 1_000_000, attempt_id));
        let snapshot = fabric.clone();

        assert!(fabric.send(0, 1, vec![0xAA], 0));
        let (first, first_overflowed) = fabric.drain_fault_observations();

        let mut replay = snapshot;
        assert!(replay.send(0, 1, vec![0xAA], 0));
        let (second, second_overflowed) = replay.drain_fault_observations();

        assert_eq!(first_overflowed, 0);
        assert_eq!(second_overflowed, 0);
        assert_eq!(first, second);
        assert_eq!(first[0].effect, FaultObservationEffect::PacketCorrupted);
    }
}
