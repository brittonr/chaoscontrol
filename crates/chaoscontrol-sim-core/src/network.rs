//! Pure deterministic virtual-network state and transitions.

use chaoscontrol_fault::outcomes::{
    validate_pending_fault_effect, validate_pending_fault_observations, FaultAttemptId,
    FaultObservation, FaultObservationEffect, FaultObservationSubsystem, FaultOutcomeLedger,
    FaultPlanEffect, FaultTransitionError,
};
use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use std::collections::VecDeque;

pub const MAX_PENDING_FAULT_OBSERVATIONS: usize = 4_096;
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

pub fn bandwidth_serialization_ticks(packet_bytes: usize, bytes_per_second: u64) -> u64 {
    checked_bandwidth_serialization_ticks(packet_bytes, bytes_per_second)
        .expect("packet timing must be admitted before application")
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

/// A message in the virtual network.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

    /// Return the maximum new attributed observations that this queue can retain.
    pub fn central_observation_reservation(&self) -> usize {
        let vectors = [
            &self.latency_attempt_ids,
            &self.jitter_attempt_ids,
            &self.bandwidth_attempt_ids,
            &self.loss_attempt_ids,
            &self.corruption_attempt_ids,
            &self.reorder_attempt_ids,
            &self.duplicate_attempt_ids,
        ];
        let attributed_mechanism_active = self.partition_attempt_ids.iter().any(Option::is_some)
            || vectors
                .into_iter()
                .any(|attempts| attempts.iter().any(Option::is_some));
        if attributed_mechanism_active {
            MAX_PENDING_FAULT_OBSERVATIONS - self.fault_observations.len()
        } else {
            0
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
            };
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
            };
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
            };
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
            };
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

    pub fn drain_fault_observations(&mut self) -> (Vec<FaultObservation>, u64) {
        let observations = self.fault_observations.drain(..).collect();
        let overflowed = self.fault_observation_overflowed;
        self.fault_observation_overflowed = 0;
        (observations, overflowed)
    }

    pub fn requeue_fault_observations(
        &mut self,
        observations: Vec<FaultObservation>,
        overflowed: u64,
    ) {
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

    pub fn validate_pending_faults(
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
    pub fn add_partition(&mut self, side_a: Vec<usize>, side_b: Vec<usize>) -> bool {
        self.partitions.push((side_a, side_b));
        self.partition_attempt_ids.push(None);
        true
    }

    pub fn arm_partition(
        &mut self,
        side_a: Vec<usize>,
        side_b: Vec<usize>,
        attempt_id: FaultAttemptId,
    ) -> bool {
        self.partitions.push((side_a, side_b));
        self.partition_attempt_ids.push(Some(attempt_id));
        true
    }

    /// Clear all partitions and packet-level faults (heal network).
    ///
    /// Resets: partitions, loss, corruption, reorder, jitter, bandwidth,
    /// and duplication rates.  Base latency is preserved (use
    /// `set_latency(target, 0)` to clear it explicitly).
    pub fn clear_partitions(&mut self) -> bool {
        self.partitions.clear();
        self.partition_attempt_ids.clear();
        self.loss_rate_ppm.fill(0);
        self.corruption_rate_ppm.fill(0);
        self.reorder_window.fill(0);
        self.jitter.fill(0);
        self.bandwidth_bps.fill(0);
        self.next_free_tick.fill(0);
        self.duplicate_rate_ppm.fill(0);
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
    pub fn set_latency(&mut self, target: usize, latency_ticks: u64) -> bool {
        let Some(slot) = self.latency.get_mut(target) else {
            return false;
        };
        *slot = latency_ticks;
        self.latency_attempt_ids[target] = None;
        true
    }

    /// Set packet loss rate for a specific VM.
    pub fn set_loss_rate(&mut self, target: usize, rate_ppm: u32) -> bool {
        let Some(slot) = self.loss_rate_ppm.get_mut(target) else {
            return false;
        };
        *slot = rate_ppm;
        self.loss_attempt_ids[target] = None;
        true
    }

    /// Set packet corruption rate for a specific VM.
    pub fn set_corruption_rate(&mut self, target: usize, rate_ppm: u32) -> bool {
        let Some(slot) = self.corruption_rate_ppm.get_mut(target) else {
            return false;
        };
        *slot = rate_ppm;
        self.corruption_attempt_ids[target] = None;
        true
    }

    /// Set reorder window for a specific VM (in ticks).
    pub fn set_reorder_window(&mut self, target: usize, window_ticks: u64) -> bool {
        let Some(slot) = self.reorder_window.get_mut(target) else {
            return false;
        };
        *slot = window_ticks;
        self.reorder_attempt_ids[target] = None;
        true
    }

    /// Set latency jitter for a specific VM (in ticks).
    ///
    /// Each packet to/from this VM receives up to `jitter_ticks` extra
    /// random delay on top of the base latency.
    pub fn set_jitter(&mut self, target: usize, jitter_ticks: u64) -> bool {
        let Some(slot) = self.jitter.get_mut(target) else {
            return false;
        };
        *slot = jitter_ticks;
        self.jitter_attempt_ids[target] = None;
        true
    }

    /// Set bandwidth limit for a specific VM (bytes per second).
    ///
    /// Models serialization delay: a 1500-byte packet on a 1 MB/s link
    /// takes ~12 µs (0.012 ticks).  Set to 0 for unlimited.
    pub fn set_bandwidth(&mut self, target: usize, bytes_per_sec: u64) -> bool {
        let Some(slot) = self.bandwidth_bps.get_mut(target) else {
            return false;
        };
        *slot = bytes_per_sec;
        self.bandwidth_attempt_ids[target] = None;
        true
    }

    /// Get a reference to the cumulative network statistics.
    pub fn stats(&self) -> &NetworkStats {
        &self.stats
    }

    /// Set packet duplication rate for a specific VM (parts per million).
    pub fn set_duplicate_rate(&mut self, target: usize, rate_ppm: u32) -> bool {
        let Some(slot) = self.duplicate_rate_ppm.get_mut(target) else {
            return false;
        };
        *slot = rate_ppm;
        self.duplicate_attempt_ids[target] = None;
        true
    }

    pub fn arm_latency(&mut self, target: usize, value: u64, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_latency(target, value);
        if applied && value > 0 {
            self.latency_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }

    pub fn arm_jitter(&mut self, target: usize, value: u64, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_jitter(target, value);
        if applied && value > 0 {
            self.jitter_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }

    pub fn arm_bandwidth(&mut self, target: usize, value: u64, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_bandwidth(target, value);
        if applied && value > 0 {
            self.bandwidth_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }

    pub fn arm_loss(&mut self, target: usize, value: u32, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_loss_rate(target, value);
        if applied && value > 0 {
            self.loss_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }

    pub fn arm_corruption(
        &mut self,
        target: usize,
        value: u32,
        attempt_id: FaultAttemptId,
    ) -> bool {
        let applied = self.set_corruption_rate(target, value);
        if applied && value > 0 {
            self.corruption_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }

    pub fn arm_reorder(&mut self, target: usize, value: u64, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_reorder_window(target, value);
        if applied && value > 0 {
            self.reorder_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }

    pub fn arm_duplicate(&mut self, target: usize, value: u32, attempt_id: FaultAttemptId) -> bool {
        let applied = self.set_duplicate_rate(target, value);
        if applied && value > 0 {
            self.duplicate_attempt_ids[target] = Some(attempt_id);
        }
        applied
    }
}

// ═══════════════════════════════════════════════════════════════════════
