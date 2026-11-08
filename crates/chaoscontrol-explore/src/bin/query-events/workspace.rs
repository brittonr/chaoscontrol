//! Explicit exact-session capture and guest observation commands.

use super::{publication::Publications, read_json, MAX_INPUT_BYTES};
use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};
use chaoscontrol_replay::moment::MAX_MOMENT_TICKS;
use chaoscontrol_replay::observation::{
    admit_observation, run_observation, validate_observation_receipt, ObservationBinding,
    ObservationProfile, ObservationReceipt,
};
use chaoscontrol_replay::recording::{RecordedEvent, Recorder, RecordingConfig};
use chaoscontrol_replay::session::{
    load_session, save_recorder, ExactSession, ExecutionSelection, SessionLimits,
};
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController};
use chaoscontrol_vmm::scheduler::core::{
    BoundaryProfileId, ProgressBudget, ProgressMode, MAX_SCHEDULE_VCPUS,
};
use chaoscontrol_vmm::scheduler::{DEFAULT_PMU_EXACT_STEP_MARGIN, DEFAULT_SMP_INSTRUCTION_QUANTUM};
use chaoscontrol_vmm::vm::VmConfig;
use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

const MEBIBYTE: usize = 1024 * 1024;
const MAX_CAPTURE_SERIAL_BYTES: usize =
    chaoscontrol_replay::serialize::MAX_RECORDING_JSON_BYTES / 2;
