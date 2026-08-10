//! Determinism stress gate: run the same seed N times and verify every run
//! produces identical VM/controller fingerprints.
//!
//! Usage:
//!   `determinism_stress <kernel-path> <initrd-path> [N=10] [--receipt path] [--dlog-dir dir]`
//!       `[--case name] [--single-clock-profile tsc|jiffies|hide-tsc] [--matrix-receipt path]`
//!
//! The default single-VM/controller clock profile is `hide-tsc`, the current
//! bounded operator profile with committed passing drift evidence. Use
//! `--single-clock-profile tsc` for legacy baseline A/B diagnosis.

use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::schedule::FaultScheduleBuilder;
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController};
use chaoscontrol_vmm::determinism_gate::{
    compare_case, refresh_divergence_classes, validate_determinism_matrix_receipt,
    ControllerFingerprint, DeterminismCaseReport, DeterminismGateReceipt, DeterminismMatrixProfile,
    DeterminismMatrixReceipt, DeterminismMatrixRow, DivergenceClass, DlogDivergenceDetail,
    RunFingerprint, RunObservation, VmFingerprint,
};
use chaoscontrol_vmm::dlog::{dlog_diff_structural, DiffResult, DlogRecord, DlogTag};
use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
use crc32fast::Hasher;
use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;

const DEFAULT_RUNS: usize = 10;
const SINGLE_VM_MAX_EXITS: u64 = 70_000;
const CONTROLLER_TICKS: u64 = 10;
const CONTROLLER_SEED: u64 = 42;
const DLOG_REGISTER_INTERVAL: u64 = 100;

#[derive(Debug, Clone, Copy)]
struct VmClockProfile {
    extra_cmdline: Option<&'static str>,
    hide_tsc: bool,
}

#[derive(Debug, Clone, Copy)]
struct ControllerRunConfig {
    num_vms: usize,
    num_vcpus: usize,
    seed: u64,
    ticks: u64,
    clock: VmClockProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SingleClockProfile {
    Tsc,
    Jiffies,
    HideTsc,
}

impl SingleClockProfile {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "tsc" => Some(Self::Tsc),
            "jiffies" => Some(Self::Jiffies),
            "hide-tsc" => Some(Self::HideTsc),
            _ => None,
        }
    }

    fn extra_cmdline(self) -> Option<&'static str> {
        match self {
            Self::Tsc => None,
            // Appended after the default single-vCPU clock parameters; Linux
            // treats later duplicate command-line keys as the effective value.
            Self::Jiffies | Self::HideTsc => Some("clocksource=jiffies notsc"),
        }
    }

    fn hide_tsc(self) -> bool {
        matches!(self, Self::HideTsc)
    }

    fn vm_profile(self) -> VmClockProfile {
        VmClockProfile {
            extra_cmdline: self.extra_cmdline(),
            hide_tsc: self.hide_tsc(),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Tsc => "tsc",
            Self::Jiffies => "jiffies",
            Self::HideTsc => "hide-tsc",
        }
    }
}

#[derive(Debug)]
struct Args {
    kernel: String,
    initrd: String,
    runs: usize,
    receipt: Option<PathBuf>,
    matrix_receipt: Option<PathBuf>,
    dlog_dir: Option<PathBuf>,
    cases: Vec<String>,
    single_clock_profile: SingleClockProfile,
}

fn parse_args() -> Args {
    let args = env::args().skip(1).collect::<Vec<_>>();
    parse_args_from(args).unwrap_or_else(|err| usage_and_exit(&err))
}

