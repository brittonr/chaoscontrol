use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use chaoscontrol_evidence::{
    expected_kernel_bundle_kvm_observations, extract_kvm_observations,
    kernel_bundle_kvm_rail_receipt, kernel_bundle_smoke_profile_identity,
    kernel_bundle_smoke_receipt, sample_mantle_private_kfunc_kvm_markers,
    sample_mantle_private_kfunc_profile, validate_kernel_bundle_smoke_profile,
    write_private_kfunc_initrd, EvidenceError, EvidenceResult, KernelBundleKvmRun,
    KernelBundleSmokeProfile, PrivateKfuncInitrdRequest, SmokeObservation, DEFAULT_KVM_MAX_EXITS,
    PRIVATE_KFUNC_EXPECTED_KERNEL_RELEASE,
};
use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};

const ARG_SAMPLE_PROFILE: &str = "--sample-profile";
const ARG_SAMPLE_RECEIPT: &str = "--sample-receipt";
const ARG_SAMPLE_KVM_MARKERS: &str = "--sample-kvm-markers";
const ARG_CHECK_PROFILE: &str = "--check-profile";
const ARG_CHECK_KVM_SERIAL: &str = "--check-kvm-serial";
const ARG_KVM_RUN_PROFILE: &str = "--kvm-run-profile";
const ARG_BUILD_PRIVATE_KFUNC_INITRD: &str = "--build-private-kfunc-initrd";
const ARG_ARTIFACTS_DIR: &str = "--artifacts-dir";
const ARG_BUSYBOX: &str = "--busybox";
const ARG_BPFTOOL: &str = "--bpftool";
const ARG_DELETE_MODULE_HELPER: &str = "--delete-module-helper";
const ARG_CLOSURE_LIST: &str = "--closure-list";
const ARG_EXPECTED_KERNEL_RELEASE: &str = "--expected-kernel-release";
const ARG_KERNEL: &str = "--kernel";
const ARG_INITRD: &str = "--initrd";
const ARG_OUT: &str = "--out";
const ARG_MAX_EXITS: &str = "--max-exits";
const ARG_MEMORY_MIB: &str = "--memory-mib";
const BYTES_PER_MIB: usize = 1024 * 1024;
const DEFAULT_MEMORY_MIB: usize = 256;
const MIN_MEMORY_MIB: usize = 128;
const MAX_MEMORY_MIB: usize = 4096;
const KVM_OBSERVATION_POLL_EXITS: u64 = 256;
const GUEST_ERROR_CLASS: &str = "error";
const EXECUTION_MODE_EXACT_KVM: &str = "chaoscontrol-vmm-kvm";
const EXECUTION_MODE_SERIAL_TRANSCRIPT: &str = "serial-marker-transcript";
const FILE_HASH_BUFFER_BYTES: usize = 64 * 1024;
const ARG_HELP_SHORT: &str = "-h";
const ARG_HELP_LONG: &str = "--help";
const EXPECTED_CHECK_ARG_COUNT: usize = 3;
const EXPECTED_KVM_SERIAL_ARG_COUNT: usize = 4;
const EXPECTED_SINGLE_ARG_COUNT: usize = 2;
const STATUS_STDERR_PREFIX: &str = "kernel-bundle-vm-compat-smoke";
const DEFAULT_RUNNER_ID: &str = "chaoscontrol-vmm-kvm-rail";