const COARSE_ENTRY_WATCHDOG_MICROSECONDS: u64 = 250_000;
const COARSE_SCHEDULE_JOURNAL_LIMIT: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum SmpProgressMode {
    ExactSingleStep,
    PmuAccelerated,
    KvmDebugBoundary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum BoundaryProfile {
    KvmDebugExitBoundaryV1,
}

#[derive(Clone, Debug, Default, Args)]
pub struct ProgressArgs {
    /// Explicit SMP mode; capture defaults to strict exact-single-step.
    #[arg(long, value_enum)]
    smp_progress_mode: Option<SmpProgressMode>,
    /// Strict guest-instruction quantum; never accepted for coarse execution.
    #[arg(long, conflicts_with = "smp_boundary_quantum")]
    smp_instruction_quantum: Option<u64>,
    /// Coarse normalized KVM boundary quantum, not a guest-instruction count.
    #[arg(long, requires = "smp_progress_mode")]
    smp_boundary_quantum: Option<u64>,
    /// Immutable normalized boundary table; required for a fresh coarse capture.
    #[arg(long, value_enum, requires = "smp_progress_mode")]
    smp_boundary_profile: Option<BoundaryProfile>,
}

impl ProgressArgs {
    fn selection(&self, retained: Option<&VmConfig>) -> Result<ExecutionSelection, String> {
        let selected = match self.smp_progress_mode {
            Some(mode) => mode,
            None if retained
                .is_some_and(|vm| vm.smp_progress_mode == ProgressMode::KvmDebugBoundary) =>
            {
                return Err(
                    "coarse execution requires explicit --smp-progress-mode kvm-debug-boundary"
                        .into(),
                );
            }
            None => match retained.map(|vm| vm.smp_progress_mode) {
                Some(ProgressMode::PmuAccelerated { .. }) => SmpProgressMode::PmuAccelerated,
                _ => SmpProgressMode::ExactSingleStep,
            },
        };
        let (progress_mode, progress_budget) = match selected {
            SmpProgressMode::KvmDebugBoundary => {
                if self.smp_instruction_quantum.is_some() {
                    return Err("--smp-instruction-quantum is strict-only".into());
                }
                let profile_id = match self.smp_boundary_profile {
                    Some(BoundaryProfile::KvmDebugExitBoundaryV1) => BoundaryProfileId::KvmDebugExitBoundaryV1,
                    None => match retained.map(|vm| vm.smp_progress_budget) {
                        Some(ProgressBudget::KvmBoundaries { profile_id, .. }) => profile_id,
                        _ => return Err("coarse capture requires --smp-boundary-profile kvm-debug-exit-boundary-v1".into()),
                    },
                };
                let progress_budget = match self.smp_boundary_quantum {
                    Some(count) => ProgressBudget::KvmBoundaries { profile_id, count },
                    None => retained
                        .map(|vm| ProgressBudget::KvmBoundaries {
                            profile_id,
                            count: vm.smp_progress_budget.count(),
                        })
                        .ok_or("coarse capture requires --smp-boundary-quantum")?,
                };
                (ProgressMode::KvmDebugBoundary, progress_budget)
            }
            SmpProgressMode::ExactSingleStep | SmpProgressMode::PmuAccelerated => {
                if self.smp_boundary_quantum.is_some() || self.smp_boundary_profile.is_some() {
                    return Err("boundary profile/quantum require kvm-debug-boundary mode".into());
                }
                let progress_mode = match selected {
                    SmpProgressMode::PmuAccelerated => {
                        match retained.map(|vm| vm.smp_progress_mode) {
                            Some(mode @ ProgressMode::PmuAccelerated { .. }) => mode,
                            _ => ProgressMode::PmuAccelerated {
                                exact_step_margin: DEFAULT_PMU_EXACT_STEP_MARGIN,
                            },
                        }
                    }
                    _ => ProgressMode::ExactSingleStep,
                };
                let progress_budget = self
                    .smp_instruction_quantum
                    .map(ProgressBudget::Instructions)
                    .or_else(|| retained.map(|vm| vm.smp_progress_budget))
                    .unwrap_or(ProgressBudget::Instructions(
                        DEFAULT_SMP_INSTRUCTION_QUANTUM,
                    ));
                (progress_mode, progress_budget)
            }
        };
        if progress_budget.count() == 0 {
            return Err("SMP progress quantum must be positive".into());
        }
        if let Some(vm) = retained {
            if progress_mode != vm.smp_progress_mode || progress_budget != vm.smp_progress_budget {
                return Err("execution selection must match retained mode, units, profile and quantum; overrides are forbidden".into());
            }
        }
        Ok(ExecutionSelection {
            progress_mode,
            progress_budget,
        })
    }

    fn capture_vm_config(
        &self,
        seed: u64,
        memory_size: usize,
        vcpus: usize,
    ) -> Result<VmConfig, String> {
        if vcpus == 0 || vcpus > MAX_SCHEDULE_VCPUS {
            return Err("capture vCPU count is outside the supported topology".into());
        }
        let selection = self.selection(None)?;
        if selection.progress_mode == ProgressMode::KvmDebugBoundary && vcpus < 2 {
            return Err("coarse SMP capture requires at least two vCPUs".into());
        }
        let mut vm = VmConfig {
            memory_size,
            num_vcpus: vcpus,
            smp_progress_mode: selection.progress_mode,
            smp_progress_budget: selection.progress_budget,
            ..VmConfig::default()
        };
        vm.cpu.seed = seed;
        vm.cpu.hide_tsc = true;
        if let ProgressBudget::KvmBoundaries { profile_id, .. } = selection.progress_budget {
            vm.smp_entry_watchdog_microseconds = COARSE_ENTRY_WATCHDOG_MICROSECONDS;
            vm.smp_schedule_journal_limit = COARSE_SCHEDULE_JOURNAL_LIMIT;
            vm.apply_kvm_debug_boundary_profile(profile_id)
                .map_err(|error| error.to_string())?;
        }
        Ok(vm)
    }

    fn authorize(&self, session: &mut ExactSession) -> Result<(), String> {
        let selection = self.selection(Some(&session.manifest().configuration.vm_config))?;
        session
            .authorize_execution(selection)
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    /// Execute an explicitly configured guest and persist exact retained checkpoints.
    CaptureSession {
        #[arg(long)]
        kernel: PathBuf,
        #[arg(long)]
        initrd: Option<PathBuf>,
        #[arg(long)]
        seed: u64,
        #[arg(long)]
        vms: usize,
        #[arg(long)]
        memory_mib: usize,
        /// vCPUs per guest. Coarse SMP must be explicitly selected.
        #[arg(long, default_value_t = 1)]
        vcpus: usize,
        #[command(flatten)]
        progress: ProgressArgs,
        #[arg(long)]
        quantum: u64,
        #[arg(long)]
        bootstrap_ticks: u64,
        #[arg(long)]
        history_ticks: u64,
        #[arg(long)]
        checkpoint_interval: u64,
        /// Optional JSON array of exact scheduled faults; absence means no faults.
        #[arg(long)]
        schedule: Option<PathBuf>,
        /// New session directory. Existing directories are never replaced.
        output: PathBuf,
    },
    /// Run one allowlisted observation inside an exact restored guest, never on the host.
    Observe {
        #[arg(long)]
        session: PathBuf,
        #[command(flatten)]
        progress: ProgressArgs,
        #[arg(long)]
        checkpoint: u64,
        #[arg(long)]
        vm_index: usize,
        #[arg(long)]
        profile: PathBuf,
        /// The same immutable profile array supplied to the guest supervisor.
        #[arg(long)]
        admitted_profiles: PathBuf,
        output: PathBuf,
    },
    /// Validate observation evidence against current exact artifacts without executing a VM.
    ValidateObservation {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        checkpoint: u64,
        #[arg(long)]
        vm_index: usize,
        #[arg(long)]
        profile: PathBuf,
        #[arg(long)]
        admitted_profiles: PathBuf,
        receipt: PathBuf,
    },
}

pub fn execute(command: WorkspaceCommand) -> Result<(), String> {
    match command {
        WorkspaceCommand::CaptureSession {
            kernel,
            initrd,
            seed,
            vms,
            memory_mib,
            vcpus,
            progress,
            quantum,
            bootstrap_ticks,
            history_ticks,
            checkpoint_interval,
            schedule,
            output,
        } => {
            let limits = SessionLimits::default();
            limits.validate().map_err(|error| error.to_string())?;
            let capture_ticks = bootstrap_ticks
                .checked_add(history_ticks)
                .ok_or("capture horizon overflow")?;
            if capture_ticks > limits.max_replay_ticks {
                return Err("capture exceeds reviewed replay tick bound".into());
            }
            let deadline = std::time::Instant::now()
                .checked_add(std::time::Duration::from_millis(limits.host_deadline_ms))
                .ok_or("capture host deadline overflow")?;
            let memory_size = memory_mib
                .checked_mul(MEBIBYTE)
                .ok_or("capture memory size overflow")?;
            let total_memory = memory_size
                .checked_mul(vms)
                .ok_or("capture aggregate memory size overflow")?;
            if vms == 0
                || memory_size == 0
                || total_memory as u64 > limits.max_snapshot_bytes
                || quantum == 0
                || bootstrap_ticks == 0
                || bootstrap_ticks > MAX_MOMENT_TICKS
                || history_ticks == 0
                || history_ticks > MAX_MOMENT_TICKS
                || checkpoint_interval == 0
                || checkpoint_interval > history_ticks
                || (history_ticks / checkpoint_interval)
                    .checked_add(1)
                    .is_none_or(|count| count > limits.max_checkpoints as u64)
            {
                return Err(
                    "capture topology, horizon, quantum, or checkpoint bound is invalid".into(),
                );
            }
            if output.try_exists().map_err(|error| error.to_string())? {
                return Err("capture output directory already exists".into());
            }
            let kernel = kernel
                .to_str()
                .ok_or("kernel path is not UTF-8")?
                .to_string();
            let initrd = initrd
                .map(|path| {
                    path.into_os_string()
                        .into_string()
                        .map_err(|_| "initrd path is not UTF-8".to_string())
                })
                .transpose()?;
            let mut faults = FaultSchedule::new();
            if let Some(path) = schedule {
                let scheduled: Vec<ScheduledFault> = read_json(&path)?;
                if scheduled.len() > chaoscontrol_fault::outcomes::MAX_FAULT_ATTEMPTS {
                    return Err("capture schedule exceeds the attempt bound".into());
                }
                for fault in scheduled {
                    faults.add(fault);
                }
            }
            let vm_config = progress.capture_vm_config(seed, memory_size, vcpus)?;
            let config = SimulationConfig {
                num_vms: vms,
                vm_config,
                kernel_path: kernel.clone(),
                initrd_path: initrd.clone(),
                seed,
                quantum,
                schedule: faults.clone(),
                disk_image_path: None,
                bootstrap_budget: Some(bootstrap_ticks),
                base_core: None,
                dlog_dir: None,
                random_faults: false,
            };
            let recording_config = RecordingConfig {
                num_vms: vms,
                vm_memory_size: memory_size,
                tsc_khz: config.vm_config.cpu.tsc_khz,
                kernel_path: kernel,
                initrd_path: initrd,
                quantum,
                smp_progress_mode: config.vm_config.smp_progress_mode,
                smp_progress_budget: config.vm_config.smp_progress_budget,
                checkpoint_interval,
                disk_image_path: None,
                random_faults: false,
            };
            let mut recorder = Recorder::new(recording_config, faults, seed);
            let mut controller =
                SimulationController::new(config).map_err(|error| error.to_string())?;
            controller
                .init_thread_timers()
                .map_err(|error| error.to_string())?;
            // Retain bootstrap too: no pre-checkpoint trace history is inferred.
            recorder
                .start_controller_session(&mut controller)
                .map_err(|error| error.to_string())?;
            let mut serial = vec![String::new(); vms];
            let mut serial_bytes = 0_usize;
            let mut checkpoint_serial_bytes = 0u64;
            loop {
                capture_round(
                    &mut controller,
                    &mut recorder,
                    &mut serial,
                    &mut serial_bytes,
                    deadline,
                    limits,
                    checkpoint_serial_bytes,
                )?;
                let ready = (0..vms).all(|index| {
                    controller
                        .vm_slot(index)
                        .is_some_and(|slot| slot.vm.fault_engine().is_setup_complete())
                });
                if ready {
                    break;
                }
                if controller.tick() >= bootstrap_ticks {
                    return Err("guest setup did not complete inside the bootstrap horizon".into());
                }
            }
            // This propagates actual per-VM admission, just as the controller's
            // bootstrap helper does. It never invents guest setup completion.
            controller.force_setup_complete();
            let stop = controller
                .tick()
                .checked_add(history_ticks)
                .ok_or("capture horizon overflow")?;
            let mut checkpoint_tick = controller
                .tick()
                .checked_add(checkpoint_interval)
                .ok_or("checkpoint horizon overflow")?;
            while controller.tick() < stop {
                capture_round(
                    &mut controller,
                    &mut recorder,
                    &mut serial,
                    &mut serial_bytes,
                    deadline,
                    limits,
                    checkpoint_serial_bytes,
                )?;
                if controller.tick() >= checkpoint_tick {
                    checkpoint_serial_bytes = checkpoint_serial_bytes
                        .checked_add(serial_bytes as u64)
                        .ok_or("capture output accounting overflow")?;
                    if checkpoint_serial_bytes
                        .checked_add(serial_bytes as u64)
                        .is_none_or(|bytes| bytes > limits.max_output_bytes)
                    {
                        return Err("capture retained output bound exceeded".into());
                    }
                    recorder
                        .on_controller_tick(&mut controller, serial.clone())
                        .map_err(|error| error.to_string())?;
                    checkpoint_tick = checkpoint_tick
                        .checked_add(checkpoint_interval)
                        .ok_or("checkpoint horizon overflow")?;
                }
            }
            let manifest = save_recorder(&recorder, &controller, &output, limits)
                .map_err(|error| error.to_string())?;
            println!(
                "{}",
                serde_json::to_string(&manifest).map_err(|error| error.to_string())?
            );
            Ok(())
        }
        WorkspaceCommand::Observe {
            session,
            progress,
            checkpoint,
            vm_index,
            profile,
            admitted_profiles,
            output,
        } => {
            super::reject_session_outputs(&session, &[("observation", output.as_path())])?;
            super::ensure_outputs_do_not_overwrite_inputs(
                &[("observation", output.as_path())],
                &[
                    ("profile", Some(profile.as_path())),
                    ("admitted profiles", Some(admitted_profiles.as_path())),
                ],
            )?;
            let publication =
                Publications::admit(&[("observation", output.as_path())], Some(&session))?;
            let profile: ObservationProfile = read_json(&profile)?;
            let identities = admitted_identities(&admitted_profiles)?;
            // Pure admission precedes even the restored VM's creation.
            admit_observation(&profile, &identities).map_err(|error| error.to_string())?;
            let mut session = load_session(&session, SessionLimits::default())
                .map_err(|error| error.to_string())?;
            progress.authorize(&mut session)?;
            let mut restored = session
                .restore_controller(checkpoint)
                .map_err(|error| error.to_string())?;
            let receipt = run_observation(&mut restored, vm_index, &profile, &identities)
                .map_err(|error| error.to_string())?;
            publication.json(&output, &receipt, MAX_INPUT_BYTES)
        }
        WorkspaceCommand::ValidateObservation {
            session,
            checkpoint,
            vm_index,
            profile,
            admitted_profiles,
            receipt,
        } => {
            let profile: ObservationProfile = read_json(&profile)?;
            let identities = admitted_identities(&admitted_profiles)?;
            admit_observation(&profile, &identities).map_err(|error| error.to_string())?;
            let session = load_session(&session, SessionLimits::default())
                .map_err(|error| error.to_string())?;
            if vm_index >= session.manifest().configuration.num_vms {
                return Err("observation VM index is outside the exact session".into());
            }
            let selected = session
                .manifest()
                .checkpoints
                .iter()
                .find(|item| item.id == checkpoint)
                .ok_or("observation checkpoint is not retained")?;
            let binding = ObservationBinding {
                session_identity: session.manifest().session_id.clone(),
                checkpoint_identity: session
                    .checkpoint_identity(checkpoint)
                    .map_err(|error| error.to_string())?
                    .to_string(),
                checkpoint_tick: selected.tick,
                vm_index,
                selected_tick: selected.tick,
            };
            let receipt: ObservationReceipt = read_json(&receipt)?;
            validate_observation_receipt(&receipt, &binding, &profile)
                .map_err(|error| error.to_string())?;
            println!("observation receipt validated");
            Ok(())
        }
    }
}

fn admitted_identities(path: &std::path::Path) -> Result<Vec<String>, String> {
    let profiles: Vec<ObservationProfile> = read_json(path)?;
    if profiles.len() > chaoscontrol_replay::observation::MAX_ADMITTED_OBSERVATION_PROFILES {
        return Err("admitted observation profile count exceeds its bound".into());
    }
    profiles.iter().map(|profile| profile.identity()).collect()
}

fn capture_round(
    controller: &mut SimulationController,
    recorder: &mut Recorder,
    serial: &mut [String],
    serial_bytes: &mut usize,
    deadline: std::time::Instant,
    limits: SessionLimits,
    checkpoint_serial_bytes: u64,
) -> Result<(), String> {
    if std::time::Instant::now() >= deadline {
        return Err("capture host execution deadline exceeded (operational failure)".into());
    }
    let round = controller.step_round().map_err(|error| error.to_string())?;
    if std::time::Instant::now() >= deadline {
        return Err("capture host execution deadline exceeded (operational failure)".into());
    }
    recorder
        .record_round(&round, controller.fault_outcomes())
        .map_err(|error| format!("invalid captured round: {error:?}"))?;
    if round.vms_running == 0 {
        return Err("all guests halted before the requested capture horizon".into());
    }
    for (index, retained) in serial.iter_mut().enumerate() {
        let vm = &mut controller
            .vm_slot_mut(index)
            .ok_or("capture VM is absent")?
            .vm;
        if vm.serial_dropped_byte_count() != 0 {
            return Err(
                "guest serial capture lost bytes; exact retained input is incomplete".into(),
            );
        }
        let text = vm.take_serial_output();
        *serial_bytes = serial_bytes
            .checked_add(text.len())
            .ok_or("serial byte accounting overflow")?;
        if *serial_bytes > MAX_CAPTURE_SERIAL_BYTES {
            return Err("capture serial byte bound exceeded".into());
        }
        if checkpoint_serial_bytes
            .checked_add(*serial_bytes as u64)
            .is_none_or(|bytes| bytes > limits.max_output_bytes)
        {
            return Err("capture retained output bound exceeded".into());
        }
        if !text.is_empty() {
            retained.push_str(&text);
            recorder.record_event(RecordedEvent::SerialOutput {
                tick: round.tick,
                vm_index: index,
                data: text,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cli, Command};
    use clap::Parser;

    const TEST_SEED: u64 = 42;
    const TEST_MEMORY: usize = 128 * MEBIBYTE;
    const TEST_VCPUS: usize = 4;
    const TEST_BOUNDARIES: u64 = 17;

    fn coarse_args(quantum: Option<u64>) -> ProgressArgs {
        ProgressArgs {
            smp_progress_mode: Some(SmpProgressMode::KvmDebugBoundary),
            smp_boundary_quantum: quantum,
            smp_boundary_profile: Some(BoundaryProfile::KvmDebugExitBoundaryV1),
            ..ProgressArgs::default()
        }
    }

    fn retained_coarse_vm() -> VmConfig {
        coarse_args(Some(TEST_BOUNDARIES))
            .capture_vm_config(TEST_SEED, TEST_MEMORY, TEST_VCPUS)
            .unwrap()
    }

    #[test]
    fn capture_preserves_explicit_smp_topology_mode_and_typed_budget() {
        let vm = retained_coarse_vm();
        assert_eq!(vm.num_vcpus, TEST_VCPUS);
        assert_eq!(vm.memory_size, TEST_MEMORY);
        assert_eq!(vm.cpu.seed, TEST_SEED);
        assert!(vm.cpu.hide_tsc);
        assert!(vm.cpu.hide_hypervisor);
        assert!(matches!(
            vm.cpu.tsc_mode,
            chaoscontrol_vmm::cpu::TscAdvanceMode::PerExit
        ));
        assert_eq!(
            vm.smp_entry_watchdog_microseconds,
            COARSE_ENTRY_WATCHDOG_MICROSECONDS
        );
        assert_eq!(vm.smp_schedule_journal_limit, COARSE_SCHEDULE_JOURNAL_LIMIT);
        let extra = vm.extra_cmdline.as_deref().unwrap();
        let command = std::str::from_utf8(vm.cmdline.strip_suffix(&[0]).unwrap()).unwrap();
        for token in ["noapictimer", "clocksource=jiffies", "notsc"] {
            assert!(extra.split_ascii_whitespace().any(|item| item == token));
            assert!(command.split_ascii_whitespace().any(|item| item == token));
        }
        assert!(!command
            .split_ascii_whitespace()
            .any(|item| matches!(item, "clocksource=tsc" | "tsc=reliable")));
        assert_eq!(vm.smp_progress_mode, ProgressMode::KvmDebugBoundary);
        assert_eq!(
            vm.smp_progress_budget,
            ProgressBudget::KvmBoundaries {
                profile_id: BoundaryProfileId::KvmDebugExitBoundaryV1,
                count: TEST_BOUNDARIES,
            }
        );
    }

    #[test]
    fn capture_defaults_remain_strict_and_instruction_quantum_is_strict_only() {
        let vm = ProgressArgs::default()
            .capture_vm_config(TEST_SEED, TEST_MEMORY, 1)
            .unwrap();
        assert_eq!(vm.num_vcpus, 1);
        assert_eq!(vm.smp_progress_mode, ProgressMode::ExactSingleStep);
        assert_eq!(
            vm.smp_progress_budget,
            ProgressBudget::Instructions(DEFAULT_SMP_INSTRUCTION_QUANTUM)
        );
        let args = ProgressArgs {
            smp_instruction_quantum: Some(123),
            ..ProgressArgs::default()
        };
        let vm = args
            .capture_vm_config(TEST_SEED, TEST_MEMORY, TEST_VCPUS)
            .unwrap();
        assert_eq!(vm.smp_progress_budget, ProgressBudget::Instructions(123));
        assert_eq!(vm.smp_progress_mode, ProgressMode::ExactSingleStep);
        let mut coarse = coarse_args(Some(TEST_BOUNDARIES));
        coarse.smp_instruction_quantum = Some(123);
        assert!(coarse
            .capture_vm_config(TEST_SEED, TEST_MEMORY, TEST_VCPUS)
            .unwrap_err()
            .contains("strict-only"));
    }

    #[test]
    fn coarse_capture_rejects_missing_zero_or_wrong_unit_budgets_and_topology() {
        assert!(coarse_args(None)
            .capture_vm_config(TEST_SEED, TEST_MEMORY, TEST_VCPUS)
            .is_err());
        assert!(coarse_args(Some(0))
            .capture_vm_config(TEST_SEED, TEST_MEMORY, TEST_VCPUS)
            .is_err());
        let mut missing_profile = coarse_args(Some(TEST_BOUNDARIES));
        missing_profile.smp_boundary_profile = None;
        assert!(missing_profile
            .capture_vm_config(TEST_SEED, TEST_MEMORY, TEST_VCPUS)
            .unwrap_err()
            .contains("--smp-boundary-profile"));
        for vcpus in [0, 1, MAX_SCHEDULE_VCPUS + 1] {
            assert!(coarse_args(Some(TEST_BOUNDARIES))
                .capture_vm_config(TEST_SEED, TEST_MEMORY, vcpus)
                .is_err());
        }
        let strict_boundary = ProgressArgs {
            smp_boundary_quantum: Some(TEST_BOUNDARIES),
            ..ProgressArgs::default()
        };
        assert!(strict_boundary
            .capture_vm_config(TEST_SEED, TEST_MEMORY, TEST_VCPUS)
            .is_err());
        let strict_profile = ProgressArgs {
            smp_boundary_profile: Some(BoundaryProfile::KvmDebugExitBoundaryV1),
            ..ProgressArgs::default()
        };
        assert!(strict_profile
            .capture_vm_config(TEST_SEED, TEST_MEMORY, TEST_VCPUS)
            .is_err());
        let strict_zero = ProgressArgs {
            smp_instruction_quantum: Some(0),
            ..ProgressArgs::default()
        };
        assert!(strict_zero
            .capture_vm_config(TEST_SEED, TEST_MEMORY, TEST_VCPUS)
            .is_err());
    }

    #[test]
    fn persisted_coarse_selection_requires_explicit_mode_and_cannot_override_budget() {
        let vm = retained_coarse_vm();
        assert!(ProgressArgs::default()
            .selection(Some(&vm))
            .unwrap_err()
            .contains("explicit"));
        let mode_only = ProgressArgs {
            smp_progress_mode: Some(SmpProgressMode::KvmDebugBoundary),
            ..ProgressArgs::default()
        };
        for args in [
            mode_only,
            coarse_args(None),
            coarse_args(Some(TEST_BOUNDARIES)),
        ] {
            assert_eq!(
                args.selection(Some(&vm)).unwrap(),
                ExecutionSelection {
                    progress_mode: vm.smp_progress_mode,
                    progress_budget: vm.smp_progress_budget,
                }
            );
        }
        for args in [
            coarse_args(Some(TEST_BOUNDARIES + 1)),
            coarse_args(Some(0)),
            ProgressArgs {
                smp_progress_mode: Some(SmpProgressMode::ExactSingleStep),
                ..ProgressArgs::default()
            },
            ProgressArgs {
                smp_progress_mode: Some(SmpProgressMode::PmuAccelerated),
                ..ProgressArgs::default()
            },
            ProgressArgs {
                smp_progress_mode: Some(SmpProgressMode::KvmDebugBoundary),
                smp_instruction_quantum: Some(TEST_BOUNDARIES),
                ..ProgressArgs::default()
            },
            ProgressArgs {
                smp_boundary_quantum: Some(TEST_BOUNDARIES),
                ..ProgressArgs::default()
            },
        ] {
            assert!(args.selection(Some(&vm)).is_err(), "{args:?}");
        }
    }

    #[test]
    fn persisted_strict_selection_preserves_mode_and_units_without_downgrade() {
        let mut vm = VmConfig::default();
        let selection = ProgressArgs::default().selection(Some(&vm)).unwrap();
        assert_eq!(selection.progress_mode, ProgressMode::ExactSingleStep);
        assert_eq!(selection.progress_budget, vm.smp_progress_budget);
        assert!(coarse_args(None).selection(Some(&vm)).is_err());
        let instruction_override = ProgressArgs {
            smp_instruction_quantum: Some(vm.smp_progress_budget.count() + 1),
            ..ProgressArgs::default()
        };
        assert!(instruction_override.selection(Some(&vm)).is_err());
        vm.smp_progress_mode = ProgressMode::PmuAccelerated {
            exact_step_margin: 31,
        };
        assert_eq!(
            ProgressArgs::default()
                .selection(Some(&vm))
                .unwrap()
                .progress_mode,
            vm.smp_progress_mode
        );
        assert!(ProgressArgs {
            smp_progress_mode: Some(SmpProgressMode::ExactSingleStep),
            ..ProgressArgs::default()
        }
        .selection(Some(&vm))
        .is_err());
    }

    #[test]
    fn capture_cli_selects_nondefault_coarse_smp() {
        let cli = Cli::try_parse_from([
            "query-events",
            "capture-session",
            "--kernel",
            "kernel",
            "--seed",
            "42",
            "--vms",
            "1",
            "--memory-mib",
            "128",
            "--vcpus",
            "4",
            "--smp-progress-mode",
            "kvm-debug-boundary",
            "--smp-boundary-profile",
            "kvm-debug-exit-boundary-v1",
            "--smp-boundary-quantum",
            "17",
            "--quantum",
            "100",
            "--bootstrap-ticks",
            "10",
            "--history-ticks",
            "20",
            "--checkpoint-interval",
            "5",
            "session-output",
        ])
        .unwrap();
        let Command::Workspace(WorkspaceCommand::CaptureSession {
            vcpus, progress, ..
        }) = cli.command
        else {
            panic!("capture command expected");
        };
        assert_eq!(vcpus, TEST_VCPUS);
        assert_eq!(
            progress.selection(None).unwrap().progress_budget.count(),
            TEST_BOUNDARIES
        );
    }

    #[test]
    fn observe_cli_exposes_explicit_mode_and_optional_matching_quantum() {
        let cli = Cli::try_parse_from([
            "query-events",
            "observe",
            "--session",
            "retained",
            "--checkpoint",
            "1",
            "--vm-index",
            "0",
            "--profile",
            "profile.json",
            "--admitted-profiles",
            "profiles.json",
            "--smp-progress-mode",
            "kvm-debug-boundary",
            "--smp-boundary-quantum",
            "17",
            "observation.json",
        ])
        .unwrap();
        let Command::Workspace(WorkspaceCommand::Observe { progress, .. }) = cli.command else {
            panic!("observe command expected");
        };
        assert_eq!(
            progress
                .selection(Some(&retained_coarse_vm()))
                .unwrap()
                .progress_budget
                .count(),
            TEST_BOUNDARIES
        );
    }

    #[test]
    fn progress_cli_rejects_ambiguous_or_unknown_options() {
        #[derive(Parser)]
        struct ProgressCli {
            #[command(flatten)]
            progress: ProgressArgs,
        }
        for args in [
            vec!["progress", "--smp-boundary-quantum", "17"],
            vec!["progress", "--smp-progress-mode", "legacy-wall-clock"],
            vec![
                "progress",
                "--smp-boundary-profile",
                "kvm-debug-exit-boundary-v1",
            ],
            vec![
                "progress",
                "--smp-progress-mode",
                "kvm-debug-boundary",
                "--smp-boundary-profile",
                "unknown",
            ],
            vec![
                "progress",
                "--smp-progress-mode",
                "kvm-debug-boundary",
                "--smp-boundary-profile",
                "kvm-debug-exit-boundary-v2",
            ],
            vec![
                "progress",
                "--smp-progress-mode",
                "kvm-debug-boundary",
                "--smp-boundary-quantum",
                "17",
                "--smp-instruction-quantum",
                "17",
            ],
            vec![
                "progress",
                "--smp-progress-mode",
                "kvm-debug-boundary",
                "--smp-boundary-quantum",
                "-1",
            ],
        ] {
            assert!(ProgressCli::try_parse_from(args).is_err());
        }
    }

    #[test]
    fn missing_coarse_profile_rejects_before_guest_or_output_creation() {
        const SEED: u64 = 42;
        const BOOTSTRAP_TICKS: u64 = 10;
        const HISTORY_TICKS: u64 = 20;
        const CHECKPOINT_INTERVAL: u64 = 5;
        const EXIT_QUANTUM: u64 = 100;
        const BOUNDARY_QUANTUM: u64 = 17;
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("session");
        let error = execute(WorkspaceCommand::CaptureSession {
            kernel: temp.path().join("absent-kernel"),
            initrd: None,
            seed: SEED,
            vms: 1,
            memory_mib: 128,
            vcpus: 4,
            progress: ProgressArgs {
                smp_progress_mode: Some(SmpProgressMode::KvmDebugBoundary),
                smp_boundary_quantum: Some(BOUNDARY_QUANTUM),
                ..ProgressArgs::default()
            },
            quantum: EXIT_QUANTUM,
            bootstrap_ticks: BOOTSTRAP_TICKS,
            history_ticks: HISTORY_TICKS,
            checkpoint_interval: CHECKPOINT_INTERVAL,
            schedule: None,
            output: output.clone(),
        })
        .unwrap_err();
        assert!(error.contains("--smp-boundary-profile"), "{error}");
        assert!(!output.exists());
    }
}