fn parse_args_from(args: Vec<String>) -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut receipt = None;
    let mut matrix_receipt = None;
    let mut dlog_dir = None;
    let mut cases = Vec::new();
    let mut single_clock_profile = SingleClockProfile::HideTsc;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--receipt" => {
                receipt = Some(PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--receipt requires a path".to_string())?,
                ));
            }
            "--matrix-receipt" => {
                matrix_receipt =
                    Some(PathBuf::from(iter.next().ok_or_else(|| {
                        "--matrix-receipt requires a path".to_string()
                    })?));
            }
            "--dlog-dir" => {
                dlog_dir =
                    Some(PathBuf::from(iter.next().ok_or_else(|| {
                        "--dlog-dir requires a directory".to_string()
                    })?));
            }
            "--case" => {
                cases.push(
                    iter.next()
                        .ok_or_else(|| "--case requires a case name".to_string())?,
                );
            }
            "--single-clock-profile" => {
                let value = iter.next().ok_or_else(|| {
                    "--single-clock-profile requires tsc, jiffies, or hide-tsc".to_string()
                })?;
                single_clock_profile = SingleClockProfile::parse(&value).ok_or_else(|| {
                    format!("unknown --single-clock-profile {value:?}; expected tsc, jiffies, or hide-tsc")
                })?;
            }
            "--help" | "-h" => usage_and_exit_code("", 0),
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}")),
            _ => positional.push(arg),
        }
    }

    if positional.len() < 2 || positional.len() > 3 {
        return Err("expected <kernel-path> <initrd-path> [N]".to_string());
    }

    let runs = positional
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_RUNS);

    Ok(Args {
        kernel: positional[0].clone(),
        initrd: positional[1].clone(),
        runs,
        receipt,
        matrix_receipt,
        dlog_dir,
        cases,
        single_clock_profile,
    })
}

fn usage_and_exit(message: &str) -> ! {
    usage_and_exit_code(message, 2)
}

fn usage_and_exit_code(message: &str, code: i32) -> ! {
    if !message.is_empty() {
        eprintln!("error: {message}\n");
    }
    eprintln!(
        "Usage: determinism_stress <kernel-path> <initrd-path> [N={DEFAULT_RUNS}] [--receipt path] [--matrix-receipt path] [--dlog-dir dir] [--case name] [--single-clock-profile tsc|jiffies|hide-tsc]\n\nDefault profile: hide-tsc (bounded operator drift gate); use --single-clock-profile tsc for legacy baseline A/B."
    );
    std::process::exit(code);
}

fn case_enabled(args: &Args, name: &str) -> bool {
    args.cases.is_empty() || args.cases.iter().any(|case| case == name)
}

