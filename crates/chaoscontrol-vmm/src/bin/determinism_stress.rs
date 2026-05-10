//! Determinism stress gate: run the same seed N times and verify every run
//! produces identical VM/controller fingerprints.
//!
//! Usage:
//!   determinism_stress <kernel-path> <initrd-path> [N=10] [--receipt path] [--dlog-dir dir]

use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::schedule::FaultScheduleBuilder;
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController};
use chaoscontrol_vmm::determinism_gate::{
    compare_case, ControllerFingerprint, DeterminismCaseReport, DeterminismGateReceipt,
    RunFingerprint, RunObservation, VmFingerprint,
};
use chaoscontrol_vmm::dlog::{dlog_diff_structural, DiffResult};
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

#[derive(Debug)]
struct Args {
    kernel: String,
    initrd: String,
    runs: usize,
    receipt: Option<PathBuf>,
    dlog_dir: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut positional = Vec::new();
    let mut receipt = None;
    let mut dlog_dir = None;
    let mut iter = env::args().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--receipt" => {
                receipt =
                    Some(PathBuf::from(iter.next().unwrap_or_else(|| {
                        usage_and_exit("--receipt requires a path")
                    })));
            }
            "--dlog-dir" => {
                dlog_dir =
                    Some(PathBuf::from(iter.next().unwrap_or_else(|| {
                        usage_and_exit("--dlog-dir requires a directory")
                    })));
            }
            "--help" | "-h" => usage_and_exit_code("", 0),
            _ if arg.starts_with('-') => usage_and_exit(&format!("unknown option: {arg}")),
            _ => positional.push(arg),
        }
    }

    if positional.len() < 2 || positional.len() > 3 {
        usage_and_exit("expected <kernel-path> <initrd-path> [N]");
    }

    let runs = positional
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_RUNS);

    Args {
        kernel: positional[0].clone(),
        initrd: positional[1].clone(),
        runs,
        receipt,
        dlog_dir,
    }
}

fn usage_and_exit(message: &str) -> ! {
    usage_and_exit_code(message, 2)
}

fn usage_and_exit_code(message: &str, code: i32) -> ! {
    if !message.is_empty() {
        eprintln!("error: {message}\n");
    }
    eprintln!(
        "Usage: determinism_stress <kernel-path> <initrd-path> [N={DEFAULT_RUNS}] [--receipt path] [--dlog-dir dir]"
    );
    std::process::exit(code);
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
) -> VmFingerprint {
    let config = VmConfig {
        num_vcpus,
        dlog_path,
        dlog_register_interval: DLOG_REGISTER_INTERVAL,
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
    num_vms: usize,
    num_vcpus: usize,
    seed: u64,
    ticks: u64,
    dlog_dir: Option<PathBuf>,
) -> ControllerFingerprint {
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
    let dlog_mismatches = compare_case_dlogs(&report);
    report.dlog_structural_match = dlog_mismatches.as_ref().map(Vec::is_empty);
    report.dlog_mismatches = dlog_mismatches.unwrap_or_default();
    if !report.dlog_mismatches.is_empty() {
        report.passed = false;
    }

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

fn compare_case_dlogs(report: &DeterminismCaseReport) -> Option<Vec<String>> {
    let reference_path = report.observations.first()?.dlog_path.as_deref()?;
    let mut mismatches = Vec::new();
    for observation in report.observations.iter().skip(1) {
        let Some(path) = observation.dlog_path.as_deref() else {
            mismatches.push(format!(
                "run {} missing dlog path for comparison against {reference_path}",
                observation.run_index
            ));
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
) -> Vec<String> {
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

fn dlog_file_mismatch(reference: &Path, actual: &Path, run_index: usize) -> Option<String> {
    match dlog_diff_structural(reference, actual) {
        Ok(DiffResult::Identical { .. }) => None,
        Ok(diff) => Some(format!(
            "run {run_index} dlog structural mismatch: {} vs {}: {diff}",
            reference.display(),
            actual.display()
        )),
        Err(err) => Some(format!(
            "run {run_index} dlog structural compare failed: {} vs {}: {err}",
            reference.display(),
            actual.display()
        )),
    }
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
    let cases = vec![
        run_case("single-vm-1vcpu", args.runs, dlog_root, |_, dlog_path| {
            RunFingerprint::SingleVm(run_single_vm(
                &args.kernel,
                &args.initrd,
                1,
                SINGLE_VM_MAX_EXITS,
                dlog_path,
            ))
        }),
        run_case("single-vm-2vcpu", args.runs, dlog_root, |_, dlog_path| {
            RunFingerprint::SingleVm(run_single_vm(
                &args.kernel,
                &args.initrd,
                2,
                SINGLE_VM_MAX_EXITS,
                dlog_path,
            ))
        }),
        run_case(
            "controller-3vm-1vcpu",
            args.runs,
            dlog_root,
            |_, dlog_dir| {
                RunFingerprint::Controller(run_controller(
                    &args.kernel,
                    &args.initrd,
                    3,
                    1,
                    CONTROLLER_SEED,
                    CONTROLLER_TICKS,
                    dlog_dir,
                ))
            },
        ),
        run_case(
            "controller-3vm-2vcpu",
            args.runs,
            dlog_root,
            |_, dlog_dir| {
                RunFingerprint::Controller(run_controller(
                    &args.kernel,
                    &args.initrd,
                    3,
                    2,
                    CONTROLLER_SEED,
                    CONTROLLER_TICKS,
                    dlog_dir,
                ))
            },
        ),
    ];

    let receipt = DeterminismGateReceipt::new(
        args.kernel.clone(),
        args.initrd.clone(),
        crc32_file(&args.kernel).unwrap_or_else(|err| format!("unavailable:{err}")),
        crc32_file(&args.initrd).unwrap_or_else(|err| format!("unavailable:{err}")),
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