fn main() {
    if let Err(error) = run(std::env::args().collect()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> EvidenceResult<()> {
    if args.len() == EXPECTED_SINGLE_ARG_COUNT && args[1] == ARG_SAMPLE_PROFILE {
        print_json(&sample_mantle_private_kfunc_profile())?;
        return Ok(());
    }
    if args.len() == EXPECTED_SINGLE_ARG_COUNT && args[1] == ARG_SAMPLE_RECEIPT {
        let receipt = kernel_bundle_smoke_receipt(&sample_mantle_private_kfunc_profile())?;
        print_json(&receipt)?;
        return Ok(());
    }
    if args.len() == EXPECTED_SINGLE_ARG_COUNT && args[1] == ARG_SAMPLE_KVM_MARKERS {
        println!("{}", sample_mantle_private_kfunc_kvm_markers());
        return Ok(());
    }
    if args.len() == EXPECTED_CHECK_ARG_COUNT && args[1] == ARG_CHECK_PROFILE {
        let profile = read_profile(Path::new(&args[2]))?;
        validate_kernel_bundle_smoke_profile(&profile)?;
        let receipt = kernel_bundle_smoke_receipt(&profile)?;
        print_json(&receipt)?;
        return Ok(());
    }
    if args.len() == EXPECTED_KVM_SERIAL_ARG_COUNT && args[1] == ARG_CHECK_KVM_SERIAL {
        let profile = read_profile(Path::new(&args[2]))?;
        let serial = fs::read_to_string(Path::new(&args[3]))?;
        let run = kvm_run_from_serial(&profile, &serial, DEFAULT_KVM_MAX_EXITS)?;
        let receipt = kernel_bundle_kvm_rail_receipt(&profile, &run)?;
        print_json(&receipt)?;
        return Ok(());
    }
    if has_arg(&args, ARG_KVM_RUN_PROFILE) {
        run_kvm_profile(&args)?;
        return Ok(());
    }
    if has_arg(&args, ARG_BUILD_PRIVATE_KFUNC_INITRD) {
        build_private_kfunc_initrd(&args)?;
        return Ok(());
    }
    if args.len() == EXPECTED_SINGLE_ARG_COUNT
        && matches!(args[1].as_str(), ARG_HELP_SHORT | ARG_HELP_LONG)
    {
        print_usage();
        return Ok(());
    }
    Err(EvidenceError::new(
        "usage error: expected --sample-profile, --sample-receipt, --sample-kvm-markers, --check-profile <path>, --check-kvm-serial <profile> <serial>, --build-private-kfunc-initrd <out> --artifacts-dir <dir> --busybox <path> --bpftool <path> --delete-module-helper <path> --closure-list <path>, or --kvm-run-profile <profile> --kernel <path> --initrd <path> --out <path> [--max-exits N] [--memory-mib N]",
    ))
}

fn build_private_kfunc_initrd(args: &[String]) -> EvidenceResult<()> {
    let out_path = required_arg(args, ARG_BUILD_PRIVATE_KFUNC_INITRD)?;
    let artifacts_dir = required_arg(args, ARG_ARTIFACTS_DIR)?;
    let busybox_path = required_arg(args, ARG_BUSYBOX)?;
    let bpftool_path = required_arg(args, ARG_BPFTOOL)?;
    let delete_module_helper_path = required_arg(args, ARG_DELETE_MODULE_HELPER)?;
    let closure_list_path = required_arg(args, ARG_CLOSURE_LIST)?;
    let expected_kernel_release = optional_arg(args, ARG_EXPECTED_KERNEL_RELEASE)
        .unwrap_or(PRIVATE_KFUNC_EXPECTED_KERNEL_RELEASE);
    let request = PrivateKfuncInitrdRequest {
        output_path: Path::new(out_path),
        artifacts_dir: Path::new(artifacts_dir),
        busybox_path: Path::new(busybox_path),
        bpftool_path: Path::new(bpftool_path),
        delete_module_helper_path: Path::new(delete_module_helper_path),
        closure_list_path: Path::new(closure_list_path),
        expected_kernel_release,
    };
    let summary = write_private_kfunc_initrd(&request)?;
    print_json(&summary)?;
    Ok(())
}

fn run_kvm_profile(args: &[String]) -> EvidenceResult<()> {
    let profile_path = required_arg(args, ARG_KVM_RUN_PROFILE)?;
    let kernel_path = required_arg(args, ARG_KERNEL)?;
    let initrd_path = required_arg(args, ARG_INITRD)?;
    let out_path = required_arg(args, ARG_OUT)?;
    let max_exits = optional_arg(args, ARG_MAX_EXITS)
        .map(parse_max_exits)
        .transpose()?
        .unwrap_or(DEFAULT_KVM_MAX_EXITS);
    let memory_mib = optional_arg(args, ARG_MEMORY_MIB)
        .map(parse_memory_mib)
        .transpose()?
        .unwrap_or(DEFAULT_MEMORY_MIB);
    let profile = read_profile(Path::new(profile_path))?;
    let run = execute_kvm_run(
        &profile,
        Path::new(kernel_path),
        Path::new(initrd_path),
        max_exits,
        memory_mib,
    )?;
    let receipt = kernel_bundle_kvm_rail_receipt(&profile, &run)?;
    write_json(Path::new(out_path), &receipt)?;
    eprintln!(
        "{STATUS_STDERR_PREFIX}: status={} receipt={} out={}",
        receipt.status, receipt.receipt_identity_blake3, out_path
    );
    Ok(())
}

fn execute_kvm_run(
    profile: &KernelBundleSmokeProfile,
    kernel_path: &Path,
    initrd_path: &Path,
    max_exits: u64,
    memory_mib: usize,
) -> EvidenceResult<KernelBundleKvmRun> {
    validate_kernel_bundle_smoke_profile(profile)?;
    let profile_id = kernel_bundle_smoke_profile_identity(profile)?;
    if !Path::new("/dev/kvm").exists() {
        return Ok(blocked_run(
            profile_id,
            max_exits,
            "kvm-device-missing".to_string(),
        ));
    }
    if !kernel_path.is_file() || !initrd_path.is_file() {
        return Ok(loader_blocked_run(
            profile_id,
            max_exits,
            format!(
                "missing kernel or initrd: kernel={} initrd={}",
                kernel_path.display(),
                initrd_path.display()
            ),
        ));
    }
    let mut config = VmConfig::default();
    config.memory_size = memory_mib.saturating_mul(BYTES_PER_MIB);
    let mut vm = match DeterministicVm::new(config) {
        Ok(vm) => vm,
        Err(error) => {
            return Ok(blocked_run(
                profile_id,
                max_exits,
                format!("kvm-create:{error}"),
            ))
        }
    };
    let kernel_image_blake3 = hash_file_blake3(kernel_path)?;
    let initrd_image_blake3 = hash_file_blake3(initrd_path)?;
    if let Err(error) = vm.load_kernel(path_str(kernel_path)?, Some(path_str(initrd_path)?)) {
        return Ok(KernelBundleKvmRun {
            profile_identity_blake3: profile_id,
            runner: DEFAULT_RUNNER_ID.to_string(),
            execution_mode: EXECUTION_MODE_EXACT_KVM.to_string(),
            kernel_image_blake3: Some(kernel_image_blake3),
            initrd_image_blake3: Some(initrd_image_blake3),
            kvm_available: true,
            loader_available: false,
            max_exits,
            exits_executed: 0,
            halted: false,
            observations: Vec::new(),
            failure_class: Some(format!("kernel-load:{error}")),
        });
    }
    let (exits_executed, halted, serial, failure_class) =
        run_until_kvm_observations(&mut vm, profile, max_exits)?;
    Ok(KernelBundleKvmRun {
        profile_identity_blake3: profile_id,
        runner: DEFAULT_RUNNER_ID.to_string(),
        execution_mode: EXECUTION_MODE_EXACT_KVM.to_string(),
        kernel_image_blake3: Some(kernel_image_blake3),
        initrd_image_blake3: Some(initrd_image_blake3),
        kvm_available: true,
        loader_available: true,
        max_exits,
        exits_executed,
        halted,
        observations: extract_kvm_observations(&serial),
        failure_class,
    })
}

fn run_until_kvm_observations(
    vm: &mut DeterministicVm,
    profile: &KernelBundleSmokeProfile,
    max_exits: u64,
) -> EvidenceResult<(u64, bool, String, Option<String>)> {
    let expected = expected_kernel_bundle_kvm_observations(profile);
    let mut serial = String::new();
    let mut exits_executed = 0_u64;
    let mut halted = false;
    let mut failure_class = None;
    while exits_executed < max_exits {
        let remaining_exits = max_exits.saturating_sub(exits_executed);
        let chunk_exits_bound = remaining_exits.min(KVM_OBSERVATION_POLL_EXITS);
        debug_assert!(chunk_exits_bound > 0);
        match vm.run_bounded(chunk_exits_bound) {
            Ok((chunk_exits, chunk_halted)) => {
                exits_executed = exits_executed.saturating_add(chunk_exits);
                halted = chunk_halted;
                serial.push_str(&vm.take_serial_output());
                let observations = extract_kvm_observations(&serial);
                if let Some(error) = guest_error_observation(&observations) {
                    failure_class = Some(format!("guest-error:{}:{}", error.case_id, error.detail));
                    break;
                }
                if observations_complete(&observations, &expected) {
                    break;
                }
                if halted {
                    break;
                }
                if chunk_exits == 0 {
                    failure_class = Some("vm-run:no-progress".to_string());
                    break;
                }
            }
            Err(error) => {
                serial.push_str(&vm.take_serial_output());
                failure_class = Some(format!("vm-run:{error}"));
                break;
            }
        }
    }
    Ok((exits_executed, halted, serial, failure_class))
}

fn guest_error_observation(observations: &[SmokeObservation]) -> Option<&SmokeObservation> {
    observations
        .iter()
        .find(|observation| observation.class == GUEST_ERROR_CLASS)
}

fn observations_complete(observations: &[SmokeObservation], expected: &[SmokeObservation]) -> bool {
    expected
        .iter()
        .all(|expected_observation| observations.contains(expected_observation))
}

fn kvm_run_from_serial(
    profile: &KernelBundleSmokeProfile,
    serial: &str,
    max_exits: u64,
) -> EvidenceResult<KernelBundleKvmRun> {
    Ok(KernelBundleKvmRun {
        profile_identity_blake3: kernel_bundle_smoke_profile_identity(profile)?,
        runner: DEFAULT_RUNNER_ID.to_string(),
        execution_mode: EXECUTION_MODE_SERIAL_TRANSCRIPT.to_string(),
        kernel_image_blake3: None,
        initrd_image_blake3: None,
        kvm_available: true,
        loader_available: true,
        max_exits,
        exits_executed: max_exits,
        halted: false,
        observations: extract_kvm_observations(serial),
        failure_class: None,
    })
}

fn blocked_run(
    profile_identity_blake3: String,
    max_exits: u64,
    reason: String,
) -> KernelBundleKvmRun {
    KernelBundleKvmRun {
        profile_identity_blake3,
        runner: DEFAULT_RUNNER_ID.to_string(),
        execution_mode: EXECUTION_MODE_EXACT_KVM.to_string(),
        kernel_image_blake3: None,
        initrd_image_blake3: None,
        kvm_available: false,
        loader_available: true,
        max_exits,
        exits_executed: 0,
        halted: false,
        observations: Vec::new(),
        failure_class: Some(reason),
    }
}

fn loader_blocked_run(
    profile_identity_blake3: String,
    max_exits: u64,
    reason: String,
) -> KernelBundleKvmRun {
    KernelBundleKvmRun {
        profile_identity_blake3,
        runner: DEFAULT_RUNNER_ID.to_string(),
        execution_mode: EXECUTION_MODE_EXACT_KVM.to_string(),
        kernel_image_blake3: None,
        initrd_image_blake3: None,
        kvm_available: true,
        loader_available: false,
        max_exits,
        exits_executed: 0,
        halted: false,
        observations: Vec::new(),
        failure_class: Some(reason),
    }
}

fn hash_file_blake3(path: &Path) -> EvidenceResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; FILE_HASH_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn read_profile(path: &Path) -> EvidenceResult<KernelBundleSmokeProfile> {
    let text = fs::read_to_string(path)?;
    let profile = serde_json::from_str(&text)?;
    Ok(profile)
}

fn path_str(path: &Path) -> EvidenceResult<&str> {
    path.to_str()
        .ok_or_else(|| EvidenceError::new(format!("path is not UTF-8: {}", path.display())))
}

fn parse_memory_mib(value: &str) -> EvidenceResult<usize> {
    let parsed = value
        .parse::<usize>()
        .map_err(|err| EvidenceError::new(format!("invalid --memory-mib {value:?}: {err}")))?;
    if !(MIN_MEMORY_MIB..=MAX_MEMORY_MIB).contains(&parsed) {
        return Err(EvidenceError::new(format!(
            "--memory-mib must be between {MIN_MEMORY_MIB} and {MAX_MEMORY_MIB}: {parsed}"
        )));
    }
    Ok(parsed)
}

fn parse_max_exits(value: &str) -> EvidenceResult<u64> {
    value
        .parse::<u64>()
        .map_err(|err| EvidenceError::new(format!("invalid --max-exits {value:?}: {err}")))
}

fn has_arg(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn required_arg<'a>(args: &'a [String], flag: &str) -> EvidenceResult<&'a str> {
    optional_arg(args, flag).ok_or_else(|| EvidenceError::new(format!("missing required {flag}")))
}

fn optional_arg<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(EXPECTED_SINGLE_ARG_COUNT)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
}

