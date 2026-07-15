use std::fs;
use std::path::Path;

use chaoscontrol_evidence::{
    extract_kvm_observations, kernel_bundle_kvm_rail_receipt, kernel_bundle_smoke_profile_identity,
    kernel_bundle_smoke_receipt, sample_mantle_private_kfunc_kvm_markers,
    sample_mantle_private_kfunc_profile, validate_kernel_bundle_smoke_profile, EvidenceError,
    EvidenceResult, KernelBundleKvmRun, KernelBundleSmokeProfile, DEFAULT_KVM_MAX_EXITS,
};
use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};

const ARG_SAMPLE_PROFILE: &str = "--sample-profile";
const ARG_SAMPLE_RECEIPT: &str = "--sample-receipt";
const ARG_SAMPLE_KVM_MARKERS: &str = "--sample-kvm-markers";
const ARG_CHECK_PROFILE: &str = "--check-profile";
const ARG_CHECK_KVM_SERIAL: &str = "--check-kvm-serial";
const ARG_KVM_RUN_PROFILE: &str = "--kvm-run-profile";
const ARG_KERNEL: &str = "--kernel";
const ARG_INITRD: &str = "--initrd";
const ARG_OUT: &str = "--out";
const ARG_MAX_EXITS: &str = "--max-exits";
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
    if args.len() == EXPECTED_SINGLE_ARG_COUNT
        && matches!(args[1].as_str(), ARG_HELP_SHORT | ARG_HELP_LONG)
    {
        print_usage();
        return Ok(());
    }
    Err(EvidenceError::new(
        "usage error: expected --sample-profile, --sample-receipt, --sample-kvm-markers, --check-profile <path>, --check-kvm-serial <profile> <serial>, or --kvm-run-profile <profile> --kernel <path> --initrd <path> --out <path> [--max-exits N]",
    ))
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
    let profile = read_profile(Path::new(profile_path))?;
    let run = execute_kvm_run(
        &profile,
        Path::new(kernel_path),
        Path::new(initrd_path),
        max_exits,
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
    let mut vm = match DeterministicVm::new(VmConfig::default()) {
        Ok(vm) => vm,
        Err(error) => {
            return Ok(blocked_run(
                profile_id,
                max_exits,
                format!("kvm-create:{error}"),
            ))
        }
    };
    if let Err(error) = vm.load_kernel(path_str(kernel_path)?, Some(path_str(initrd_path)?)) {
        return Ok(KernelBundleKvmRun {
            profile_identity_blake3: profile_id,
            runner: DEFAULT_RUNNER_ID.to_string(),
            kvm_available: true,
            loader_available: false,
            max_exits,
            exits_executed: 0,
            halted: false,
            observations: Vec::new(),
            failure_class: Some(format!("kernel-load:{error}")),
        });
    }
    let (exits_executed, halted) = match vm.run_bounded(max_exits) {
        Ok(result) => result,
        Err(error) => {
            let serial = vm.take_serial_output();
            return Ok(KernelBundleKvmRun {
                profile_identity_blake3: profile_id,
                runner: DEFAULT_RUNNER_ID.to_string(),
                kvm_available: true,
                loader_available: true,
                max_exits,
                exits_executed: 0,
                halted: false,
                observations: extract_kvm_observations(&serial),
                failure_class: Some(format!("vm-run:{error}")),
            });
        }
    };
    let serial = vm.take_serial_output();
    Ok(KernelBundleKvmRun {
        profile_identity_blake3: profile_id,
        runner: DEFAULT_RUNNER_ID.to_string(),
        kvm_available: true,
        loader_available: true,
        max_exits,
        exits_executed,
        halted,
        observations: extract_kvm_observations(&serial),
        failure_class: None,
    })
}

fn kvm_run_from_serial(
    profile: &KernelBundleSmokeProfile,
    serial: &str,
    max_exits: u64,
) -> EvidenceResult<KernelBundleKvmRun> {
    Ok(KernelBundleKvmRun {
        profile_identity_blake3: kernel_bundle_smoke_profile_identity(profile)?,
        runner: DEFAULT_RUNNER_ID.to_string(),
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
        kvm_available: true,
        loader_available: false,
        max_exits,
        exits_executed: 0,
        halted: false,
        observations: Vec::new(),
        failure_class: Some(reason),
    }
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
    println!("kernel-bundle-vm-compat-smoke --kvm-run-profile <profile.json> --kernel <vmlinux> --initrd <initrd.gz> --out <receipt.json> [--max-exits N]");
}