/// Strip known non-deterministic lines from serial output.
fn strip_nondeterministic(s: &str) -> String {
    s.lines()
        .filter(|line| {
            let stripped = line.trim();
            !stripped.contains("Detected") || !stripped.contains("MHz processor")
        })
        .filter(|line| {
            let stripped = line.trim();
            !stripped.contains("Memory:") || !stripped.contains("available")
        })
        .map(|line| {
            // Strip kernel timestamp prefix: [    0.123456]
            if let Some(pos) = line.find("] ") {
                &line[pos + 2..]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn run_single_vm(
    kernel: &str,
    initrd: &str,
    num_vcpus: usize,
    max_exits: u64,
    dlog_path: Option<PathBuf>,
    extra_cmdline: Option<&str>,
    hide_tsc: bool,
) -> VmFingerprint {
    let schedule_journal_limit =
        usize::try_from(max_exits).expect("the bounded exit count must fit the host address width");
    assert!(
        schedule_journal_limit > 0,
        "the exit bound must be positive"
    );
    let config = VmConfig {
        num_vcpus,
        dlog_path,
        dlog_register_interval: DLOG_REGISTER_INTERVAL,
        smp_schedule_journal_limit: schedule_journal_limit,
        extra_cmdline: extra_cmdline.map(str::to_string),
        cpu: chaoscontrol_vmm::cpu::CpuConfig {
            hide_tsc,
            ..VmConfig::default().cpu
        },
        ..Default::default()
    };
    let mut vm = DeterministicVm::new(config).expect("create VM");
    vm.load_kernel(kernel, Some(initrd)).expect("load kernel");

    let mut output = String::new();
    let mut remaining = max_exits;
    while remaining > 0 {
        let chunk = remaining.min(10_000);
        let (ran, halted) = vm.run_bounded(chunk).expect("run");
        output.push_str(&vm.take_serial_output());
        remaining -= ran;
        if halted {
            break;
        }
    }

    VmFingerprint {
        exit_count: vm.exit_count(),
        virtual_tsc: vm.virtual_tsc(),
        serial_stripped: strip_nondeterministic(&output),
    }
}

fn run_controller(
    kernel: &str,
    initrd: &str,
    config: ControllerRunConfig,
    dlog_dir: Option<PathBuf>,
) -> ControllerFingerprint {
    let ControllerRunConfig {
        num_vms,
        num_vcpus,
        seed,
        ticks,
        clock,
    } = config;
    let schedule = FaultScheduleBuilder::new()
        .at_ns(
            2_000_000,
            Fault::NetworkLatency {
                target: 0,
                latency_ns: 10,
            },
        )
        .at_ns(
            3_000_000,
            Fault::PacketLoss {
                target: 1 % num_vms,
                rate_ppm: 100_000,
            },
        )
        .build();

    let config = SimulationConfig {
        num_vms,
        vm_config: VmConfig {
            num_vcpus,
            dlog_register_interval: DLOG_REGISTER_INTERVAL,
            extra_cmdline: clock.extra_cmdline.map(str::to_string),
            cpu: chaoscontrol_vmm::cpu::CpuConfig {
                hide_tsc: clock.hide_tsc,
                ..VmConfig::default().cpu
            },
            ..Default::default()
        },
        kernel_path: kernel.to_string(),
        initrd_path: Some(initrd.to_string()),
        seed,
        quantum: 5000,
        schedule,
        disk_image_path: None,
        base_core: None,
        dlog_dir,
        bootstrap_budget: None,
    };

    let mut ctrl = SimulationController::new(config).expect("create controller");
    ctrl.force_setup_complete();

    for _ in 0..ticks {
        ctrl.step_round().expect("step");
    }

    let mut vm_exits = Vec::new();
    let mut vm_vtscs = Vec::new();
    let mut serials_stripped = Vec::new();

    for i in 0..num_vms {
        let slot = ctrl.vm_slot(i).unwrap();
        vm_exits.push(slot.vm.exit_count());
        vm_vtscs.push(slot.vm.virtual_tsc());
        serials_stripped.push(String::new());
    }

    ControllerFingerprint {
        tick: ctrl.tick(),
        vm_exits,
        vm_vtscs,
        serials_stripped,
    }
}

fn run_case<F>(
    name: &str,
    runs: usize,
    dlog_root: Option<&Path>,
    mut run_once: F,
) -> DeterminismCaseReport
where
    F: FnMut(usize, Option<PathBuf>) -> RunFingerprint,
{
    println!("━━━ {name}: {runs} runs ━━━");
    let start = Instant::now();
    let mut observations = Vec::new();

    for run_index in 1..=runs {
        let dlog_path = dlog_root.map(|root| dlog_path_for(root, name, run_index));
        if let Some(path) = &dlog_path {
            if path.extension().is_some() {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).expect("create dlog parent");
                }
            } else {
                std::fs::create_dir_all(path).expect("create dlog directory");
            }
        }

        let fingerprint = run_once(run_index, dlog_path.clone());
        print_observation(run_index, &fingerprint, run_index == 1);
        observations.push(RunObservation {
            run_index,
            fingerprint,
            dlog_path: dlog_path.map(|p| p.display().to_string()),
        });
    }

    let mut report = compare_case(name, observations);
    let dlog_divergences = compare_case_dlogs(&report);
    report.dlog_structural_match = dlog_divergences.as_ref().map(Vec::is_empty);
    report.dlog_divergences = dlog_divergences.unwrap_or_default();
    report.dlog_mismatches = report
        .dlog_divergences
        .iter()
        .map(|detail| detail.summary.clone())
        .collect();
    if !report.dlog_divergences.is_empty() {
        report.passed = false;
    }
    refresh_divergence_classes(&mut report);

    let elapsed = start.elapsed();
    if report.passed {
        println!(
            "  ✅ PASS: {}/{} runs identical ({:.1}s)\n",
            runs,
            runs,
            elapsed.as_secs_f64()
        );
    } else {
        println!(
            "  ❌ FAIL: {} fingerprint mismatch(es), {} dlog mismatch(es) across {} runs ({:.1}s)",
            report.mismatches.len(),
            report.dlog_mismatches.len(),
            runs,
            elapsed.as_secs_f64()
        );
        for mismatch in &report.mismatches {
            eprintln!(
                "         run {} {}: {} vs reference {}",
                mismatch.run_index, mismatch.field, mismatch.actual, mismatch.expected
            );
        }
        for mismatch in &report.dlog_mismatches {
            eprintln!("         {mismatch}");
        }
        println!();
    }

    report
}

fn dlog_path_for(root: &Path, case_name: &str, run_index: usize) -> PathBuf {
    let safe_name = case_name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    if case_name.contains("controller") {
        root.join(format!("{safe_name}-run-{run_index}"))
    } else {
        root.join(format!("{safe_name}-run-{run_index}.dlog"))
    }
}

fn compare_case_dlogs(report: &DeterminismCaseReport) -> Option<Vec<DlogDivergenceDetail>> {
    let reference_path = report.observations.first()?.dlog_path.as_deref()?;
    let mut mismatches = Vec::new();
    for observation in report.observations.iter().skip(1) {
        let Some(path) = observation.dlog_path.as_deref() else {
            mismatches.push(DlogDivergenceDetail {
                run_index: observation.run_index,
                class: DivergenceClass::DlogCompareError,
                summary: format!(
                    "run {} missing dlog path for comparison against {reference_path}",
                    observation.run_index
                ),
            });
            continue;
        };
        mismatches.extend(structural_dlog_path_mismatches(
            Path::new(reference_path),
            Path::new(path),
            observation.run_index,
        ));
    }
    Some(mismatches)
}

fn structural_dlog_path_mismatches(
    reference: &Path,
    actual: &Path,
    run_index: usize,
) -> Vec<DlogDivergenceDetail> {
    if reference.is_dir() || actual.is_dir() {
        let mut mismatches = Vec::new();
        for vm_idx in 0..64 {
            let ref_file = reference.join(format!("vm_{vm_idx}.dlog"));
            let actual_file = actual.join(format!("vm_{vm_idx}.dlog"));
            if !ref_file.exists() && !actual_file.exists() {
                break;
            }
            if let Some(mismatch) = dlog_file_mismatch(&ref_file, &actual_file, run_index) {
                mismatches.push(mismatch);
            }
        }
        mismatches
    } else {
        dlog_file_mismatch(reference, actual, run_index)
            .into_iter()
            .collect()
    }
}

fn dlog_file_mismatch(
    reference: &Path,
    actual: &Path,
    run_index: usize,
) -> Option<DlogDivergenceDetail> {
    match dlog_diff_structural(reference, actual) {
        Ok(DiffResult::Identical { .. }) => None,
        Ok(diff) => Some(DlogDivergenceDetail {
            run_index,
            class: classify_dlog_diff(&diff),
            summary: format!(
                "run {run_index} dlog structural mismatch: {} vs {}: {diff}",
                reference.display(),
                actual.display()
            ),
        }),
        Err(err) => Some(DlogDivergenceDetail {
            run_index,
            class: DivergenceClass::DlogCompareError,
            summary: format!(
                "run {run_index} dlog structural compare failed: {} vs {}: {err}",
                reference.display(),
                actual.display()
            ),
        }),
    }
}

fn classify_dlog_diff(diff: &DiffResult) -> DivergenceClass {
    match diff {
        DiffResult::Identical { .. } => DivergenceClass::Unknown,
        DiffResult::LengthMismatch { .. } => DivergenceClass::DlogLength,
        DiffResult::Diverged {
            record_a, record_b, ..
        } => classify_dlog_records(record_a, record_b),
    }
}

fn classify_dlog_records(a: &DlogRecord, b: &DlogRecord) -> DivergenceClass {
    if a.vcpu != b.vcpu
        || matches!(a.tag(), Some(DlogTag::SchedulerSwitch))
        || matches!(b.tag(), Some(DlogTag::SchedulerSwitch))
    {
        return DivergenceClass::VcpuScheduling;
    }

    let ports = [record_port(a), record_port(b)];
    if ports
        .iter()
        .flatten()
        .any(|port| matches!(*port, 0x0cf8 | 0x0cfc))
    {
        return DivergenceClass::PciConfigAccess;
    }
    if ports
        .iter()
        .flatten()
        .any(|port| matches!(*port, 0x0070 | 0x0071))
    {
        return DivergenceClass::RtcCmosAccess;
    }
    if ports
        .iter()
        .flatten()
        .any(|port| matches!(*port, 0x0040..=0x0043 | 0x0061))
    {
        return DivergenceClass::PitTscCalibration;
    }
    let port_count = ports.iter().flatten().count();
    if port_count > 0
        && ports
            .iter()
            .flatten()
            .all(|port| matches!(*port, 0x03f8 | 0x03fd))
    {
        return DivergenceClass::SerialByteStream;
    }

    if matches!(a.tag(), Some(DlogTag::MmioRead | DlogTag::MmioWrite))
        || matches!(b.tag(), Some(DlogTag::MmioRead | DlogTag::MmioWrite))
    {
        DivergenceClass::MmioDeviceAccess
    } else if matches!(a.tag(), Some(DlogTag::IoIn | DlogTag::IoOut))
        || matches!(b.tag(), Some(DlogTag::IoIn | DlogTag::IoOut))
    {
        DivergenceClass::DeviceIoAccess
    } else {
        DivergenceClass::Unknown
    }
}

fn record_port(record: &DlogRecord) -> Option<u16> {
    matches!(record.tag(), Some(DlogTag::IoIn | DlogTag::IoOut)).then_some(record.port_or_addr_lo)
}

fn print_observation(run_index: usize, fingerprint: &RunFingerprint, reference: bool) {
    let suffix = if reference {
        " ✅ (reference)"
    } else {
        " ✅"
    };
    match fingerprint {
        RunFingerprint::SingleVm(fp) => println!(
            "  run {run_index:>2}: exits={:<8} vtsc={:<16}{suffix}",
            fp.exit_count, fp.virtual_tsc
        ),
        RunFingerprint::Controller(fp) => println!(
            "  run {run_index:>2}: tick={:<4} exits={:?}{suffix}",
            fp.tick, fp.vm_exits
        ),
    }
}

fn crc32_file(path: &str) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("crc32:{:08x}", hasher.finalize()))
}

