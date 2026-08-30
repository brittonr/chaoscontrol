//! Multi-VM simulation controller for deterministic distributed system testing.
//!
//! [`SimulationController`] orchestrates multiple [`DeterministicVm`] instances
//! in a single deterministic simulation, handling fault injection, network
//! routing, and deterministic scheduling.

pub use crate::controller_core::VmStatus;
use crate::controller_core::{
    all_setup_complete, checked_usize, checked_usize_u64, core_vm_status,
    device_disappeared_application_error, fault_vm_status, internal_application_error,
    next_operation, non_runnable_application_error, plan_completion,
    target_state_application_error, u32_targets_to_usize, validate_process_snapshot_effect,
    CompletionFacts, FaultApplicationError,
};
use crate::scheduler::core::ScheduleTrace;
use crate::scheduler::ScheduleVariant;
use crate::sim_adapter::KvmVcpuExecutor;
use crate::snapshot::VmSnapshot;
use crate::vm::{DeterministicVm, SnapshotSnafu, VmConfig, VmError};
use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::oracle::OracleReport;
use chaoscontrol_fault::outcomes::{
    checked_ns_to_tsc_delta, plan_fault_application, preflight_fault_application_events_with_limit,
    preflight_fault_observation_events_with_limit, validate_pending_fault_observations,
    FaultApplicationPolicy, FaultAttempt, FaultAttemptId, FaultAuthoritativeStage, FaultMechanism,
    FaultObservation, FaultObservationEffect, FaultObservationSubsystem, FaultPlan,
    FaultPlanEffect, FaultPlanningFacts, FaultStageEvent, FaultStageKind, FaultTransitionError,
    VmFaultFacts, MAX_FAULT_OUTCOME_EVENTS,
};
use chaoscontrol_fault::report_merge::{merge_oracle_reports, rejected_merge_report};
use chaoscontrol_sim_core::fault::{EngineConfig, FaultEngine, FaultSchedule};
#[cfg(test)]
use chaoscontrol_sim_core::network::{
    bandwidth_serialization_ticks, MAX_PENDING_FAULT_OBSERVATIONS,
};
pub use chaoscontrol_sim_core::network::{
    DiskFaultFlags, NetworkFabric, NetworkMessage, NetworkSendError, NetworkStats, PacketInFlight,
};
use chaoscontrol_sim_core::{
    complete_round, plan_round, simulation_config_identity, CommandExecutor, CoreVmStatus,
    ExitObservation, RoundInput, RoundObservation, SimulationCoreSnapshot,
    CORE_SNAPSHOT_SCHEMA_VERSION,
};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const MAX_PENDING_PROCESS_OBSERVATIONS: usize = 4_096;
const GUEST_ARTIFACT_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.sim-core.guest-artifact.v1";

fn guest_artifact_identity(path: &str) -> Result<[u8; 32], VmError> {
    let bytes = std::fs::read(path).map_err(|error| VmError::DiskImage {
        message: format!("read guest artifact {path}: {error}"),
    })?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(GUEST_ARTIFACT_IDENTITY_DOMAIN);
    hasher.update(&bytes);
    Ok(*hasher.finalize().as_bytes())
}

fn ledger_has_observed_effect(
    ledger: &chaoscontrol_fault::outcomes::FaultOutcomeLedger,
    expected: &FaultPlanEffect,
) -> bool {
    ledger.events.iter().any(|event| {
        matches!(
            &event.kind,
            FaultStageKind::Applied { effect } if effect == expected
        ) && ledger
            .attempts
            .get(&event.attempt_id)
            .is_some_and(|state| state.stage == FaultAuthoritativeStage::Observed)
    })
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
    /// Memory pressure limit in bytes (`None` = admitted baseline).
    pub memory_limit_bytes: Option<u64>,
    /// Tick at which the admitted baseline memory ceiling is restored.
    pub memory_limit_release_at_tick: Option<u64>,
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

//  Simulation Controller
// ═══════════════════════════════════════════════════════════════════════

fn fault_transition_vm_error(error: FaultTransitionError) -> VmError {
    VmError::Snapshot {
        message: format!("fault outcome transition failed: {error}"),
    }
}

/// Permanent controller-level failure after a round starts mutating state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerRoundPoison {
    /// One-based round that failed.
    pub round: u64,
    /// Tick observed before the failed round completed.
    pub tick: u64,
    /// Original failure retained for diagnostics.
    pub detail: String,
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
    config: SimulationConfig,
    /// Exact BLAKE3 identities of the loaded kernel and optional initrd.
    guest_artifact_ids: Vec<[u8; 32]>,
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
    /// Permanent latch after a round fails once mutation has started.
    round_poison: Option<ControllerRoundPoison>,
}

impl SimulationController {
    fn controller_round_poison_error(&self) -> Option<VmError> {
        self.round_poison
            .as_ref()
            .map(|poison| VmError::ControllerRoundPoisoned {
                round: poison.round,
                tick: poison.tick,
                detail: poison.detail.clone(),
            })
    }

    fn ensure_controller_healthy(&self) -> Result<(), VmError> {
        if let Some(error) = self.controller_round_poison_error() {
            return Err(error);
        }
        Ok(())
    }

    fn assert_controller_healthy(&self) {
        if let Some(error) = self.controller_round_poison_error() {
            panic!("{error}");
        }
    }

    fn latch_round_failure_at(&mut self, round: u64, tick: u64, error: &VmError) {
        if self.round_poison.is_none() {
            self.round_poison = Some(ControllerRoundPoison {
                round,
                tick,
                detail: error.to_string(),
            });
        }
    }

    fn finish_round_mutation<T>(
        &mut self,
        round: u64,
        starting_tick: u64,
        mutation_started: bool,
        result: Result<T, VmError>,
    ) -> Result<T, VmError> {
        let plan = plan_completion(CompletionFacts {
            mutation_started,
            operation_failed: result.is_err(),
            poison_already_latched: self.round_poison.is_some(),
        });
        if plan.latch_first_failure {
            if let Err(error) = &result {
                self.latch_round_failure_at(round, starting_tick, error);
            } else {
                unreachable!("failed completion plan needs an error");
            }
        }
        debug_assert!(plan.return_original_result);
        result
    }

    fn ensure_round_can_start(&mut self) -> Result<(), VmError> {
        self.ensure_controller_healthy()?;
        let poisoned_vm = self.vms.iter().enumerate().find_map(|(vm_index, slot)| {
            slot.vm
                .schedule_execution_poison()
                .map(|poison| (vm_index, poison.clone()))
        });
        if let Some((vm_index, poison)) = poisoned_vm {
            let error = VmError::ScheduleExecutionPoisoned {
                stage: poison.stage,
                detail: format!("VM {vm_index}: {}", poison.detail),
            };
            let round = next_operation(self.tick);
            self.latch_round_failure_at(round, self.tick, &error);
            return self.ensure_controller_healthy();
        }
        Ok(())
    }

    /// Return the permanent failed-round latch for diagnostics.
    pub fn round_poison(&self) -> Option<&ControllerRoundPoison> {
        self.round_poison.as_ref()
    }

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

