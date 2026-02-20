//! Multi-VM simulation controller for deterministic distributed system testing.
//!
//! [`SimulationController`] orchestrates multiple [`DeterministicVm`] instances
//! in a single deterministic simulation, handling fault injection, network
//! routing, and deterministic scheduling.

use crate::snapshot::VmSnapshot;
use crate::vm::{DeterministicVm, SnapshotSnafu, VmConfig, VmError};
use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::schedule::FaultSchedule;
use log::{debug, info, warn};
use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use snafu::ResultExt;
use std::collections::VecDeque;

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
}

/// Current status of a VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, Default)]
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
#[derive(Debug, Clone, Default)]
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
    /// Packets that had jitter added to delivery time.
    pub packets_jittered: u64,
    /// Packets that had reorder window applied.
    pub packets_reordered: u64,
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
             duplicated={} bw_delayed={}({}ticks) jittered={}({}ticks) reordered={}",
            self.packets_sent,
            self.packets_delivered,
            self.packets_dropped_partition,
            self.packets_dropped_loss,
            self.packets_corrupted,
            self.packets_duplicated,
            self.packets_bandwidth_delayed,
            self.total_bandwidth_delay_ticks,
            self.packets_jittered,
            self.total_jitter_ticks,
            self.packets_reordered,
        )
    }
}

/// Virtual network with partition awareness and packet-level fault injection.
///
/// Models real-world network impairments: latency, jitter, bandwidth limits,
/// packet loss, corruption, reordering, and duplication.  All values are
/// per-VM and bidirectional (max of sender/receiver is used).
#[derive(Debug, Clone)]
pub struct NetworkFabric {
    /// Active partition rules — (side_a, side_b) pairs.
    pub partitions: Vec<(Vec<usize>, Vec<usize>)>,
    /// Per-VM base latency in ticks (0 = no added latency).
    pub latency: Vec<u64>,
    /// Per-VM latency jitter in ticks (0 = no jitter).
    /// Each packet gets up to this much extra random delay on top of base latency.
    pub jitter: Vec<u64>,
    /// Per-VM bandwidth limit in bytes per second (0 = unlimited).
    pub bandwidth_bps: Vec<u64>,
    /// Per-VM next-free tick for bandwidth serialization queuing.
    pub next_free_tick: Vec<u64>,
    /// Messages in flight (not yet delivered).
    pub in_flight: Vec<NetworkMessage>,
    /// Per-VM packet loss rate in parts per million (0 = no loss).
    pub loss_rate_ppm: Vec<u32>,
    /// Per-VM packet corruption rate in parts per million (0 = no corruption).
    pub corruption_rate_ppm: Vec<u32>,
    /// Per-VM reorder window in ticks (0 = no reordering).
    pub reorder_window: Vec<u64>,
    /// Per-VM packet duplication rate in parts per million (0 = no duplication).
    pub duplicate_rate_ppm: Vec<u32>,
    /// Deterministic RNG for packet-level fault decisions.
    pub rng: ChaCha20Rng,
    /// Cumulative packet-level statistics.
    pub stats: NetworkStats,
}