fn profile_for_case(
    case_name: &str,
    kernel_fingerprint: &str,
    initrd_fingerprint: &str,
    clock_profile: SingleClockProfile,
) -> DeterminismMatrixProfile {
    DeterminismMatrixProfile {
        row_id: format!("rust-workload-{case_name}-{}", clock_profile.as_str()),
        workload: "rust-workload".to_string(),
        kernel_fingerprint: kernel_fingerprint.to_string(),
        initrd_fingerprint: initrd_fingerprint.to_string(),
        device_profile: device_profile_for_case(case_name).to_string(),
        clock_profile: clock_profile.as_str().to_string(),
        controller_profile: case_name.to_string(),
        local_product_profile: "single-machine-multi-hypervisor".to_string(),
        worker_count: worker_count_for_case(case_name),
        hypervisor_profile: hypervisor_profile_for_case(case_name).to_string(),
    }
}

fn worker_count_for_case(case_name: &str) -> u32 {
    match case_name {
        "controller-3vm-1vcpu" | "controller-3vm-2vcpu" => 3,
        _ => 1,
    }
}

fn hypervisor_profile_for_case(case_name: &str) -> &'static str {
    match case_name {
        "controller-3vm-1vcpu" | "controller-3vm-2vcpu" => "local-kvm-controller-workers",
        _ => "local-kvm-single-worker",
    }
}