fn print_json<T: serde::Serialize>(value: &T) -> EvidenceResult<()> {
    let rendered = serde_json::to_string_pretty(value)?;
    println!("{rendered}");
    Ok(())
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> EvidenceResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let rendered = serde_json::to_string_pretty(value)?;
    fs::write(path, format!("{rendered}\n"))?;
    Ok(())
}

fn print_usage() {
    println!("kernel-bundle-vm-compat-smoke --sample-profile");
    println!("kernel-bundle-vm-compat-smoke --sample-receipt");
    println!("kernel-bundle-vm-compat-smoke --sample-kvm-markers");
    println!("kernel-bundle-vm-compat-smoke --check-profile <profile.json>");
    println!("kernel-bundle-vm-compat-smoke --check-kvm-serial <profile.json> <serial.txt>");
    println!("kernel-bundle-vm-compat-smoke --build-private-kfunc-initrd <out.cpio> --artifacts-dir <dir> --busybox <busybox> --bpftool <bpftool> --delete-module-helper <helper> --closure-list <store-paths.txt> [--expected-kernel-release 6.18.20]");
    println!("kernel-bundle-vm-compat-smoke --kvm-run-profile <profile.json> --kernel <vmlinux> --initrd <initrd.cpio> --out <receipt.json> [--max-exits N] [--memory-mib 1024]");
}