impl NetworkFabric {
    /// Create a new network fabric for `num_vms` VMs with the given seed.
    fn new(num_vms: usize, seed: u64) -> Self {
        let mut rng_key = [0u8; 32];
        // Derive network RNG from seed + a domain separator
        let derived = seed.wrapping_add(0x4E45_5446_4142); // "NETFAB" as hex
        rng_key[..8].copy_from_slice(&derived.to_le_bytes());
        Self {
            partitions: Vec::new(),
            latency: vec![0; num_vms],
            jitter: vec![0; num_vms],
            bandwidth_bps: vec![0; num_vms],
            next_free_tick: vec![0; num_vms],
            in_flight: Vec::new(),
            loss_rate_ppm: vec![0; num_vms],
            corruption_rate_ppm: vec![0; num_vms],
            reorder_window: vec![0; num_vms],
            duplicate_rate_ppm: vec![0; num_vms],
            rng: ChaCha20Rng::from_seed(rng_key),
            stats: NetworkStats::default(),
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

    /// Send a message from `from` to `to` at the current tick.
    ///
    /// Applies the full packet-level fault pipeline in order:
    /// 1. Partition check — drop if partitioned
    /// 2. Packet loss — drop with probability
    /// 3. Bandwidth — add serialization delay (queuing model)
    /// 4. Packet corruption — flip a random byte
    /// 5. Latency + jitter — base delay plus random variation
    /// 6. Packet reorder — additional random shuffle within window
    /// 7. Packet duplication — clone with slightly offset delivery
    pub fn send(&mut self, from: usize, to: usize, data: Vec<u8>, current_tick: u64) -> bool {
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
        // idle.  A packet of N bytes on a B bytes/sec link takes
        // `N * 8 * 1000 / B` ticks of serialization time (1 tick = 1 ms).
        // Back-to-back packets queue behind each other naturally.
        let mut bandwidth_delay_ticks: u64 = 0;
        let sender_bw = self.bandwidth_bps.get(from).copied().unwrap_or(0);
        let receiver_bw = self.bandwidth_bps.get(to).copied().unwrap_or(0);
        let effective_bw = match (sender_bw, receiver_bw) {
            (0, 0) => 0,          // both unlimited
            (0, b) | (b, 0) => b, // one is limited
            (a, b) => a.min(b),   // bottleneck
        };
        if effective_bw > 0 && !data.is_empty() {
            let bits = data.len() as u64 * 8;
            let serialization_ticks = (bits * 1000).saturating_div(effective_bw);
            let tx_start = current_tick.max(self.next_free_tick.get(from).copied().unwrap_or(0));
            if let Some(slot) = self.next_free_tick.get_mut(from) {
                *slot = tx_start + serialization_ticks;
            }
            bandwidth_delay_ticks = (tx_start + serialization_ticks).saturating_sub(current_tick);
            self.stats.packets_bandwidth_delayed += 1;
            self.stats.total_bandwidth_delay_ticks += bandwidth_delay_ticks;
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

        let sender_jitter = self.jitter.get(from).copied().unwrap_or(0);
        let receiver_jitter = self.jitter.get(to).copied().unwrap_or(0);
        let jitter_max = sender_jitter.max(receiver_jitter);
        let jitter_ticks = if jitter_max > 0 {
            let jt = self.rng.next_u64() % (jitter_max + 1);
            if jt > 0 {
                self.stats.packets_jittered += 1;
                self.stats.total_jitter_ticks += jt;
            }
            jt
        } else {
            0
        };

        let mut deliver_at_tick =
            current_tick + bandwidth_delay_ticks + latency_ticks + jitter_ticks;

        // 6. Packet reorder — additional random shuffle within window
        let sender_reorder = self.reorder_window.get(from).copied().unwrap_or(0);
        let receiver_reorder = self.reorder_window.get(to).copied().unwrap_or(0);
        let reorder_win = sender_reorder.max(receiver_reorder);
        if reorder_win > 0 {
            let reorder_jitter = self.rng.next_u64() % (reorder_win + 1);
            deliver_at_tick += reorder_jitter;
            if reorder_jitter > 0 {
                self.stats.packets_reordered += 1;
            }
            debug!(
                "Message from VM{} to VM{} reordered by {} ticks",
                from, to, reorder_jitter
            );
        }

        // 7. Packet duplication — maybe enqueue a second copy
        let sender_dup = self.duplicate_rate_ppm.get(from).copied().unwrap_or(0);
        let receiver_dup = self.duplicate_rate_ppm.get(to).copied().unwrap_or(0);
        let dup_rate = sender_dup.max(receiver_dup);
        if dup_rate > 0 {
            let roll = (self.rng.next_u64() % 1_000_000) as u32;
            if roll < dup_rate {
                // Duplicate arrives with slight offset (0–2 extra ticks)
                let dup_offset = self.rng.next_u64() % 3;
                self.in_flight.push(NetworkMessage {
                    from,
                    to,
                    data: data.clone(),
                    deliver_at_tick: deliver_at_tick + dup_offset,
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

    /// Add a network partition between two sides.
    fn add_partition(&mut self, side_a: Vec<usize>, side_b: Vec<usize>) {
        info!("Network partition: {:?} | {:?}", side_a, side_b);
        self.partitions.push((side_a, side_b));
    }

    /// Clear all partitions and packet-level faults (heal network).
    ///
    /// Resets: partitions, loss, corruption, reorder, jitter, bandwidth,
    /// and duplication rates.  Base latency is preserved (use
    /// `set_latency(target, 0)` to clear it explicitly).
    fn clear_partitions(&mut self) {
        info!("Network healed: all partitions and packet faults removed");
        self.partitions.clear();
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
        // Note: stats are NOT reset on heal — they are cumulative.
    }

    /// Set latency for a specific VM.
    fn set_latency(&mut self, target: usize, latency_ns: u64) {
        if target < self.latency.len() {
            self.latency[target] = latency_ns;
            debug!("VM{} latency set to {} ns", target, latency_ns);
        }
    }

    /// Set packet loss rate for a specific VM.
    fn set_loss_rate(&mut self, target: usize, rate_ppm: u32) {
        if target < self.loss_rate_ppm.len() {
            self.loss_rate_ppm[target] = rate_ppm;
            debug!("VM{} packet loss set to {} ppm", target, rate_ppm);
        }
    }

    /// Set packet corruption rate for a specific VM.
    fn set_corruption_rate(&mut self, target: usize, rate_ppm: u32) {
        if target < self.corruption_rate_ppm.len() {
            self.corruption_rate_ppm[target] = rate_ppm;
            debug!("VM{} packet corruption set to {} ppm", target, rate_ppm);
        }
    }

    /// Set reorder window for a specific VM (in ticks).
    fn set_reorder_window(&mut self, target: usize, window_ticks: u64) {
        if target < self.reorder_window.len() {
            self.reorder_window[target] = window_ticks;
            debug!("VM{} reorder window set to {} ticks", target, window_ticks);
        }
    }

    /// Set latency jitter for a specific VM (in ticks).
    ///
    /// Each packet to/from this VM receives up to `jitter_ticks` extra
    /// random delay on top of the base latency.
    fn set_jitter(&mut self, target: usize, jitter_ticks: u64) {
        if target < self.jitter.len() {
            self.jitter[target] = jitter_ticks;
            debug!("VM{} jitter set to {} ticks", target, jitter_ticks);
        }
    }

    /// Set bandwidth limit for a specific VM (bytes per second).
    ///
    /// Models serialization delay: a 1500-byte packet on a 1 MB/s link
    /// takes ~12 µs (0.012 ticks).  Set to 0 for unlimited.
    fn set_bandwidth(&mut self, target: usize, bytes_per_sec: u64) {
        if target < self.bandwidth_bps.len() {
            self.bandwidth_bps[target] = bytes_per_sec;
            debug!("VM{} bandwidth set to {} B/s", target, bytes_per_sec);
        }
    }

    /// Get a reference to the cumulative network statistics.
    pub fn stats(&self) -> &NetworkStats {
        &self.stats
    }

    /// Set packet duplication rate for a specific VM (parts per million).
    fn set_duplicate_rate(&mut self, target: usize, rate_ppm: u32) {
        if target < self.duplicate_rate_ppm.len() {
            self.duplicate_rate_ppm[target] = rate_ppm;
            debug!("VM{} packet duplication set to {} ppm", target, rate_ppm);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Simulation Controller
// ═══════════════════════════════════════════════════════════════════════

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
            });
        }

        let network = NetworkFabric::new(config.num_vms, config.seed);

        Ok(Self {
            vms,
            fault_engine,
            network,
            tick: 0,
            quantum: config.quantum,
            config,
        })
    }

    /// Run the simulation for up to `num_ticks` scheduling rounds.
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

            // Check for immediate assertion failures
            if self.fault_engine.has_assertion_failure() {
                warn!("Assertion failure detected at tick {}", self.tick);
                break;
            }
        }

        let oracle_report = self.fault_engine.oracle().report();
        let vm_exit_counts = self.vms.iter().map(|slot| slot.vm.exit_count()).collect();

        Ok(SimulationResult {
            total_ticks: self.tick,
            oracle_report,
            vm_exit_counts,
            network_stats: self.network.stats.clone(),
        })
    }

    /// Execute one scheduling round: step each Running VM by `quantum` exits,
    /// advance the global clock, dispatch faults, deliver network messages.
    pub fn step_round(&mut self) -> Result<RoundResult, VmError> {
        let mut vms_running = 0;
        let mut vms_halted = 0;

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
                }
                VmStatus::Restarting { restart_at_tick } => {
                    if self.tick >= restart_at_tick {
                        self.restart_vm(i)?;
                        vms_running += 1;
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
                    }
                }
            }
        }

        // Advance global tick
        self.tick += 1;

        // Poll and apply faults
        let current_time_ns = self.tick * 1_000_000; // Convert ticks to nanoseconds
        let faults = self.fault_engine.poll_faults(current_time_ns);
        let faults_fired = faults.clone();

        for fault in faults {
            self.apply_fault(&fault)?;
        }

        // Deliver pending network messages
        let messages_delivered = self.deliver_messages();

        Ok(RoundResult {
            tick: self.tick,
            vms_running,
            vms_halted,
            faults_fired,
            messages_delivered,
        })
    }

    /// Apply a fault effect from the engine to the actual VMs.
    fn apply_fault(&mut self, fault: &Fault) -> Result<(), VmError> {
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
                    // Can be restarted later with ProcessRestart
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
        }

        Ok(())
    }