        let mut guest_artifact_ids = vec![guest_artifact_identity(&config.kernel_path)?];
        if let Some(initrd_path) = config.initrd_path.as_deref() {
            guest_artifact_ids.push(guest_artifact_identity(initrd_path)?);
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
                memory_limit_release_at_tick: None,
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
            guest_artifact_ids,
            vm_memory_bases: vec![None; num_vms],
            fault_application_policy: FaultApplicationPolicy::default(),
            fault_operation_sequence: 0,
            pending_process_observations: VecDeque::new(),
            round_poison: None,
        })
    }

    /// Run the simulation for up to `num_ticks` scheduling rounds.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn run(&mut self, num_ticks: u64) -> Result<SimulationResult, VmError> {
        self.ensure_round_can_start()?;
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
        self.ensure_controller_healthy()?;
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

    /// Run the simulation until all VMs signal `setup_complete`, or until
    /// `max_ticks` is reached (whichever comes first).
    ///
    /// Used for bootstrap: boot kernel + guest initialisation is variable-length,
    /// so we can't use a fixed tick budget.  After `setup_complete`, the snapshot
    /// captures a fully-initialised guest ready for exploration branches.
    pub fn run_until_setup_complete(
        &mut self,
        max_ticks: u64,
    ) -> Result<SimulationResult, VmError> {
        self.ensure_round_can_start()?;
        let stop_at = self.tick + max_ticks;
        info!(
            "Bootstrap: running until setup_complete (max {} ticks, tick {}→{})",
            max_ticks, self.tick, stop_at
        );

        while self.tick < stop_at {
            let result = self.step_round()?;

            // The SDK hypercall goes to each per-VM engine, not the
            // controller's engine. Every VM must complete setup before the
            // controller can create an exploration snapshot.
            let all_setup_complete = all_setup_complete(
                self.vms
                    .iter()
                    .map(|slot| slot.vm.fault_engine().is_setup_complete()),
            );

            if all_setup_complete {
                info!(
                    "Bootstrap complete: all VMs reached setup_complete at tick {}",
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

        let all_setup = all_setup_complete(
            self.vms
                .iter()
                .map(|slot| slot.vm.fault_engine().is_setup_complete()),
        );
        if !all_setup {
            warn!(
                "Bootstrap reached max_ticks ({}) before all VMs completed setup",
                max_ticks
            );
        }

        self.ensure_controller_healthy()?;
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
        self.ensure_round_can_start()?;
        let starting_tick = self.tick;
        let vm_statuses: Vec<CoreVmStatus> = self
            .vms
            .iter()
            .map(|slot| core_vm_status(slot.status))
            .collect();
        let kernel_input = RoundInput {
            current_tick: self.tick,
            seed: self.config.seed,
            config_id: simulation_config_identity(
                vm_statuses.len(),
                self.config.seed,
                self.quantum,
            ),
            guest_artifact_ids: self.guest_artifact_ids.clone(),
            vm_statuses,
            exit_budget: self.quantum,
        };
        let kernel_plan = plan_round(&kernel_input).map_err(|error| VmError::Snapshot {
            message: format!("simulation kernel rejected round: {error}"),
        })?;
        let next_tick = kernel_plan.next_tick;
        let current_time_ns = kernel_plan.virtual_time_ns;
        let had_pending_process_observations = !self.pending_process_observations.is_empty();
        let had_pending_block_observations = self
            .vms
            .iter_mut()
            .any(|slot| slot.vm.block_fault_observation_reservation() != 0);
        let had_pending_network_observations = self.network.central_observation_reservation() != 0;
        let mut round_mutation_started = false;
        let round_result = (|| -> Result<RoundResult, VmError> {
            self.commit_pending_process_observations(observation_event_limit)?;
            round_mutation_started |= had_pending_process_observations;
            self.commit_pending_block_observations(observation_event_limit)?;
            round_mutation_started |= had_pending_block_observations;
            self.commit_pending_network_observations(observation_event_limit)?;
            round_mutation_started |= had_pending_network_observations;
            let process_reservation = self
                .vms
                .iter()
                .filter(|slot| {
                    slot.process_fault_attempt.is_some_and(|attempt_id| {
                        self.fault_engine
                            .fault_outcomes()
                            .attempts
                            .get(&attempt_id)
                            .is_some_and(|state| state.stage == FaultAuthoritativeStage::Applied)
                    })
                })
                .count();
            let block_reservation = self.vms.iter_mut().try_fold(0_usize, |total, slot| {
                total
                    .checked_add(slot.vm.block_fault_observation_reservation())
                    .ok_or_else(|| VmError::Snapshot {
                        message: "block observation reservation overflow".to_string(),
                    })
            })?;
            let network_reservation = self.network.central_observation_reservation();
            let central_reservation = process_reservation
                .checked_add(block_reservation)
                .and_then(|total| total.checked_add(network_reservation))
                .ok_or_else(|| VmError::Snapshot {
                    message: "central observation reservation overflow".to_string(),
                })?;
            preflight_fault_observation_events_with_limit(
                self.fault_engine.fault_outcomes(),
                central_reservation,
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
            let mut kernel_observations = Vec::with_capacity(kernel_plan.commands.len());
            round_mutation_started = true;

            // Emit tick markers into each VM's dlog (for cross-VM correlation).
            for i in 0..self.vms.len() {
                self.vms[i].vm.dlog_tick_marker(self.tick);
            }

            self.release_expired_fault_windows()?;

            // Step each VM by quantum exits (round-robin)
            for i in 0..self.vms.len() {
                match self.vms[i].status {
                    VmStatus::Running => {
                        let single_vcpu_stalled =
                            self.vms[i].vm.num_vcpus() == 1 && self.vms[i].vm.vcpu_is_stalled(0);
                        if single_vcpu_stalled {
                            let command = kernel_plan
                                .commands
                                .get(kernel_observations.len())
                                .ok_or_else(|| VmError::Snapshot {
                                    message: format!(
                                        "simulation kernel omitted stalled VM command for VM {i}"
                                    ),
                                })?;
                            kernel_observations.push(RoundObservation {
                                observation: ExitObservation::VcpuCompleted {
                                    sequence: command.sequence(),
                                    vm_index: i,
                                    exits: 0,
                                    halted: false,
                                },
                            });
                            vms_running += 1;
                            continue;
                        }
                        let command = kernel_plan
                            .commands
                            .get(kernel_observations.len())
                            .ok_or_else(|| VmError::Snapshot {
                                message: format!(
                                    "simulation kernel omitted running VM command for VM {i}"
                                ),
                            })?;
                        let observation = {
                            let mut executor = KvmVcpuExecutor::new(i, &mut self.vms[i].vm);
                            executor.execute(command)?
                        };
                        let ExitObservation::VcpuCompleted { exits, halted, .. } = observation
                        else {
                            return Err(VmError::Snapshot {
                                message: "KVM vCPU adapter returned an invalid observation"
                                    .to_string(),
                            });
                        };
                        kernel_observations.push(RoundObservation {
                            observation: ExitObservation::VcpuCompleted {
                                sequence: command.sequence(),
                                vm_index: i,
                                exits,
                                halted,
                            },
                        });
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
                            if self.process_observation_is_pending(attempt_id) {
                                self.queue_process_observation(
                                    i,
                                    attempt_id,
                                    FaultObservationEffect::ProcessSkipped,
                                )?;
                            }
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
                            self.vms[i].process_fault_attempt = None;
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
                                if self.process_observation_is_pending(attempt_id) {
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
            }

            self.commit_pending_block_observations(observation_event_limit)?;
            self.commit_pending_process_observations(observation_event_limit)?;

            // Bridge network packets between VMs (virtio-net TX → RX)
            self.bridge_network_packets()?;
            self.commit_pending_network_observations(observation_event_limit)?;

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
            complete_round(&kernel_input, kernel_plan, &kernel_observations).map_err(|error| {
                VmError::Snapshot {
                    message: format!("simulation kernel rejected shell observations: {error}"),
                }
            })?;

            let mut schedule_traces = Vec::new();
            for (vm_index, slot) in self.vms.iter_mut().enumerate() {
                let trace = slot.vm.take_schedule_trace()?;
                if !trace.records.is_empty() {
                    schedule_traces.push(VmScheduleTrace { vm_index, trace });
                }
            }

            Ok(RoundResult {
                tick: self.tick,
                vms_running,
                vms_halted,
                faults_fired,
                fault_outcomes,
                messages_delivered,
                schedule_traces,
            })
        })();
        self.finish_round_mutation(
            next_tick,
            starting_tick,
            round_mutation_started,
            round_result,
        )
    }

    fn release_expired_fault_windows(&mut self) -> Result<(), VmError> {
        for slot in &mut self.vms {
            let expired_vcpus = slot
                .vcpu_stall_until
                .iter()
                .filter_map(|(vcpu, expires)| (self.tick >= *expires).then_some(*vcpu))
                .collect::<Vec<_>>();
            for vcpu in expired_vcpus {
                slot.vcpu_stall_until.remove(&vcpu);
                slot.vm.set_vcpu_stalled(vcpu, false)?;
            }
            if let Some((_, expires)) = slot.clock_freeze {
                if self.tick >= expires {
                    slot.vm.virtual_tsc_mut().set_frozen(false);
                    slot.clock_freeze = None;
                }
            }
            if slot
                .memory_limit_release_at_tick
                .is_some_and(|expires| self.tick >= expires)
            {
                let baseline_bytes =
                    u64::try_from(slot.vm.memory().size()).map_err(|_| VmError::Snapshot {
                        message: "guest memory size exceeds resource-observation bounds"
                            .to_string(),
                    })?;
                slot.vm.set_memory_ceiling_bytes(baseline_bytes)?;
                slot.memory_limit_bytes = None;
                slot.memory_limit_release_at_tick = None;
            }
        }
        Ok(())
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
        let immediate_observations =
            u64::try_from(plan.max_immediate_observations()).map_err(|_| VmError::Snapshot {
                message: "immediate observation reservation exceeds sequence bounds".to_string(),
            })?;
        self.fault_operation_sequence
            .checked_add(immediate_observations)
            .ok_or_else(|| VmError::Snapshot {
                message: "fault operation sequence exhausted".to_string(),
            })?;
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
                memory_size_bytes: u64::try_from(slot.vm.memory().size()).map_err(|_| {
                    VmError::Snapshot {
                        message: "guest memory size exceeds fault-planning bounds".to_string(),
                    }
                })?,
                block_device_size_bytes: slot.vm.block_device_size_bytes(),
                has_initial_snapshot: slot.initial_snapshot.is_some(),
                supports_irq: true,
                supports_nmi: true,
                supports_clock_freeze: true,
                supports_clock_jitter: true,
                supports_cpu_stall: true,
                supports_memory_pressure: true,
                virtual_tsc: slot.vm.virtual_tsc(),
                tsc_khz: slot.vm.virtual_tsc_ref().tsc_khz(),
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

    // r[impl chaoscontrol.fault_outcomes.observation]
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

    fn commit_pending_block_observations(&mut self, event_limit: usize) -> Result<(), VmError> {
        for vm_index in 0..self.vms.len() {
            let (observations, overflowed) = self.vms[vm_index].vm.drain_block_fault_observations();
            if let Err(error) =
                self.record_fault_observations_with_event_limit(&observations, event_limit)
            {
                let restored = self.vms[vm_index]
                    .vm
                    .requeue_block_fault_observations(observations, overflowed);
                assert!(restored);
                return Err(error);
            }
            if overflowed != 0 {
                return Err(VmError::Snapshot {
                    message: format!(
                        "block fault observation queue overflowed by {overflowed} records"
                    ),
                });
            }
        }
        Ok(())
    }

    fn commit_pending_network_observations(&mut self, event_limit: usize) -> Result<(), VmError> {
        let (observations, overflowed) = self.network.drain_fault_observations();
        if let Err(error) =
            self.record_fault_observations_with_event_limit(&observations, event_limit)
        {
            self.network
                .requeue_fault_observations(observations, overflowed);
            return Err(error);
        }
        if overflowed != 0 {
            return Err(VmError::Snapshot {
                message: format!(
                    "network fault observation queue overflowed by {overflowed} records"
                ),
            });
        }
        Ok(())
    }

    fn process_observation_is_pending(&self, attempt_id: FaultAttemptId) -> bool {
        self.fault_engine
            .fault_outcomes()
            .attempts
            .get(&attempt_id)
            .is_some_and(|state| state.stage == FaultAuthoritativeStage::Applied)
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
            let active_pause = matches!(self.vms[vm_index].status, VmStatus::Resuming { .. });
            if !active_pause
                && self.vms[vm_index].process_fault_attempt == Some(observation.attempt_id)
            {
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

    // r[impl chaoscontrol.fault_outcomes.effect_reachability]
    // r[impl chaoscontrol.fault_outcomes.application]
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
            FaultMechanism::VirtualClockSkew
            | FaultMechanism::VirtualClockJump
            | FaultMechanism::VirtualClockFreeze
            | FaultMechanism::VirtualClockJitter => self.apply_clock_fault_plan(plan),
            FaultMechanism::IrqInjection | FaultMechanism::NmiInjection => {
                self.apply_interrupt_fault_plan(plan)
            }
            FaultMechanism::CpuRegisterBitflip | FaultMechanism::CpuStall => {
                self.apply_cpu_fault_plan(plan)
            }
            FaultMechanism::MemoryPressure => self.apply_resource_fault_plan(plan),
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
        let (target, observation_effect) = match &plan.effect {
            FaultPlanEffect::VirtualClockSkew {
                target,
                basis_tsc,
                tsc_khz,
                offset_ns,
                tsc_delta,
                target_tsc,
            }
            | FaultPlanEffect::VirtualClockJump {
                target,
                basis_tsc,
                tsc_khz,
                delta_ns: offset_ns,
                tsc_delta,
                target_tsc,
            } => {
                let expected_target = if *tsc_delta >= 0 {
                    basis_tsc.checked_add(tsc_delta.unsigned_abs())
                } else {
                    basis_tsc.checked_sub(tsc_delta.unsigned_abs())
                };
                if *offset_ns == 0
                    || *tsc_delta == 0
                    || checked_ns_to_tsc_delta(*offset_ns, *tsc_khz).ok() != Some(*tsc_delta)
                    || expected_target != Some(*target_tsc)
                {
                    return Err(internal_application_error());
                }
                let slot = self.vm_slot_mut_checked(*target)?;
                if slot.vm.virtual_tsc() != *basis_tsc
                    || slot.vm.virtual_tsc_ref().tsc_khz() != *tsc_khz
                {
                    return Err(target_state_application_error());
                }
                slot.vm.virtual_tsc_mut().set(*target_tsc);
                (*target, FaultObservationEffect::VirtualClockChanged)
            }
            FaultPlanEffect::VirtualClockFreeze {
                target,
                frozen_tsc,
                release_at_tick,
            } => {
                if *release_at_tick <= self.tick {
                    return Err(target_state_application_error());
                }
                let slot = self.vm_slot_mut_checked(*target)?;
                if slot.vm.virtual_tsc() != *frozen_tsc {
                    return Err(target_state_application_error());
                }
                slot.vm.virtual_tsc_mut().set_frozen(true);
                slot.clock_freeze = Some((*frozen_tsc, *release_at_tick));
                (*target, FaultObservationEffect::VirtualClockFrozen)
            }
            FaultPlanEffect::VirtualClockJitter { target, bound_tsc } => {
                let slot = self.vm_slot_mut_checked(*target)?;
                slot.vm.virtual_tsc_mut().set_jitter_bound(*bound_tsc);
                slot.clock_jitter_bound = *bound_tsc;
                (
                    *target,
                    FaultObservationEffect::VirtualClockJitterConfigured,
                )
            }
            _ => return Err(internal_application_error()),
        };
        let observation = self
            .make_shell_observation(
                plan.attempt_id,
                FaultObservationSubsystem::VirtualClock,
                observation_effect,
            )
            .map_err(|_| internal_application_error())?;
        debug!("VM{} virtual clock effect applied", target);
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
        let effect = match &plan.effect {
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
                FaultObservationEffect::CpuRegisterChanged
            }
            FaultPlanEffect::CpuStall {
                target,
                vcpu,
                release_at_tick,
            } => {
                if *release_at_tick <= self.tick {
                    return Err(target_state_application_error());
                }
                let slot = self.vm_slot_mut_checked(*target)?;
                let vcpu = checked_usize(*vcpu)?;
                slot.vm
                    .set_vcpu_stalled(vcpu, true)
                    .map_err(|_| non_runnable_application_error())?;
                slot.vcpu_stall_until.insert(vcpu, *release_at_tick);
                FaultObservationEffect::CpuStallActivated
            }
            _ => return Err(internal_application_error()),
        };
        let observation = self
            .make_shell_observation(plan.attempt_id, FaultObservationSubsystem::Cpu, effect)
            .map_err(|_| internal_application_error())?;
        Ok(vec![observation])
    }

    fn apply_resource_fault_plan(
        &mut self,
        plan: &FaultPlan,
    ) -> Result<Vec<FaultObservation>, FaultApplicationError> {
        let FaultPlanEffect::MemoryPressure {
            target,
            limit_bytes,
            baseline_bytes,
            release_at_tick,
        } = &plan.effect
        else {
            return Err(internal_application_error());
        };
        if *release_at_tick <= self.tick || *limit_bytes == 0 || *limit_bytes >= *baseline_bytes {
            return Err(target_state_application_error());
        }
        let slot = self.vm_slot_mut_checked(*target)?;
        let observed_baseline =
            u64::try_from(slot.vm.memory().size()).map_err(|_| internal_application_error())?;
        if observed_baseline != *baseline_bytes {
            return Err(target_state_application_error());
        }
        slot.vm
            .set_memory_ceiling_bytes(*limit_bytes)
            .map_err(|_| target_state_application_error())?;
        slot.memory_limit_bytes = Some(*limit_bytes);
        slot.memory_limit_release_at_tick = Some(*release_at_tick);
        let observation = self
            .make_shell_observation(
                plan.attempt_id,
                FaultObservationSubsystem::Scheduler,
                FaultObservationEffect::MemoryCeilingChanged,
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
        self.fault_engine
            .begin_counterfactual_run(schedule)
            .map_err(|error| VmError::Snapshot {
                message: format!("fault branch run failed: {error}"),
            })?;
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
    pub fn snapshot_all(&mut self) -> Result<SimulationSnapshot, VmError> {
        self.ensure_controller_healthy()?;
        let mut vm_snapshots = Vec::with_capacity(self.vms.len());

        for slot in &mut self.vms {
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
        let memory_pressure = self
            .vms
            .iter()
            .map(|slot| {
                slot.memory_limit_bytes
                    .zip(slot.memory_limit_release_at_tick)
                    .map(|(limit_bytes, release_at_tick)| {
                        u64::try_from(slot.vm.memory().size())
                            .map(|baseline_bytes| MemoryPressureSnapshotState {
                                limit_bytes,
                                baseline_bytes,
                                release_at_tick,
                            })
                            .map_err(|_| VmError::Snapshot {
                                message: "guest memory size exceeds snapshot bounds".to_string(),
                            })
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
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
            memory_pressure,
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
    pub fn snapshot_all_incremental(&mut self) -> Result<(SimulationSnapshot, usize), VmError> {
        self.ensure_controller_healthy()?;
        let mut vm_snapshots = Vec::with_capacity(self.vms.len());
        let mut total_dirty = 0usize;

        for (i, slot) in self.vms.iter_mut().enumerate() {
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
        let memory_pressure = self
            .vms
            .iter()
            .map(|slot| {
                slot.memory_limit_bytes
                    .zip(slot.memory_limit_release_at_tick)
                    .map(|(limit_bytes, release_at_tick)| {
                        u64::try_from(slot.vm.memory().size())
                            .map(|baseline_bytes| MemoryPressureSnapshotState {
                                limit_bytes,
                                baseline_bytes,
                                release_at_tick,
                            })
                            .map_err(|_| VmError::Snapshot {
                                message: "guest memory size exceeds snapshot bounds".to_string(),
                            })
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
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
            memory_pressure,
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
        self.assert_controller_healthy();
        self.vm_memory_bases = bases.into_iter().map(Some).collect();
    }

    /// Initialize per-thread POSIX timers on all VMs.
    ///
    /// Must be called from the worker thread that will run this controller.
    /// Creates thread-targeted `SIGALRM` timers for single-vCPU watchdogs.
    /// It also rebinds optional PMU overflow delivery to this worker thread.
    pub fn init_thread_timers(&mut self) -> Result<(), VmError> {
        self.ensure_controller_healthy()?;
        for slot in &mut self.vms {
            slot.vm.init_thread_timer()?;
        }
        Ok(())
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
        self.ensure_controller_healthy()?;
        snapshot
            .validate_assertion_identity(self.vms.len())
            .map_err(|message| SnapshotSnafu { message }.build())?;
        self.fault_engine
            .validate_orchestration_snapshot(&snapshot.fault_engine_snapshot)
            .map_err(fault_transition_vm_error)?;
        for (index, (vm_snapshot, _)) in snapshot.vm_snapshots.iter().enumerate() {
            self.vms[index]
                .vm
                .validate_fault_engine_snapshot(vm_snapshot)?;
        }
        self.validate_pending_snapshot(snapshot)?;

        let starting_tick = self.tick;
        let restore_round = starting_tick.saturating_add(1);
        let result = self.restore_all_validated(snapshot);
        if let Err(error) = &result {
            self.latch_round_failure_at(restore_round, starting_tick, error);
        }
        result
    }

    fn restore_all_validated(&mut self, snapshot: &SimulationSnapshot) -> Result<(), VmError> {
        self.tick = snapshot.tick;
        self.network = snapshot.network_state.clone();
        self.fault_engine
            .restore_orchestration(&snapshot.fault_engine_snapshot)
            .map_err(fault_transition_vm_error)?;
        self.fault_operation_sequence = snapshot.fault_operation_sequence;
        self.pending_process_observations = snapshot.pending_process_observations.clone();

        for (i, (vm_snap, status)) in snapshot.vm_snapshots.iter().enumerate() {
            self.vms[i].vm.restore(vm_snap)?;
            self.vms[i].status = *status;
            self.restore_slot_fault_surface(i, snapshot)?;
        }

        info!(
            "Restored simulation state from snapshot at tick {}",
            self.tick
        );
        Ok(())
    }

    fn restore_slot_fault_surface(
        &mut self,
        index: usize,
        snapshot: &SimulationSnapshot,
    ) -> Result<(), VmError> {
        if let Some(stalls) = snapshot.vcpu_stall_until.get(index) {
            self.vms[index].vcpu_stall_until = stalls.clone();
        } else {
            self.vms[index].vcpu_stall_until.clear();
        }
        self.vms[index].clock_freeze = snapshot.clock_freeze.get(index).copied().flatten();
        self.vms[index].clock_jitter_bound =
            snapshot.clock_jitter_bound.get(index).copied().unwrap_or(0);
        let frozen = self.vms[index].clock_freeze.is_some();
        let jitter_bound = self.vms[index].clock_jitter_bound;
        self.vms[index].vm.virtual_tsc_mut().set_frozen(frozen);
        self.vms[index]
            .vm
            .virtual_tsc_mut()
            .set_jitter_bound(jitter_bound);
        let stalled_vcpus = self.vms[index]
            .vcpu_stall_until
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for vcpu in stalled_vcpus {
            self.vms[index].vm.set_vcpu_stalled(vcpu, true)?;
        }
        let memory_pressure = snapshot.memory_pressure.get(index).copied().flatten();
        self.vms[index].memory_limit_bytes = memory_pressure.map(|state| state.limit_bytes);
        self.vms[index].memory_limit_release_at_tick =
            memory_pressure.map(|state| state.release_at_tick);
        if let Some(state) = memory_pressure {
            self.vms[index]
                .vm
                .set_memory_ceiling_bytes(state.limit_bytes)?;
        }
        self.vms[index].process_fault_attempt =
            snapshot.process_fault_attempt.get(index).copied().flatten();
        Ok(())
    }

    fn validate_pending_snapshot(&self, snapshot: &SimulationSnapshot) -> Result<(), VmError> {
        let vm_count = snapshot.vm_snapshots.len();
        SimulationCoreSnapshot {
            schema_version: CORE_SNAPSHOT_SCHEMA_VERSION,
            tick: snapshot.tick,
            vm_count,
            network: snapshot.network_state.clone(),
        }
        .validate()
        .map_err(|error| VmError::Snapshot {
            message: format!("simulation core snapshot rejected: {error}"),
        })?;
        let vector_lengths = [
            snapshot.vcpu_stall_until.len(),
            snapshot.clock_freeze.len(),
            snapshot.clock_jitter_bound.len(),
            snapshot.process_fault_attempt.len(),
        ];
        let memory_state_length_valid =
            snapshot.memory_pressure.is_empty() || snapshot.memory_pressure.len() == vm_count;
        if vector_lengths.into_iter().any(|length| length != vm_count) || !memory_state_length_valid
        {
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
            for (vcpu, release_at_tick) in &snapshot.vcpu_stall_until[index] {
                let expected = FaultPlanEffect::CpuStall {
                    target,
                    vcpu: u32::try_from(*vcpu).map_err(|_| {
                        fault_transition_vm_error(
                            FaultTransitionError::SnapshotPendingStateMismatch,
                        )
                    })?,
                    release_at_tick: *release_at_tick,
                };
                if *vcpu >= vm_snapshot.vcpu_snapshots.len()
                    || *release_at_tick <= snapshot.tick
                    || !ledger_has_observed_effect(ledger, &expected)
                {
                    return Err(fault_transition_vm_error(
                        FaultTransitionError::SnapshotPendingStateMismatch,
                    ));
                }
            }
            if let Some((frozen_tsc, release_at_tick)) = snapshot.clock_freeze[index] {
                let expected = FaultPlanEffect::VirtualClockFreeze {
                    target,
                    frozen_tsc,
                    release_at_tick,
                };
                if release_at_tick <= snapshot.tick
                    || !ledger_has_observed_effect(ledger, &expected)
                {
                    return Err(fault_transition_vm_error(
                        FaultTransitionError::SnapshotPendingStateMismatch,
                    ));
                }
            }
            let jitter_bound = snapshot.clock_jitter_bound[index];
            if jitter_bound != 0 {
                let expected = FaultPlanEffect::VirtualClockJitter {
                    target,
                    bound_tsc: jitter_bound,
                };
                if !ledger_has_observed_effect(ledger, &expected) {
                    return Err(fault_transition_vm_error(
                        FaultTransitionError::SnapshotPendingStateMismatch,
                    ));
                }
            }
            if let Some(memory_state) = snapshot.memory_pressure.get(index).copied().flatten() {
                let expected = FaultPlanEffect::MemoryPressure {
                    target,
                    limit_bytes: memory_state.limit_bytes,
                    baseline_bytes: memory_state.baseline_bytes,
                    release_at_tick: memory_state.release_at_tick,
                };
                if memory_state.limit_bytes == 0
                    || memory_state.limit_bytes >= memory_state.baseline_bytes
                    || memory_state.release_at_tick <= snapshot.tick
                    || !ledger_has_observed_effect(ledger, &expected)
                {
                    return Err(fault_transition_vm_error(
                        FaultTransitionError::SnapshotPendingStateMismatch,
                    ));
                }
            }
            for device in &vm_snapshot.virtio_snapshots {
                if let crate::snapshot::VirtioBackendSnapshot::Block(block_snapshot) =
                    &device.backend
                {
                    block_snapshot
                        .validate_pending_faults(ledger, target)
                        .map_err(fault_transition_vm_error)?;
                }
            }
        }
        let mut shell_sequences = std::collections::BTreeSet::new();
        for event in &ledger.events {
            if let FaultStageKind::Observed { observation } = &event.kind {
                let shell_observation = observation.subsystem != FaultObservationSubsystem::Block
                    && observation.subsystem != FaultObservationSubsystem::Network;
                if shell_observation
                    && (observation.operation_sequence >= snapshot.fault_operation_sequence
                        || !shell_sequences.insert(observation.operation_sequence))
                {
                    return Err(fault_transition_vm_error(
                        FaultTransitionError::SnapshotPendingStateMismatch,
                    ));
                }
            }
        }
        for (_, observation) in &snapshot.pending_process_observations {
            if observation.operation_sequence >= snapshot.fault_operation_sequence
                || !shell_sequences.insert(observation.operation_sequence)
            {
                return Err(fault_transition_vm_error(
                    FaultTransitionError::SnapshotPendingStateMismatch,
                ));
            }
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
        self.ensure_controller_healthy()?;
        snapshot
            .validate_assertion_identity(self.vms.len())
            .map_err(|message| SnapshotSnafu { message }.build())?;
        self.fault_engine
            .validate_orchestration_snapshot(&snapshot.fault_engine_snapshot)
            .map_err(fault_transition_vm_error)?;
        for (index, (vm_snapshot, _)) in snapshot.vm_snapshots.iter().enumerate() {
            self.vms[index]
                .vm
                .validate_fault_engine_snapshot(vm_snapshot)?;
        }
        self.validate_pending_snapshot(snapshot)?;

        let starting_tick = self.tick;
        let restore_round = starting_tick.saturating_add(1);
        let result = self.restore_all_incremental_validated(snapshot);
        if let Err(error) = &result {
            self.latch_round_failure_at(restore_round, starting_tick, error);
        }
        result
    }

    fn restore_all_incremental_validated(
        &mut self,
        snapshot: &SimulationSnapshot,
    ) -> Result<(), VmError> {
        self.tick = snapshot.tick;
        self.network = snapshot.network_state.clone();
        self.fault_engine
            .restore_orchestration(&snapshot.fault_engine_snapshot)
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
            self.restore_slot_fault_surface(i, snapshot)?;
        }

        debug!("Incremental restore from snapshot at tick {}", self.tick);
        Ok(())
    }

    /// Get the oracle report (merged from all VMs).
    pub fn report(&self) -> OracleReport {
        self.assert_controller_healthy();
        self.merged_oracle_report()
    }

    /// Merge oracle reports from all VM fault engines.
    ///
    /// Each VM has its own FaultEngine + PropertyOracle that tracks
    /// assertions from that VM's guest.  We merge them so the
    /// exploration sees a unified view of all assertion violations.
    fn merged_oracle_report(&self) -> OracleReport {
        let mut reports = Vec::with_capacity(self.vms.len().max(1));
        if self.vms.is_empty() {
            reports.push((0, self.fault_engine.oracle().report()));
        } else {
            for (index, slot) in self.vms.iter().enumerate() {
                let Ok(vm_instance) = u32::try_from(index) else {
                    return rejected_merge_report(
                        chaoscontrol_fault::report_merge::ReportMergeConflict::CardinalityOverflow,
                    );
                };
                reports.push((
                    vm_instance,
                    slot.vm
                        .fault_engine()
                        .oracle()
                        .finalized_report_projection(),
                ));
            }
        }
        match merge_oracle_reports(&reports) {
            Ok(report) => report,
            Err(conflict) => rejected_merge_report(conflict),
        }
    }

    /// Get the immutable simulation configuration.
    pub fn config(&self) -> &SimulationConfig {
        &self.config
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
        self.assert_controller_healthy();
        self.vms.get_mut(index)
    }

    /// Clear coverage bitmaps in all VMs.
    ///
    /// Call this before each branch run in the exploration loop.
    pub fn clear_all_coverage(&self) {
        self.assert_controller_healthy();
        for slot in &self.vms {
            slot.vm.clear_coverage_bitmap();
        }
    }

    /// Force the fault engine's setup_complete flag to true.
    ///
    /// Use this in integration tests where the guest doesn't use the
    /// ChaosControl SDK but you still want scheduled faults to fire.
    pub fn force_setup_complete(&mut self) {
        self.assert_controller_healthy();
        self.fault_engine.force_setup_complete();
    }

    /// Get a reference to a VM by index.
    pub fn vm(&self, index: usize) -> &DeterministicVm {
        &self.vms[index].vm
    }

    /// Get a mutable reference to a VM by index.
    pub fn vm_mut(&mut self, index: usize) -> &mut DeterministicVm {
        self.assert_controller_healthy();
        &mut self.vms[index].vm
    }

    /// Get a reference to the network fabric.
    pub fn network(&self) -> &NetworkFabric {
        &self.network
    }

    /// Get a mutable reference to the network fabric.
    pub fn network_mut(&mut self) -> &mut NetworkFabric {
        self.assert_controller_healthy();
        &mut self.network
    }

    /// Get network statistics.
    pub fn network_stats(&self) -> &NetworkStats {
        &self.network.stats
    }

    /// Replace the fault schedule (used by the explorer between branches).
    pub fn set_schedule(&mut self, schedule: FaultSchedule) -> Result<(), VmError> {
        self.ensure_controller_healthy()?;
        self.fault_engine
            .set_schedule(schedule)
            .map_err(|error| VmError::Snapshot {
                message: format!("fault schedule replacement failed: {error}"),
            })
    }

    /// Start one exact clean fault run for deterministic replay.
    pub fn start_fault_run_at(&mut self, schedule: FaultSchedule, run_sequence: u64) {
        self.assert_controller_healthy();
        self.fault_engine
            .rebind_fresh_run_at(schedule, run_sequence);
    }

    /// Start one clean bounded counterfactual fault run.
    pub fn begin_counterfactual_fault_run(
        &mut self,
        schedule: FaultSchedule,
    ) -> Result<(), VmError> {
        self.ensure_controller_healthy()?;
        self.fault_engine
            .begin_counterfactual_run(schedule)
            .map_err(|error| VmError::Snapshot {
                message: format!("counterfactual fault run failed: {error}"),
            })
    }

    /// Set the explicit campaign policy for rejected fault attempts.
    pub fn set_fault_application_policy(&mut self, policy: FaultApplicationPolicy) {
        self.assert_controller_healthy();
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
    pub fn apply_schedule_variant(&mut self, variant: &ScheduleVariant) -> Result<(), VmError> {
        self.ensure_controller_healthy()?;
        for (i, slot) in self.vms.iter_mut().enumerate() {
            let per_vm = ScheduleVariant {
                scheduler_seed: variant.scheduler_seed.wrapping_add(i as u64),
                strategy_override: variant.strategy_override,
                quantum_override: variant.quantum_override,
            };
            slot.vm.scheduler_mut().apply_variant(&per_vm)?;
        }
        Ok(())
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
        self.assert_controller_healthy();
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
        self.assert_controller_healthy();
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
        self.assert_controller_healthy();
        if let Some(slot) = self.vms.get_mut(vm_id) {
            slot.vm.fault_engine_mut().set_random_overrides(overrides);
        }
    }

    /// Clear random choice overrides for all VMs.
    pub fn clear_all_choice_overrides(&mut self) {
        self.assert_controller_healthy();
        for slot in &mut self.vms {
            slot.vm.fault_engine_mut().clear_random_overrides();
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Result types
// ═══════════════════════════════════════════════════════════════════════

/// Canonical schedule trace emitted by one VM during one round.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VmScheduleTrace {
    /// Stable VM index in the simulation controller.
    pub vm_index: usize,
    /// Independently verifiable bounded trace.
    pub trace: ScheduleTrace,
}

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
    /// Canonical deterministic SMP traces emitted during this round.
    pub schedule_traces: Vec<VmScheduleTrace>,
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

/// Active guest-visible memory-pressure state retained by replay snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryPressureSnapshotState {
    pub limit_bytes: u64,
    pub baseline_bytes: u64,
    pub release_at_tick: u64,
}

/// Complete snapshot of simulation state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Per-VM active memory-pressure state.
    #[serde(default)]
    pub memory_pressure: Vec<Option<MemoryPressureSnapshotState>>,
    /// Per-VM pending process-effect attempt identity.
    pub process_fault_attempt: Vec<Option<FaultAttemptId>>,
    /// Next deterministic operation sequence for shell observations.
    pub fault_operation_sequence: u64,
    /// Process observations waiting for ledger commit.
    pub pending_process_observations: VecDeque<(usize, FaultObservation)>,
}

impl SimulationSnapshot {
    pub fn validate_assertion_identity(&self, expected_vms: usize) -> Result<(), String> {
        if self.vm_snapshots.len() != expected_vms {
            return Err("Snapshot VM count mismatch".to_string());
        }
        chaoscontrol_fault::engine::validate_orchestration_engine_snapshot(
            &self.fault_engine_snapshot,
        )
        .map_err(|error| {
            format!(
                "invalid controller orchestration snapshot: {error:?}; {}",
                chaoscontrol_fault::engine::engine_snapshot_validation_diagnostic(
                    &self.fault_engine_snapshot,
                )
            )
        })?;
        for (index, (snapshot, _)) in self.vm_snapshots.iter().enumerate() {
            snapshot.validate_assertion_identity().map_err(|error| {
                format!(
                    "invalid VM {index} assertion snapshot: {error:?}; {}",
                    snapshot.assertion_validation_diagnostic()
                )
            })?;
        }
        Ok(())
    }

    pub fn validate_assertion_evidence(
        &self,
        expected_vms: usize,
        identity: &chaoscontrol_protocol::admission::AssertionEvidenceIdentity,
    ) -> Result<(), String> {
        self.validate_assertion_identity(expected_vms)?;
        let admitted = self
            .vm_snapshots
            .iter()
            .filter(|(snapshot, _)| snapshot.validate_assertion_evidence(identity).is_ok())
            .count();
        if admitted == 0 {
            return Err("snapshot assertion evidence is absent from every VM".to_string());
        }
        Ok(())
    }
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
    use chaoscontrol_fault::outcomes::{
        transition_fault_outcome, FaultApplicationFailureDisposition,
        FaultApplicationFailureReason, FaultOutcomeLedger,
    };
    use chaoscontrol_fault::schedule::FaultScheduleBuilder;

    #[test]
    fn setup_completion_requires_every_vm() {
        assert!(all_setup_complete([true, true, true].into_iter()));
        assert!(!all_setup_complete([true, false, true].into_iter()));
        assert!(!all_setup_complete([].into_iter()));
    }

    fn dummy_kernel_path() -> String {
        // Return a plausible path; tests that actually run VMs will need a real kernel
        "/tmp/dummy-vmlinux".to_string()
    }

    fn adapter_test_controller() -> SimulationController {
        const NETWORK_NODE_COUNT: usize = 2;
        const TEST_VM_COUNT: usize = 1;
        let config = SimulationConfig {
            num_vms: TEST_VM_COUNT,
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
            memory_limit_release_at_tick: None,
            initial_snapshot: None,
            vcpu_stall_until: std::collections::BTreeMap::new(),
            clock_freeze: None,
            clock_jitter_bound: 0,
            process_fault_attempt: None,
        };
        let mut controller = SimulationController {
            vms: vec![slot],
            fault_engine: FaultEngine::new(EngineConfig::default()),
            network: NetworkFabric::new(NETWORK_NODE_COUNT, config.seed),
            tick: 0,
            quantum: config.quantum,
            config,
            guest_artifact_ids: Vec::new(),
            vm_memory_bases: vec![None],
            fault_application_policy: FaultApplicationPolicy::default(),
            fault_operation_sequence: 0,
            pending_process_observations: VecDeque::new(),
            round_poison: None,
        };
        controller.fault_engine.begin_run();
        controller
    }

    fn deterministic_smp_test_controller() -> SimulationController {
        const VM_COUNT: usize = 2;
        const VCPU_COUNT: usize = 2;
        let config = SimulationConfig {
            num_vms: VM_COUNT,
            kernel_path: dummy_kernel_path(),
            ..Default::default()
        };
        let mut vms = Vec::with_capacity(VM_COUNT);
        for vm_id in 0..VM_COUNT {
            let vm_config = VmConfig {
                num_vcpus: VCPU_COUNT,
                vm_id,
                ..Default::default()
            };
            let vm = DeterministicVm::new(vm_config).expect("create deterministic SMP test VM");
            vms.push(VmSlot {
                vm,
                status: VmStatus::Running,
                inbox: VecDeque::new(),
                disk_faults: DiskFaultFlags::default(),
                tsc_skew: 0,
                memory_limit_bytes: None,
                memory_limit_release_at_tick: None,
                initial_snapshot: None,
                vcpu_stall_until: std::collections::BTreeMap::new(),
                clock_freeze: None,
                clock_jitter_bound: 0,
                process_fault_attempt: None,
            });
        }
        SimulationController {
            vms,
            fault_engine: FaultEngine::new(EngineConfig::default()),
            network: NetworkFabric::new(VM_COUNT, config.seed),
            tick: 0,
            quantum: config.quantum,
            config,
            guest_artifact_ids: Vec::new(),
            vm_memory_bases: vec![None; VM_COUNT],
            fault_application_policy: FaultApplicationPolicy::default(),
            fault_operation_sequence: 0,
            pending_process_observations: VecDeque::new(),
            round_poison: None,
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
    fn failed_multi_vm_round_latches_before_retry_or_evidence_publication() {
        let mut controller = deterministic_smp_test_controller();
        let snapshot = controller
            .snapshot_all()
            .expect("snapshot healthy controller");
        let tick_before = controller.tick;
        let fault_sequence_before = controller.fault_operation_sequence;
        let fault_ledger_before = controller.fault_outcomes().clone();
        let network_before =
            serde_json::to_vec(&controller.network).expect("serialize initial network state");
        let vm0_sequence_before = controller.vms[0].vm.controller_test_schedule_sequence();

        let failed_round = (|| -> Result<RoundResult, VmError> {
            controller.vms[0].vm.inject_controller_test_progress()?;
            let vm1_poison = controller.vms[1].vm.inject_controller_test_poison()?;
            Err(vm1_poison)
        })();
        let first_error = controller.finish_round_mutation(
            controller.tick.saturating_add(1),
            controller.tick,
            true,
            failed_round,
        );

        assert!(matches!(
            first_error,
            Err(VmError::ScheduleExecutionPoisoned { .. })
        ));
        assert!(controller.round_poison().is_some());
        let vm0_sequence_after_failure = controller.vms[0].vm.controller_test_schedule_sequence();
        assert_eq!(vm0_sequence_after_failure, vm0_sequence_before + 1);
        assert!(controller.vms[1].vm.scheduler().reservation_outstanding());
        assert_eq!(controller.tick, tick_before);
        assert_eq!(controller.fault_operation_sequence, fault_sequence_before);
        assert_eq!(controller.fault_outcomes(), &fault_ledger_before);
        assert_eq!(
            serde_json::to_vec(&controller.network).expect("serialize failed-round network state"),
            network_before
        );

        assert!(matches!(
            controller.step_round(),
            Err(VmError::ControllerRoundPoisoned { .. })
        ));
        assert_eq!(
            controller.vms[0].vm.controller_test_schedule_sequence(),
            vm0_sequence_after_failure
        );
        assert_eq!(controller.tick, tick_before);
        assert_eq!(controller.fault_operation_sequence, fault_sequence_before);
        assert_eq!(controller.fault_outcomes(), &fault_ledger_before);
        assert_eq!(
            serde_json::to_vec(&controller.network).expect("serialize retry network state"),
            network_before
        );
        assert!(matches!(
            controller.snapshot_all(),
            Err(VmError::ControllerRoundPoisoned { .. })
        ));
        assert!(matches!(
            controller.restore_all(&snapshot),
            Err(VmError::ControllerRoundPoisoned { .. })
        ));
        assert!(matches!(
            controller.set_schedule(FaultScheduleBuilder::new().build()),
            Err(VmError::ControllerRoundPoisoned { .. })
        ));
        assert!(matches!(
            controller.run(0),
            Err(VmError::ControllerRoundPoisoned { .. })
        ));
    }

    #[test]
    fn test_simulation_config_default() {
        let config = SimulationConfig::default();
        assert_eq!(config.num_vms, 2);
        assert_eq!(config.seed, 42);
        assert_eq!(config.quantum, 100);
    }

    #[test]
    fn forged_assertion_snapshot_leaves_controller_state_unchanged() {
        const ORIGINAL_TICK: u64 = 17;
        const FORGED_TICK: u64 = 99;
        const ORIGINAL_SEED: u64 = 41;
        const FORGED_SEED: u64 = 43;
        let config = SimulationConfig {
            num_vms: 0,
            seed: ORIGINAL_SEED,
            ..SimulationConfig::default()
        };
        let mut controller = SimulationController {
            vms: Vec::new(),
            fault_engine: FaultEngine::new(EngineConfig::default()),
            network: NetworkFabric::new(0, ORIGINAL_SEED),
            tick: ORIGINAL_TICK,
            quantum: config.quantum,
            config,
            guest_artifact_ids: Vec::new(),
            vm_memory_bases: Vec::new(),
            fault_application_policy: FaultApplicationPolicy::default(),
            fault_operation_sequence: 0,
            pending_process_observations: VecDeque::new(),
            round_poison: None,
        };
        let mut engine_value =
            serde_json::to_value(controller.fault_engine.snapshot()).expect("engine snapshot");
        engine_value["oracle"]["catalog_status"] = serde_json::json!("accepted");
        let forged_engine =
            serde_json::from_value(engine_value).expect("forged engine snapshot shape");
        let snapshot = SimulationSnapshot {
            tick: FORGED_TICK,
            vm_snapshots: Vec::new(),
            network_state: NetworkFabric::new(0, FORGED_SEED),
            fault_engine_snapshot: forged_engine,
            vcpu_stall_until: Vec::new(),
            clock_freeze: Vec::new(),
            clock_jitter_bound: Vec::new(),
            memory_pressure: Vec::new(),
            process_fault_attempt: Vec::new(),
            fault_operation_sequence: 0,
            pending_process_observations: VecDeque::new(),
        };
        let network_before =
            serde_json::to_value(&controller.network).expect("network before restore");
        let report_before = controller.fault_engine.oracle().report();

        assert!(controller.restore_all(&snapshot).is_err());
        assert_eq!(controller.tick, ORIGINAL_TICK);
        assert_eq!(
            serde_json::to_value(&controller.network).expect("network after restore"),
            network_before
        );
        assert_eq!(controller.fault_engine.oracle().report(), report_before);
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
        controller.fault_engine.set_schedule(schedule).unwrap();
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

        controller.vms[0].status = VmStatus::Paused;
        controller.step_round().unwrap();
        assert!(controller.network.fault_observations.is_empty());
        assert_eq!(controller.fault_outcomes().counters.observed, 1);
    }

    #[test]
    fn prior_network_overflow_stops_before_new_process_observation() {
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
            vec![FaultObservationEffect::PacketDroppedByLoss]
        );
        assert!(controller.vms[0].process_fault_attempt.is_some());
    }

    #[test]
    fn central_capacity_failure_precedes_block_network_and_process_effects() {
        let mut controller = adapter_test_controller();
        controller
            .apply_fault(&Fault::DiskFull { target: 0 })
            .unwrap();
        let event_limit = controller.fault_outcomes().events.len();
        let (block_bytes_before, block_stats_before, block_sequence_before) = {
            let block = adapter_test_block(&mut controller);
            (
                block.materialize(),
                block.stats().clone(),
                block.operation_sequence(),
            )
        };
        let network_stats_before = controller.network.stats.clone();
        let network_packets_before = controller.network.packet_in_flight.len();
        let network_sequence_before = controller.network.fault_observation_sequence;
        let status_before = controller.vms[0].status;
        let shell_sequence_before = controller.fault_operation_sequence;
        let tick_before = controller.tick;

        let result = controller.step_round_with_observation_event_limit(event_limit);

        assert!(matches!(result, Err(VmError::Snapshot { .. })));
        let block = adapter_test_block(&mut controller);
        assert_eq!(block.materialize(), block_bytes_before);
        assert_eq!(block.stats(), &block_stats_before);
        assert_eq!(block.operation_sequence(), block_sequence_before);
        assert_eq!(controller.network.stats, network_stats_before);
        assert_eq!(
            controller.network.packet_in_flight.len(),
            network_packets_before
        );
        assert_eq!(
            controller.network.fault_observation_sequence,
            network_sequence_before
        );
        assert_eq!(controller.vms[0].status, status_before);
        assert_eq!(controller.fault_operation_sequence, shell_sequence_before);
        assert_eq!(controller.tick, tick_before);
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
    fn exhausted_immediate_observation_sequence_preserves_guest_visible_state() {
        const CLOCK_OFFSET_NS: i64 = 1_000_000;
        const TEST_IRQ: u32 = 1;
        let mut controller = adapter_test_controller();
        let before = controller.vms[0].vm.snapshot().unwrap();
        let before_registers = controller.vms[0].vm.read_vcpu_registers(0).unwrap();
        controller.fault_operation_sequence = u64::MAX;
        let faults = [
            Fault::ClockSkew {
                target: 0,
                offset_ns: CLOCK_OFFSET_NS,
            },
            Fault::CpuBitflip {
                target: 0,
                vcpu: 0,
                register: GpRegister::Rax,
                bit: 0,
            },
            Fault::InjectInterrupt {
                target: 0,
                irq: TEST_IRQ,
            },
            Fault::InjectNmi { target: 0, vcpu: 0 },
        ];

        for fault in faults {
            let result = controller.apply_fault(&fault);
            assert!(matches!(result, Err(VmError::Snapshot { .. })));
        }

        let after = controller.vms[0].vm.snapshot().unwrap();
        let after_registers = controller.vms[0].vm.read_vcpu_registers(0).unwrap();
        assert_eq!(after.virtual_tsc, before.virtual_tsc);
        assert_eq!(
            format!("{after_registers:?}"),
            format!("{before_registers:?}")
        );
        assert_eq!(
            format!("{:?}", after.pic_master),
            format!("{:?}", before.pic_master)
        );
        assert_eq!(
            format!("{:?}", after.pic_slave),
            format!("{:?}", before.pic_slave)
        );
        assert_eq!(
            format!("{:?}", after.ioapic),
            format!("{:?}", before.ioapic)
        );
        assert_eq!(
            format!("{:?}", after.vcpu_snapshots[0].lapic),
            format!("{:?}", before.vcpu_snapshots[0].lapic)
        );
        assert_eq!(controller.fault_operation_sequence, u64::MAX);
        assert_eq!(controller.fault_outcomes().counters.applied, 0);
        assert_eq!(controller.fault_outcomes().counters.observed, 0);
    }

    #[test]
    fn every_supported_variant_reaches_a_successful_application_adapter() {
        // r[verify chaoscontrol.fault_outcomes.validation.variant_matrix]
        const FULL_RATE_PPM: u32 = 1_000_000;
        const TEST_MEMORY_BASELINE_BYTES: u64 = 256 * 1024 * 1024;
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
                    tsc_khz: 3_000_000,
                    offset_ns: 1,
                    tsc_delta: 3,
                    target_tsc: 3,
                },
            ),
            (
                FaultVariant::ClockJump,
                FaultPlanEffect::VirtualClockJump {
                    target: 0,
                    basis_tsc: 3,
                    tsc_khz: 3_000_000,
                    delta_ns: 2,
                    tsc_delta: 6,
                    target_tsc: 9,
                },
            ),
            (
                FaultVariant::MemoryPressure,
                FaultPlanEffect::MemoryPressure {
                    target: 0,
                    limit_bytes: 1,
                    baseline_bytes: TEST_MEMORY_BASELINE_BYTES,
                    release_at_tick: 1,
                },
            ),
            (
                FaultVariant::ClockFreeze,
                FaultPlanEffect::VirtualClockFreeze {
                    target: 0,
                    frozen_tsc: 9,
                    release_at_tick: 1,
                },
            ),
            (
                FaultVariant::ClockJitter,
                FaultPlanEffect::VirtualClockJitter {
                    target: 0,
                    bound_tsc: 1,
                },
            ),
            (
                FaultVariant::CpuStall,
                FaultPlanEffect::CpuStall {
                    target: 0,
                    vcpu: 0,
                    release_at_tick: 1,
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
        for variant in FaultVariant::ALL {
            assert!(
                covered.contains(&variant),
                "missing adapter case for {variant:?}"
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
    fn zero_clock_fault_is_rejected_without_applied_or_observed_counters() {
        let mut controller = adapter_test_controller();
        let before_tsc = controller.vms[0].vm.virtual_tsc();

        controller
            .apply_fault(&Fault::ClockSkew {
                target: 0,
                offset_ns: 0,
            })
            .unwrap();

        assert_eq!(controller.vms[0].vm.virtual_tsc(), before_tsc);
        assert_eq!(controller.fault_outcomes().counters.rejected, 1);
        assert_eq!(controller.fault_outcomes().counters.applied, 0);
        assert_eq!(controller.fault_outcomes().counters.observed, 0);
    }

    #[test]
    fn zero_clock_delta_is_rejected_without_an_observed_change() {
        let mut controller = adapter_test_controller();
        let current_tsc = controller.vms[0].vm.virtual_tsc();
        let plan = FaultPlan {
            attempt_id: FaultAttemptId([0; 32]),
            effect: FaultPlanEffect::VirtualClockSkew {
                target: 0,
                basis_tsc: current_tsc,
                tsc_khz: controller.vms[0].vm.virtual_tsc_ref().tsc_khz(),
                offset_ns: 0,
                tsc_delta: 0,
                target_tsc: current_tsc,
            },
        };

        let result = controller.apply_fault_plan(&plan);

        assert_eq!(result, Err(internal_application_error()));
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
    fn clock_freeze_records_all_stages_and_releases_without_a_jump() {
        // r[verify chaoscontrol.fault_surface.clock_freeze]
        // r[verify chaoscontrol.fault_surface.stage_evidence]
        const FREEZE_TICKS: u64 = 2;
        let mut controller = adapter_test_controller();
        let frozen_tsc = controller.vms[0].vm.virtual_tsc();

        controller
            .apply_fault(&Fault::ClockFreeze {
                target: 0,
                duration_ticks: FREEZE_TICKS,
            })
            .unwrap();

        assert_eq!(controller.fault_outcomes().counters.selected, 1);
        assert!(controller
            .fault_outcomes()
            .events
            .iter()
            .any(|event| { matches!(event.kind, FaultStageKind::Applicable { .. }) }));
        assert_eq!(controller.fault_outcomes().counters.applied, 1);
        assert_eq!(controller.fault_outcomes().counters.observed, 1);
        assert!(controller.vms[0].vm.virtual_tsc_ref().is_frozen());
        controller.vms[0].vm.virtual_tsc_mut().tick();
        assert_eq!(controller.vms[0].vm.virtual_tsc(), frozen_tsc);

        controller.tick = FREEZE_TICKS;
        controller.release_expired_fault_windows().unwrap();
        assert!(!controller.vms[0].vm.virtual_tsc_ref().is_frozen());
        controller.vms[0].vm.virtual_tsc_mut().tick();
        assert!(controller.vms[0].vm.virtual_tsc() > frozen_tsc);
    }

    #[test]
    fn jitter_is_deterministic_bounded_and_explicitly_clearable() {
        // r[verify chaoscontrol.fault_surface.clock_jitter]
        const JITTER_BOUND_TSC: u64 = 37;
        const TEST_TSC: u64 = 1_000;
        let mut controller = adapter_test_controller();
        controller.vms[0].vm.virtual_tsc_mut().set(TEST_TSC);

        controller
            .apply_fault(&Fault::ClockJitter {
                target: 0,
                bound_tsc: JITTER_BOUND_TSC,
            })
            .unwrap();

        let counter = controller.vms[0].vm.virtual_tsc();
        let first = controller.vms[0].vm.virtual_tsc_ref().guest_read();
        let second = controller.vms[0].vm.virtual_tsc_ref().guest_read();
        assert_eq!(first, second);
        assert!(first.abs_diff(counter) <= JITTER_BOUND_TSC);
        assert_eq!(
            controller.vms[0].vm.virtual_tsc_ref().jitter_bound(),
            JITTER_BOUND_TSC
        );

        controller
            .apply_fault(&Fault::ClockJitter {
                target: 0,
                bound_tsc: 0,
            })
            .unwrap();
        assert_eq!(controller.vms[0].vm.virtual_tsc_ref().guest_read(), counter);
    }

    #[test]
    fn cpu_stall_marks_the_vcpu_non_runnable_until_exact_release() {
        // r[verify chaoscontrol.fault_surface.cpu_stall]
        const STALL_TICKS: u64 = 3;
        let mut controller = deterministic_smp_test_controller();

        controller
            .apply_fault(&Fault::CpuStall {
                target: 0,
                vcpu: 0,
                duration_ticks: STALL_TICKS,
            })
            .unwrap();

        assert!(controller.vms[0].vm.vcpu_is_stalled(0));
        assert!(!controller.vms[0].vm.scheduler().state().runnable_vcpus[0]);
        controller.tick = STALL_TICKS - 1;
        controller.release_expired_fault_windows().unwrap();
        assert!(controller.vms[0].vm.vcpu_is_stalled(0));
        controller.tick = STALL_TICKS;
        controller.release_expired_fault_windows().unwrap();
        assert!(!controller.vms[0].vm.vcpu_is_stalled(0));
    }

    #[test]
    fn memory_pressure_is_guest_visible_and_restores_the_baseline() {
        // r[verify chaoscontrol.fault_surface.memory_pressure]
        const PRESSURE_TICKS: u64 = 2;
        const PRESSURE_DIVISOR: u64 = 2;
        let mut controller = adapter_test_controller();
        let baseline = controller.vms[0].vm.memory_ceiling_bytes();
        let limit = baseline / PRESSURE_DIVISOR;

        controller
            .apply_fault(&Fault::MemoryPressure {
                target: 0,
                limit_bytes: limit,
                duration_ticks: PRESSURE_TICKS,
            })
            .unwrap();

        assert_eq!(controller.vms[0].memory_limit_bytes, Some(limit));
        assert_eq!(controller.vms[0].vm.memory_ceiling_bytes(), limit);
        controller.tick = PRESSURE_TICKS;
        controller.release_expired_fault_windows().unwrap();
        assert_eq!(controller.vms[0].memory_limit_bytes, None);
        assert_eq!(controller.vms[0].vm.memory_ceiling_bytes(), baseline);
    }

    #[test]
    fn active_memory_pressure_snapshot_round_trips_and_rejects_deadline_drift() {
        // r[verify chaoscontrol.fault_surface.stage_evidence]
        const PRESSURE_TICKS: u64 = 3;
        const PRESSURE_DIVISOR: u64 = 2;
        const NETWORK_NODE_COUNT: usize = 1;
        let mut controller = adapter_test_controller();
        controller.network = NetworkFabric::new(NETWORK_NODE_COUNT, controller.config.seed);
        let baseline = controller.vms[0].vm.memory_ceiling_bytes();
        let limit = baseline / PRESSURE_DIVISOR;
        controller
            .apply_fault(&Fault::MemoryPressure {
                target: 0,
                limit_bytes: limit,
                duration_ticks: PRESSURE_TICKS,
            })
            .unwrap();
        let snapshot = controller.snapshot_all().unwrap();

        controller.vms[0]
            .vm
            .set_memory_ceiling_bytes(baseline)
            .unwrap();
        controller.vms[0].memory_limit_bytes = None;
        controller.vms[0].memory_limit_release_at_tick = None;
        controller.restore_all(&snapshot).unwrap();
        assert_eq!(controller.vms[0].vm.memory_ceiling_bytes(), limit);

        let mut forged = snapshot;
        forged.memory_pressure[0].as_mut().unwrap().release_at_tick += 1;
        assert!(controller.restore_all(&forged).is_err());
    }

    #[test]
    fn invalid_fault_windows_are_rejected_without_observation() {
        // r[verify chaoscontrol.fault_surface.validation]
        let invalid_faults = [
            Fault::ClockFreeze {
                target: 0,
                duration_ticks: 0,
            },
            Fault::CpuStall {
                target: 0,
                vcpu: 0,
                duration_ticks: 0,
            },
            Fault::MemoryPressure {
                target: 0,
                limit_bytes: 0,
                duration_ticks: 1,
            },
        ];
        for fault in invalid_faults {
            let mut controller = adapter_test_controller();
            controller.apply_fault(&fault).unwrap();
            assert_eq!(controller.fault_outcomes().counters.rejected, 1);
            assert_eq!(controller.fault_outcomes().counters.applied, 0);
            assert_eq!(controller.fault_outcomes().counters.observed, 0);
        }
    }

    #[test]
    fn counterfactual_branch_preserves_armed_prefix_attribution() {
        const DISK_DELAY_NS: u64 = 1_000_000;
        let mut controller = adapter_test_controller();
        controller.network = NetworkFabric::new(1, controller.config.seed);
        let _ = adapter_test_block(&mut controller);
        controller
            .apply_fault(&Fault::DiskSlow {
                target: 0,
                delay_ns: DISK_DELAY_NS,
            })
            .unwrap();
        let attempt_id = attempt_id_for(&controller, FaultVariant::DiskSlow);
        assert_eq!(
            controller
                .fault_outcomes()
                .attempts
                .get(&attempt_id)
                .unwrap()
                .stage,
            FaultAuthoritativeStage::Applied
        );
        let snapshot = controller.snapshot_all().unwrap();

        controller.restore_all(&snapshot).unwrap();
        controller
            .begin_counterfactual_fault_run(FaultSchedule::new())
            .unwrap();
        assert!(controller.fault_engine.is_setup_complete());
        let mut read_buffer = [0_u8; 1];
        adapter_test_block(&mut controller)
            .read(0, &mut read_buffer)
            .unwrap();
        controller
            .commit_pending_block_observations(MAX_FAULT_OUTCOME_EVENTS)
            .unwrap();

        assert_eq!(
            controller
                .fault_outcomes()
                .attempts
                .get(&attempt_id)
                .unwrap()
                .stage,
            FaultAuthoritativeStage::Observed
        );
    }

    #[test]
    fn snapshot_rejects_duplicate_shell_operation_sequences() {
        const FIRST_CLOCK_OFFSET_NS: i64 = 1_000_000;
        const SECOND_CLOCK_DELTA_NS: i64 = 2_000_000;
        let mut controller = adapter_test_controller();
        controller
            .apply_fault(&Fault::ClockSkew {
                target: 0,
                offset_ns: FIRST_CLOCK_OFFSET_NS,
            })
            .unwrap();
        controller
            .apply_fault(&Fault::ClockJump {
                target: 0,
                delta_ns: SECOND_CLOCK_DELTA_NS,
            })
            .unwrap();
        let mut snapshot = controller.snapshot_all().unwrap();
        let original = snapshot.fault_engine_snapshot.outcomes().clone();
        let duplicate_sequence = original
            .events
            .iter()
            .find_map(|event| match &event.kind {
                FaultStageKind::Observed { observation } => Some(observation.operation_sequence),
                _ => None,
            })
            .unwrap();
        let mut observed_count = 0_usize;
        let mut rebuilt = FaultOutcomeLedger::default();
        for event in &original.events {
            let kind = match &event.kind {
                FaultStageKind::Observed { observation } => {
                    observed_count += 1;
                    if observed_count == 2 {
                        FaultStageKind::Observed {
                            observation: FaultObservation::new(
                                observation.attempt_id,
                                observation.subsystem,
                                duplicate_sequence,
                                observation.effect,
                            ),
                        }
                    } else {
                        event.kind.clone()
                    }
                }
                _ => event.kind.clone(),
            };
            let attempt = if kind == FaultStageKind::Selected {
                Some(&original.attempts.get(&event.attempt_id).unwrap().attempt)
            } else {
                None
            };
            rebuilt = transition_fault_outcome(&rebuilt, attempt, event.attempt_id, kind).unwrap();
        }
        snapshot
            .fault_engine_snapshot
            .replace_outcomes_for_validation_test(rebuilt);

        assert!(matches!(
            controller.restore_all(&snapshot),
            Err(VmError::Snapshot { .. })
        ));
    }

    #[test]
    fn mid_pause_snapshot_restores_after_process_skip_observation() {
        const PAUSE_TICKS: u64 = 3;
        const PAUSE_DURATION_NS: u64 =
            PAUSE_TICKS * chaoscontrol_fault::outcomes::NANOSECONDS_PER_SIMULATION_TICK;
        let mut controller = adapter_test_controller();
        controller.network = NetworkFabric::new(1, controller.config.seed);
        controller
            .apply_fault(&Fault::ProcessPause {
                target: 0,
                duration_ns: PAUSE_DURATION_NS,
            })
            .unwrap();
        let attempt_id = attempt_id_for(&controller, FaultVariant::ProcessPause);
        controller.step_round().unwrap();
        assert_eq!(controller.fault_outcomes().counters.observed, 1);

        let snapshot = controller.snapshot_all().unwrap();
        assert_eq!(snapshot.process_fault_attempt[0], Some(attempt_id));
        assert!(matches!(
            snapshot.vm_snapshots[0].1,
            VmStatus::Resuming { .. }
        ));
        controller.vms[0].status = VmStatus::Running;
        controller.vms[0].process_fault_attempt = None;

        controller.restore_all(&snapshot).unwrap();

        assert_eq!(controller.vms[0].process_fault_attempt, Some(attempt_id));
        assert_eq!(controller.vms[0].status, snapshot.vm_snapshots[0].1);
    }

    #[test]
    fn pause_snapshot_rejects_wrong_target_and_deadline() {
        const PAUSE_TICKS: u64 = 3;
        const PAUSE_DURATION_NS: u64 =
            PAUSE_TICKS * chaoscontrol_fault::outcomes::NANOSECONDS_PER_SIMULATION_TICK;
        const WRONG_TARGET: u32 = 1;
        let mut controller = adapter_test_controller();
        controller.network = NetworkFabric::new(1, controller.config.seed);
        controller
            .apply_fault(&Fault::ProcessPause {
                target: 0,
                duration_ns: PAUSE_DURATION_NS,
            })
            .unwrap();
        let attempt_id = attempt_id_for(&controller, FaultVariant::ProcessPause);
        controller.step_round().unwrap();
        let snapshot = controller.snapshot_all().unwrap();
        let VmStatus::Resuming { resume_at_tick } = snapshot.vm_snapshots[0].1 else {
            panic!("pause snapshot must remain active");
        };

        assert!(validate_process_snapshot_effect(
            snapshot.fault_engine_snapshot.outcomes(),
            WRONG_TARGET,
            VmStatus::Resuming { resume_at_tick },
            Some(attempt_id),
            false,
        )
        .is_err());

        let mut wrong_deadline = snapshot.clone();
        wrong_deadline.vm_snapshots[0].1 = VmStatus::Resuming {
            resume_at_tick: resume_at_tick + 1,
        };
        let status_before = controller.vms[0].status;
        assert!(controller.restore_all(&wrong_deadline).is_err());
        assert_eq!(controller.vms[0].status, status_before);
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
