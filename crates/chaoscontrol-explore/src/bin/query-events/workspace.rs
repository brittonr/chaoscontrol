//! Explicit exact-session capture and guest observation commands.

use super::{publication::Publications, read_json, MAX_INPUT_BYTES};
use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};
use chaoscontrol_replay::moment::MAX_MOMENT_TICKS;
use chaoscontrol_replay::observation::{
    admit_observation, run_observation, validate_observation_receipt, ObservationBinding,
    ObservationProfile, ObservationReceipt,
};
use chaoscontrol_replay::recording::{RecordedEvent, Recorder, RecordingConfig};
use chaoscontrol_replay::session::{load_session, save_recorder, SessionLimits};
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController};
use chaoscontrol_vmm::vm::VmConfig;
use clap::Subcommand;
use std::path::PathBuf;

const MEBIBYTE: usize = 1024 * 1024;
const MAX_CAPTURE_SERIAL_BYTES: usize =
    chaoscontrol_replay::serialize::MAX_RECORDING_JSON_BYTES / 2;

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
            let mut vm_config = VmConfig::single_vcpu(seed, true);
            vm_config.memory_size = memory_size;
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
            let session = load_session(&session, SessionLimits::default())
                .map_err(|error| error.to_string())?;
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