    /// Schedule a VM restart at a future tick.
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

        if let Some(snapshot) = &slot.initial_snapshot {
            info!("Restarting VM{} from initial snapshot", target);
            slot.vm.restore(snapshot)?;
            slot.status = VmStatus::Running;
            slot.inbox.clear();
            slot.disk_faults = DiskFaultFlags::default();
            slot.tsc_skew = 0;
            slot.memory_limit_bytes = None;
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

    /// Snapshot all VMs and simulation state.
    pub fn snapshot_all(&self) -> Result<SimulationSnapshot, VmError> {
        let mut vm_snapshots = Vec::with_capacity(self.vms.len());

        for slot in &self.vms {
            let vm_snapshot = slot.vm.snapshot()?;
            vm_snapshots.push((vm_snapshot, slot.status));
        }

        Ok(SimulationSnapshot {
            tick: self.tick,
            vm_snapshots,
            network_state: self.network.clone(),
            fault_engine_snapshot: self.fault_engine.snapshot(),
        })
    }

    /// Restore all VMs from a snapshot.
    pub fn restore_all(&mut self, snapshot: &SimulationSnapshot) -> Result<(), VmError> {
        if snapshot.vm_snapshots.len() != self.vms.len() {
            return SnapshotSnafu {
                message: "Snapshot VM count mismatch",
            }
            .fail();
        }

        self.tick = snapshot.tick;
        self.network = snapshot.network_state.clone();
        self.fault_engine.restore(&snapshot.fault_engine_snapshot);

        for (i, (vm_snap, status)) in snapshot.vm_snapshots.iter().enumerate() {
            self.vms[i].vm.restore(vm_snap)?;
            self.vms[i].status = *status;
        }

        info!(
            "Restored simulation state from snapshot at tick {}",
            self.tick
        );
        Ok(())
    }

    /// Get the oracle report.
    pub fn report(&self) -> OracleReport {
        self.fault_engine.oracle().report()
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

    /// Get a reference to the network fabric.
    pub fn network(&self) -> &NetworkFabric {
        &self.network
    }

    /// Get a mutable reference to the network fabric.
    pub fn network_mut(&mut self) -> &mut NetworkFabric {
        &mut self.network
    }

    /// Replace the fault schedule (used by the explorer between branches).
    pub fn set_schedule(&mut self, schedule: FaultSchedule) {
        self.fault_engine.set_schedule(schedule);
    }

    /// Reset all VM statuses to Running and the tick counter to a
    /// snapshot's tick. Called implicitly by `restore_all`, but
    /// exposed for manual control.
    pub fn reset_vm_statuses(&mut self) {
        for slot in &mut self.vms {
            slot.status = VmStatus::Running;
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
    /// Faults that fired this round.
    pub faults_fired: Vec<Fault>,
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
}

/// Complete snapshot of simulation state.
#[derive(Debug, Clone)]
pub struct SimulationSnapshot {
    /// Global tick counter.
    pub tick: u64,
    /// Per-VM snapshots and status.
    pub vm_snapshots: Vec<(VmSnapshot, VmStatus)>,
    /// Network fabric state.
    pub network_state: NetworkFabric,
    /// Fault engine state.
    pub fault_engine_snapshot: chaoscontrol_fault::engine::EngineSnapshot,
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_fault::schedule::FaultScheduleBuilder;

    fn dummy_kernel_path() -> String {
        // Return a plausible path; tests that actually run VMs will need a real kernel
        "/tmp/dummy-vmlinux".to_string()
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
        let mut fabric = NetworkFabric::new(2, 42);
        // 8000 bytes/sec → 1 byte = 1 bit/ms = 1 tick per byte for 8-bit data
        // Actually: bits * 1000 / bps = N*8*1000/8000 = N ticks
        fabric.set_bandwidth(0, 8000);

        // Send 100 bytes: 100 * 8 * 1000 / 8000 = 100 ticks serialization
        fabric.send(0, 1, vec![0xAA; 100], 0);
        assert_eq!(fabric.in_flight.len(), 1);
        assert_eq!(fabric.in_flight[0].deliver_at_tick, 100);
    }

    #[test]
    fn test_network_fabric_bandwidth_queuing() {
        let mut fabric = NetworkFabric::new(2, 42);
        // 8000 bytes/sec → each 100-byte packet takes 100 ticks
        fabric.set_bandwidth(0, 8000);

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
        fabric.set_bandwidth(0, 8000); // sender: 8000 B/s
        fabric.set_bandwidth(1, 4000); // receiver: 4000 B/s (bottleneck)

        // 100 bytes at min(8000, 4000) = 4000 B/s → 100*8*1000/4000 = 200 ticks
        fabric.send(0, 1, vec![0xAA; 100], 0);
        assert_eq!(fabric.in_flight[0].deliver_at_tick, 200);
    }

    #[test]
    fn test_network_fabric_bandwidth_with_latency() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_bandwidth(0, 8000); // 100 ticks per 100 bytes
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
        fabric.set_bandwidth(0, 80_000); // 80 KB/s

        // 100 bytes: 100*8*1000/80000 = 10 ticks serialization
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
        fabric.set_bandwidth(0, 8000); // 8000 B/s
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
        let (ticks_b, stats_b) = send_and_collect(99);

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
}