fn device_profile_for_case(case_name: &str) -> &'static str {
    match case_name {
        "single-vm-1vcpu" => "single-vm-virtio-console-1vcpu",
        "single-vm-2vcpu" => "single-vm-virtio-console-2vcpu",
        "controller-3vm-1vcpu" => "controller-3vm-network-faults-1vcpu",
        "controller-3vm-2vcpu" => "controller-3vm-network-faults-2vcpu",
        _ => "custom-determinism-case",
    }
}

fn build_matrix_receipt(
    matrix_id: &str,
    reports: &[DeterminismCaseReport],
    kernel_fingerprint: &str,
    initrd_fingerprint: &str,
    clock_profile: SingleClockProfile,
) -> DeterminismMatrixReceipt {
    let rows = reports
        .iter()
        .map(|report| DeterminismMatrixRow {
            profile: profile_for_case(
                &report.name,
                kernel_fingerprint,
                initrd_fingerprint,
                clock_profile,
            ),
            report: report.clone(),
            status: if report.passed {
                chaoscontrol_vmm::determinism_gate::MatrixRowStatus::Passed
            } else {
                chaoscontrol_vmm::determinism_gate::MatrixRowStatus::Failed
            },
        })
        .collect::<Vec<_>>();
    let required_profiles = rows
        .iter()
        .map(|row| row.profile.clone())
        .collect::<Vec<_>>();
    let receipt = DeterminismMatrixReceipt::new(matrix_id, rows);
    validate_determinism_matrix_receipt(&receipt, &required_profiles)
        .expect("matrix receipt built from local case reports validates");
    receipt
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args = parse_args();

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!(
        "║       Determinism Stress Gate — {} runs per config           ║",
        args.runs
    );
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    let dlog_root = args.dlog_dir.as_deref();
    let vm_clock_profile = args.single_clock_profile.vm_profile();
    let mut cases = Vec::new();

    if case_enabled(&args, "single-vm-1vcpu") {
        cases.push(run_case(
            "single-vm-1vcpu",
            args.runs,
            dlog_root,
            |_, dlog_path| {
                RunFingerprint::SingleVm(run_single_vm(
                    &args.kernel,
                    &args.initrd,
                    1,
                    SINGLE_VM_MAX_EXITS,
                    dlog_path,
                    vm_clock_profile.extra_cmdline,
                    vm_clock_profile.hide_tsc,
                ))
            },
        ));
    }
    if case_enabled(&args, "single-vm-2vcpu") {
        cases.push(run_case(
            "single-vm-2vcpu",
            args.runs,
            dlog_root,
            |_, dlog_path| {
                RunFingerprint::SingleVm(run_single_vm(
                    &args.kernel,
                    &args.initrd,
                    2,
                    SINGLE_VM_MAX_EXITS,
                    dlog_path,
                    vm_clock_profile.extra_cmdline,
                    vm_clock_profile.hide_tsc,
                ))
            },
        ));
    }
    if case_enabled(&args, "controller-3vm-1vcpu") {
        cases.push(run_case(
            "controller-3vm-1vcpu",
            args.runs,
            dlog_root,
            |_, dlog_dir| {
                RunFingerprint::Controller(run_controller(
                    &args.kernel,
                    &args.initrd,
                    ControllerRunConfig {
                        num_vms: 3,
                        num_vcpus: 1,
                        seed: CONTROLLER_SEED,
                        ticks: CONTROLLER_TICKS,
                        clock: vm_clock_profile,
                    },
                    dlog_dir,
                ))
            },
        ));
    }
    if case_enabled(&args, "controller-3vm-2vcpu") {
        cases.push(run_case(
            "controller-3vm-2vcpu",
            args.runs,
            dlog_root,
            |_, dlog_dir| {
                RunFingerprint::Controller(run_controller(
                    &args.kernel,
                    &args.initrd,
                    ControllerRunConfig {
                        num_vms: 3,
                        num_vcpus: 2,
                        seed: CONTROLLER_SEED,
                        ticks: CONTROLLER_TICKS,
                        clock: vm_clock_profile,
                    },
                    dlog_dir,
                ))
            },
        ));
    }

    if cases.is_empty() {
        usage_and_exit("no cases selected; valid cases: single-vm-1vcpu, single-vm-2vcpu, controller-3vm-1vcpu, controller-3vm-2vcpu");
    }

    let kernel_crc32 = crc32_file(&args.kernel).unwrap_or_else(|err| format!("unavailable:{err}"));
    let initrd_crc32 = crc32_file(&args.initrd).unwrap_or_else(|err| format!("unavailable:{err}"));
    let receipt = DeterminismGateReceipt::new(
        args.kernel.clone(),
        args.initrd.clone(),
        kernel_crc32.clone(),
        initrd_crc32.clone(),
        cases,
    );

    if let Some(path) = &args.receipt {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create receipt parent");
        }
        let json = serde_json::to_string_pretty(&receipt).expect("serialize receipt");
        std::fs::write(path, format!("{json}\n")).expect("write receipt");
        println!("receipt: {}", path.display());
    }

    if let Some(path) = &args.matrix_receipt {
        let matrix = build_matrix_receipt(
            "bounded-operator-hide-tsc",
            &receipt.cases,
            &kernel_crc32,
            &initrd_crc32,
            args.single_clock_profile,
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create matrix receipt parent");
        }
        let json = serde_json::to_string_pretty(&matrix).expect("serialize matrix receipt");
        std::fs::write(path, format!("{json}\n")).expect("write matrix receipt");
        println!("matrix receipt: {}", path.display());
    }

    println!("╔══════════════════════════════════════════════════════════════╗");
    if receipt.passed {
        println!(
            "║  ✅ ALL CONFIGURATIONS DETERMINISTIC ({} runs each)         ║",
            args.runs
        );
    } else {
        println!("║  ❌ DETERMINISM FAILURES DETECTED                           ║");
    }
    println!("╚══════════════════════════════════════════════════════════════╝");

    if !receipt.passed {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn io(tag: DlogTag, port: u16) -> DlogRecord {
        DlogRecord::new(1, 1_000, 1, 0, tag, 0).with_port(port)
    }

    #[test]
    fn dlog_classifier_prefers_pci_config_when_serial_order_reaches_pci() {
        let serial = io(DlogTag::IoIn, 0x03fd);
        let pci = io(DlogTag::IoOut, 0x0cf8);
        assert_eq!(
            classify_dlog_records(&serial, &pci),
            DivergenceClass::PciConfigAccess
        );
    }

    #[test]
    fn dlog_classifier_distinguishes_rtc_and_length() {
        let serial = io(DlogTag::IoOut, 0x03f8);
        let rtc = io(DlogTag::IoOut, 0x0070);
        assert_eq!(
            classify_dlog_records(&serial, &rtc),
            DivergenceClass::RtcCmosAccess
        );
        assert_eq!(
            classify_dlog_diff(&DiffResult::LengthMismatch {
                matched: 3,
                len_a: 3,
                len_b: 4,
            }),
            DivergenceClass::DlogLength
        );
    }

    #[test]
    fn parse_args_accepts_matrix_receipt_path() {
        let args = parse_args_from(vec![
            "kernel".to_string(),
            "initrd".to_string(),
            "--matrix-receipt".to_string(),
            "matrix.json".to_string(),
        ])
        .unwrap();
        assert_eq!(args.matrix_receipt, Some(PathBuf::from("matrix.json")));
    }

    #[test]
    fn matrix_receipt_wraps_case_reports_with_bounded_profiles() {
        let report = compare_case(
            "single-vm-1vcpu",
            vec![RunObservation {
                run_index: 1,
                fingerprint: RunFingerprint::SingleVm(VmFingerprint {
                    exit_count: 1,
                    virtual_tsc: 2,
                    serial_stripped: "ready".to_string(),
                }),
                dlog_path: None,
            }],
        );
        let matrix = build_matrix_receipt(
            "test-matrix",
            &[report],
            "crc32:kernel",
            "crc32:initrd",
            SingleClockProfile::HideTsc,
        );
        assert!(matrix.passed);
        assert_eq!(matrix.gate, "vm-determinism-profile-matrix");
        assert_eq!(
            matrix.rows[0].profile.row_id,
            "rust-workload-single-vm-1vcpu-hide-tsc"
        );
        assert!(matrix.unlisted_profiles_unproven);
    }

    #[test]
    fn parse_args_accepts_narrow_jiffies_case() {
        let args = parse_args_from(vec![
            "kernel".to_string(),
            "initrd".to_string(),
            "5".to_string(),
            "--case".to_string(),
            "single-vm-1vcpu".to_string(),
            "--single-clock-profile".to_string(),
            "jiffies".to_string(),
        ])
        .unwrap();
        assert_eq!(args.runs, 5);
        assert_eq!(args.cases, vec!["single-vm-1vcpu"]);
        assert_eq!(args.single_clock_profile, SingleClockProfile::Jiffies);
        assert_eq!(
            args.single_clock_profile.extra_cmdline(),
            Some("clocksource=jiffies notsc")
        );
    }

    #[test]
    fn parse_args_defaults_to_hide_tsc_operator_profile() {
        let args = parse_args_from(vec!["kernel".to_string(), "initrd".to_string()]).unwrap();
        assert_eq!(args.single_clock_profile, SingleClockProfile::HideTsc);
        assert_eq!(
            args.single_clock_profile.extra_cmdline(),
            Some("clocksource=jiffies notsc")
        );
        assert!(args.single_clock_profile.hide_tsc());
    }

    #[test]
    fn parse_args_accepts_explicit_tsc_legacy_baseline() {
        let args = parse_args_from(vec![
            "kernel".to_string(),
            "initrd".to_string(),
            "--single-clock-profile".to_string(),
            "tsc".to_string(),
        ])
        .unwrap();
        assert_eq!(args.single_clock_profile, SingleClockProfile::Tsc);
        assert_eq!(args.single_clock_profile.extra_cmdline(), None);
        assert!(!args.single_clock_profile.hide_tsc());
    }

    #[test]
    fn parse_args_rejects_unknown_clock_profile() {
        let err = parse_args_from(vec![
            "kernel".to_string(),
            "initrd".to_string(),
            "--single-clock-profile".to_string(),
            "walltime".to_string(),
        ])
        .unwrap_err();
        assert!(err.contains("unknown --single-clock-profile"));
    }
}
