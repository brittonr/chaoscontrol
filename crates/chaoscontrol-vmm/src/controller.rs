//! Multi-VM simulation controller for deterministic distributed system testing.
//!
//! [`SimulationController`] orchestrates multiple [`DeterministicVm`] instances
//! in a single deterministic simulation, handling fault injection, network
//! routing, and deterministic scheduling.

use crate::snapshot::VmSnapshot;
use crate::vm::{DeterministicVm, VmConfig, VmError};
use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::schedule::FaultSchedule;
use log::{debug, info, warn};
use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
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

/// Virtual network with partition awareness and packet-level fault injection.
#[derive(Debug, Clone)]
pub struct NetworkFabric {
    /// Active partition rules — (side_a, side_b) pairs.
    pub partitions: Vec<(Vec<usize>, Vec<usize>)>,
    /// Per-VM latency injection in nanoseconds.
    pub latency: Vec<u64>,
    /// Messages in flight (not yet delivered).
    pub in_flight: Vec<NetworkMessage>,
    /// Per-VM packet loss rate in parts per million (0 = no loss).
    pub loss_rate_ppm: Vec<u32>,
    /// Per-VM packet corruption rate in parts per million (0 = no corruption).
    pub corruption_rate_ppm: Vec<u32>,
    /// Per-VM reorder window in ticks (0 = no reordering).
    pub reorder_window: Vec<u64>,
    /// Deterministic RNG for packet-level fault decisions.
    pub rng: ChaCha20Rng,
}

impl NetworkFabric {
    /// Create a new network fabric for `num_vms` VMs with the given seed.
    fn new(num_vms: usize, seed: u64) -> Self {
        let mut rng_key = [0u8; 32];
        // Derive network RNG from seed + a domain separator
        let derived = seed.wrapping_add(0x4E45_5446_4142);  // "NETFAB" as hex
        rng_key[..8].copy_from_slice(&derived.to_le_bytes());
        Self {
            partitions: Vec::new(),
            latency: vec![0; num_vms],
            in_flight: Vec::new(),
            loss_rate_ppm: vec![0; num_vms],
            corruption_rate_ppm: vec![0; num_vms],
            reorder_window: vec![0; num_vms],
            rng: ChaCha20Rng::from_seed(rng_key),
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
    /// Applies packet-level faults in order:
    /// 1. Partition check — drop if partitioned
    /// 2. Packet loss — drop with probability `loss_rate_ppm / 1_000_000`
    /// 3. Packet corruption — flip a random byte with probability
    /// 4. Packet reorder — randomize delivery time within window
    /// 5. Latency — add sender/receiver latency to delivery time
    pub fn send(
        &mut self,
        from: usize,
        to: usize,
        data: Vec<u8>,
        current_tick: u64,
    ) -> bool {
        // 1. Partition check
        if !self.can_reach(from, to) {
            debug!("Message from VM{} to VM{} dropped by partition", from, to);
            return false;
        }

        // 2. Packet loss — check sender's loss rate
        let sender_loss = self.loss_rate_ppm.get(from).copied().unwrap_or(0);
        let receiver_loss = self.loss_rate_ppm.get(to).copied().unwrap_or(0);
        let loss_rate = sender_loss.max(receiver_loss);
        if loss_rate > 0 {
            let roll = (self.rng.next_u64() % 1_000_000) as u32;
            if roll < loss_rate {
                debug!("Message from VM{} to VM{} dropped by packet loss ({}ppm)", from, to, loss_rate);
                return false;
            }
        }

        // 3. Packet corruption — flip a random byte
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
                debug!("Message from VM{} to VM{} corrupted at byte {}", from, to, byte_idx);
            }
        }

        // 4. Calculate delivery tick with latency
        let sender_latency = self.latency.get(from).copied().unwrap_or(0);
        let receiver_latency = self.latency.get(to).copied().unwrap_or(0);
        let latency_ticks = sender_latency.max(receiver_latency);
        let mut deliver_at_tick = current_tick + latency_ticks;

        // 5. Packet reorder — add random jitter within the reorder window
        let sender_reorder = self.reorder_window.get(from).copied().unwrap_or(0);
        let receiver_reorder = self.reorder_window.get(to).copied().unwrap_or(0);
        let reorder_win = sender_reorder.max(receiver_reorder);
        if reorder_win > 0 {
            let jitter = self.rng.next_u64() % (reorder_win + 1);
            deliver_at_tick += jitter;
            debug!("Message from VM{} to VM{} reordered by {} ticks", from, to, jitter);
        }

        self.in_flight.push(NetworkMessage {
            from,
            to,
            data,
            deliver_at_tick,
        });

        true
    }

    /// Add a network partition between two sides.
    fn add_partition(&mut self, side_a: Vec<usize>, side_b: Vec<usize>) {
        info!("Network partition: {:?} | {:?}", side_a, side_b);
        self.partitions.push((side_a, side_b));
    }

    /// Clear all partitions and packet-level faults (heal network).
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
            return Err(VmError::Snapshot("num_vms must be > 0".to_string()));
        }

        if config.kernel_path.is_empty() {
            return Err(VmError::Snapshot("kernel_path is required".to_string()));
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
            vm.load_kernel(
                &config.kernel_path,
                config.initrd_path.as_deref(),
            )?;

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
        info!("Running simulation for {} ticks (tick {}→{})", num_ticks, self.tick, stop_at);

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
                info!("PacketReorder: VM{} window {} ns ({} ticks)", target, window_ns, window_ticks);
                self.network.set_reorder_window(*target, window_ticks);
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
            Fault::DiskTornWrite { target, offset, bytes_written } => {
                warn!("DiskTornWrite fault not yet implemented: VM{}, offset {:#x}, {} bytes", 
                      target, offset, bytes_written);
            }
            Fault::DiskCorruption { target, offset, len } => {
                warn!("DiskCorruption fault not yet implemented: VM{}, offset {:#x}, {} bytes", 
                      target, offset, len);
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
            Fault::ProcessPause { target, duration_ns } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    // Convert duration_ns to ticks (1 tick = 1_000_000 ns), minimum 1 tick
                    let pause_ticks = (*duration_ns / 1_000_000).max(1);
                    let resume_at = self.tick + pause_ticks;
                    info!("ProcessPause: VM{} paused for {} ns ({} ticks), resume at tick {}", 
                          target, duration_ns, pause_ticks, resume_at);
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
            Fault::MemoryPressure { target, limit_bytes } => {
                if let Some(slot) = self.vms.get_mut(*target) {
                    info!("MemoryPressure: VM{} limited to {} bytes ({} MB)", 
                          target, limit_bytes, limit_bytes / (1024 * 1024));
                    slot.memory_limit_bytes = Some(*limit_bytes);
                }
            }
        }

        Ok(())
    }

    /// Schedule a VM restart at a future tick.
    fn schedule_restart(&mut self, target: usize, restart_at_tick: u64) -> Result<(), VmError> {
        if let Some(slot) = self.vms.get_mut(target) {
            info!("VM{} scheduled to restart at tick {}", target, restart_at_tick);
            slot.status = VmStatus::Restarting { restart_at_tick };
        }
        Ok(())
    }

    /// Schedule a paused VM to resume at a future tick.
    fn schedule_resume(&mut self, target: usize, resume_at_tick: u64) -> Result<(), VmError> {
        if let Some(slot) = self.vms.get_mut(target) {
            if slot.status == VmStatus::Paused {
                info!("VM{} scheduled to resume at tick {}", target, resume_at_tick);
                slot.status = VmStatus::Resuming { resume_at_tick };
            }
        }
        Ok(())
    }

    /// Restart a VM from its initial snapshot.
    fn restart_vm(&mut self, target: usize) -> Result<(), VmError> {
        let slot = self.vms.get_mut(target)
            .ok_or_else(|| VmError::Snapshot(format!("VM{} not found", target)))?;

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
            return Err(VmError::Snapshot(
                "Snapshot VM count mismatch".to_string()
            ));
        }

        self.tick = snapshot.tick;
        self.network = snapshot.network_state.clone();
        self.fault_engine.restore(&snapshot.fault_engine_snapshot);

        for (i, (vm_snap, status)) in snapshot.vm_snapshots.iter().enumerate() {
            self.vms[i].vm.restore(vm_snap)?;
            self.vms[i].status = *status;
        }

        info!("Restored simulation state from snapshot at tick {}", self.tick);
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
        assert!(!all_same, "Reorder window should produce varied delivery ticks");
    }

    #[test]
    fn test_network_heal_clears_packet_faults() {
        let mut fabric = NetworkFabric::new(2, 42);
        fabric.set_loss_rate(0, 500_000);
        fabric.set_corruption_rate(0, 200_000);
        fabric.set_reorder_window(0, 50);
        fabric.add_partition(vec![0], vec![1]);

        fabric.clear_partitions();

        assert!(fabric.partitions.is_empty());
        assert_eq!(fabric.loss_rate_ppm[0], 0);
        assert_eq!(fabric.corruption_rate_ppm[0], 0);
        assert_eq!(fabric.reorder_window[0], 0);
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

        status = VmStatus::Restarting { restart_at_tick: 100 };
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
            .at_ns(1_000_000, Fault::NetworkPartition {
                side_a: vec![0],
                side_b: vec![1],
            })
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
}
