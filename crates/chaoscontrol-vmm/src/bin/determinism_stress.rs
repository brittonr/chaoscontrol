//! Determinism stress test: run the same seed N times and verify
//! every run produces identical results.
//!
//! Usage:
//!   cargo run --release --bin determinism_stress -- <kernel> <initrd> [N]
//!
//! Tests four configurations:
//!   1. Single VM, 1 vCPU, 10 runs
//!   2. Single VM, 2 vCPUs (SMP), 10 runs
//!   3. 3 VMs via SimulationController, 1 vCPU each, 10 runs
//!   4. 3 VMs via SimulationController, 2 vCPUs each, 10 runs

use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::schedule::FaultScheduleBuilder;
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController};
use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
use std::env;
use std::time::Instant;

/// Fingerprint of a single-VM run.
#[derive(Debug, Clone, PartialEq, Eq)]
struct VmFingerprint {
    exit_count: u64,
    virtual_tsc: u64,
    /// Serial output with non-deterministic lines stripped.
    serial_stripped: String,
}

/// Fingerprint of a multi-VM controller run.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ControllerFingerprint {
    tick: u64,
    vm_exits: Vec<u64>,
    vm_vtscs: Vec<u64>,
    serials_stripped: Vec<String>,
}

/// Strip known non-deterministic lines from serial output.
fn strip_nondeterministic(s: &str) -> String {
    s.lines()
        .filter(|line| {
            let stripped = line.trim();
            // PIT calibration reads host time → varies
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

fn run_single_vm(kernel: &str, initrd: &str, num_vcpus: usize, max_exits: u64) -> VmFingerprint {
    let config = VmConfig {
        num_vcpus,
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
            ..Default::default()
        },
        kernel_path: kernel.to_string(),
        initrd_path: Some(initrd.to_string()),
        seed,
        quantum: 5000,
        schedule,
        disk_image_path: None,
        base_core: None,
    };

    let mut ctrl = SimulationController::new(config).expect("create controller");
    ctrl.force_setup_complete();

    for _ in 0..ticks {
        ctrl.step_round().expect("step");
    }

    // Collect fingerprints from each VM
    let mut vm_exits = Vec::new();
    let mut vm_vtscs = Vec::new();
    let mut serials_stripped = Vec::new();

    for i in 0..num_vms {
        let slot = ctrl.vm_slot(i).unwrap();
        vm_exits.push(slot.vm.exit_count());
        vm_vtscs.push(slot.vm.virtual_tsc());
        // Controller doesn't expose serial easily, so we just track exits/vtsc
        serials_stripped.push(String::new());
    }

    ControllerFingerprint {
        tick: ctrl.tick(),
        vm_exits,
        vm_vtscs,
        serials_stripped,
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <kernel-path> <initrd-path> [N=10]",
            args[0]
        );
        std::process::exit(1);
    }

    let kernel = &args[1];
    let initrd = &args[2];
    let n: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10);

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║       Determinism Stress Test — {} runs per config           ║", n);
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    let mut all_passed = true;

    // ═══════════════════════════════════════════════════════════════
    //  Test 1: Single VM, 1 vCPU, N runs
    // ═══════════════════════════════════════════════════════════════
    {
        println!("━━━ Test 1: Single VM, 1 vCPU, {} runs × 70K exits ━━━", n);
        let start = Instant::now();
        let max_exits = 70_000;

        let reference = run_single_vm(kernel, initrd, 1, max_exits);
        print!("  run  1: exits={:<8} vtsc={:<16} ✅ (reference)\n", reference.exit_count, reference.virtual_tsc);

        let mut mismatches = 0;
        for i in 2..=n {
            let fp = run_single_vm(kernel, initrd, 1, max_exits);
            let ok = fp == reference;
            if ok {
                print!("  run {:>2}: exits={:<8} vtsc={:<16} ✅\n", i, fp.exit_count, fp.virtual_tsc);
            } else {
                print!("  run {:>2}: exits={:<8} vtsc={:<16} ❌ MISMATCH\n", i, fp.exit_count, fp.virtual_tsc);
                if fp.exit_count != reference.exit_count {
                    eprintln!("         exit_count: {} vs reference {}", fp.exit_count, reference.exit_count);
                }
                if fp.virtual_tsc != reference.virtual_tsc {
                    eprintln!("         virtual_tsc: {} vs reference {}", fp.virtual_tsc, reference.virtual_tsc);
                }
                if fp.serial_stripped != reference.serial_stripped {
                    // Find first differing line
                    let ref_lines: Vec<&str> = reference.serial_stripped.lines().collect();
                    let fp_lines: Vec<&str> = fp.serial_stripped.lines().collect();
                    for (j, (a, b)) in ref_lines.iter().zip(fp_lines.iter()).enumerate() {
                        if a != b {
                            eprintln!("         first serial diff at line {}:", j);
                            eprintln!("           ref: {}", a);
                            eprintln!("           got: {}", b);
                            break;
                        }
                    }
                    if ref_lines.len() != fp_lines.len() {
                        eprintln!("         serial line count: {} vs reference {}", fp_lines.len(), ref_lines.len());
                    }
                }
                mismatches += 1;
            }
        }

        let elapsed = start.elapsed();
        if mismatches == 0 {
            println!("  ✅ PASS: {}/{} runs identical ({:.1}s)\n", n, n, elapsed.as_secs_f64());
        } else {
            println!("  ❌ FAIL: {}/{} runs mismatched ({:.1}s)\n", mismatches, n, elapsed.as_secs_f64());
            all_passed = false;
        }
    }

    // ═══════════════════════════════════════════════════════════════
    //  Test 2: Single VM, 2 vCPUs (SMP), N runs
    // ═══════════════════════════════════════════════════════════════
    {
        println!("━━━ Test 2: Single VM, 2 vCPUs (SMP), {} runs × 70K exits ━━━", n);
        let start = Instant::now();
        let max_exits = 70_000;

        let reference = run_single_vm(kernel, initrd, 2, max_exits);
        print!("  run  1: exits={:<8} vtsc={:<16} ✅ (reference)\n", reference.exit_count, reference.virtual_tsc);

        let mut mismatches = 0;
        for i in 2..=n {
            let fp = run_single_vm(kernel, initrd, 2, max_exits);
            let ok = fp == reference;
            if ok {
                print!("  run {:>2}: exits={:<8} vtsc={:<16} ✅\n", i, fp.exit_count, fp.virtual_tsc);
            } else {
                print!("  run {:>2}: exits={:<8} vtsc={:<16} ❌ MISMATCH\n", i, fp.exit_count, fp.virtual_tsc);
                if fp.exit_count != reference.exit_count {
                    eprintln!("         exit_count: {} vs reference {}", fp.exit_count, reference.exit_count);
                }
                if fp.virtual_tsc != reference.virtual_tsc {
                    eprintln!("         virtual_tsc: {} vs reference {}", fp.virtual_tsc, reference.virtual_tsc);
                }
                if fp.serial_stripped != reference.serial_stripped {
                    let ref_lines: Vec<&str> = reference.serial_stripped.lines().collect();
                    let fp_lines: Vec<&str> = fp.serial_stripped.lines().collect();
                    for (j, (a, b)) in ref_lines.iter().zip(fp_lines.iter()).enumerate() {
                        if a != b {
                            eprintln!("         first serial diff at line {}:", j);
                            eprintln!("           ref: {}", a);
                            eprintln!("           got: {}", b);
                            break;
                        }
                    }
                }
                mismatches += 1;
            }
        }

        let elapsed = start.elapsed();
        if mismatches == 0 {
            println!("  ✅ PASS: {}/{} runs identical ({:.1}s)\n", n, n, elapsed.as_secs_f64());
        } else {
            println!("  ❌ FAIL: {}/{} runs mismatched ({:.1}s)\n", mismatches, n, elapsed.as_secs_f64());
            all_passed = false;
        }
    }

    // ═══════════════════════════════════════════════════════════════
    //  Test 3: 3 VMs, 1 vCPU each, N runs via SimulationController
    // ═══════════════════════════════════════════════════════════════
    {
        println!("━━━ Test 3: 3 VMs × 1 vCPU, {} runs × 10 ticks ━━━", n);
        let start = Instant::now();
        let seed = 42u64;

        let reference = run_controller(kernel, initrd, 3, 1, seed, 10);
        print!("  run  1: tick={:<4} exits={:?} ✅ (reference)\n", reference.tick, reference.vm_exits);

        let mut mismatches = 0;
        for i in 2..=n {
            let fp = run_controller(kernel, initrd, 3, 1, seed, 10);
            let ok = fp == reference;
            if ok {
                print!("  run {:>2}: tick={:<4} exits={:?} ✅\n", i, fp.tick, fp.vm_exits);
            } else {
                print!("  run {:>2}: tick={:<4} exits={:?} ❌ MISMATCH\n", i, fp.tick, fp.vm_exits);
                if fp.tick != reference.tick {
                    eprintln!("         tick: {} vs reference {}", fp.tick, reference.tick);
                }
                for (j, (a, b)) in fp.vm_exits.iter().zip(reference.vm_exits.iter()).enumerate() {
                    if a != b {
                        eprintln!("         VM{} exits: {} vs reference {}", j, a, b);
                    }
                }
                for (j, (a, b)) in fp.vm_vtscs.iter().zip(reference.vm_vtscs.iter()).enumerate() {
                    if a != b {
                        eprintln!("         VM{} vtsc: {} vs reference {}", j, a, b);
                    }
                }
                mismatches += 1;
            }
        }

        let elapsed = start.elapsed();
        if mismatches == 0 {
            println!("  ✅ PASS: {}/{} runs identical ({:.1}s)\n", n, n, elapsed.as_secs_f64());
        } else {
            println!("  ❌ FAIL: {}/{} runs mismatched ({:.1}s)\n", mismatches, n, elapsed.as_secs_f64());
            all_passed = false;
        }
    }

    // ═══════════════════════════════════════════════════════════════
    //  Test 4: 3 VMs, 2 vCPUs each (SMP), N runs via Controller
    // ═══════════════════════════════════════════════════════════════
    {
        println!("━━━ Test 4: 3 VMs × 2 vCPUs (SMP), {} runs × 10 ticks ━━━", n);
        let start = Instant::now();
        let seed = 42u64;

        let reference = run_controller(kernel, initrd, 3, 2, seed, 10);
        print!("  run  1: tick={:<4} exits={:?} ✅ (reference)\n", reference.tick, reference.vm_exits);

        let mut mismatches = 0;
        for i in 2..=n {
            let fp = run_controller(kernel, initrd, 3, 2, seed, 10);
            let ok = fp == reference;
            if ok {
                print!("  run {:>2}: tick={:<4} exits={:?} ✅\n", i, fp.tick, fp.vm_exits);
            } else {
                print!("  run {:>2}: tick={:<4} exits={:?} ❌ MISMATCH\n", i, fp.tick, fp.vm_exits);
                if fp.tick != reference.tick {
                    eprintln!("         tick: {} vs reference {}", fp.tick, reference.tick);
                }
                for (j, (a, b)) in fp.vm_exits.iter().zip(reference.vm_exits.iter()).enumerate() {
                    if a != b {
                        eprintln!("         VM{} exits: {} vs reference {}", j, a, b);
                    }
                }
                for (j, (a, b)) in fp.vm_vtscs.iter().zip(reference.vm_vtscs.iter()).enumerate() {
                    if a != b {
                        eprintln!("         VM{} vtsc: {} vs reference {}", j, a, b);
                    }
                }
                mismatches += 1;
            }
        }

        let elapsed = start.elapsed();
        if mismatches == 0 {
            println!("  ✅ PASS: {}/{} runs identical ({:.1}s)\n", n, n, elapsed.as_secs_f64());
        } else {
            println!("  ❌ FAIL: {}/{} runs mismatched ({:.1}s)\n", mismatches, n, elapsed.as_secs_f64());
            all_passed = false;
        }
    }

    // ═══════════════════════════════════════════════════════════════
    //  Summary
    // ═══════════════════════════════════════════════════════════════
    println!("╔══════════════════════════════════════════════════════════════╗");
    if all_passed {
        println!("║  ✅ ALL CONFIGURATIONS DETERMINISTIC ({} runs each)         ║", n);
    } else {
        println!("║  ❌ DETERMINISM FAILURES DETECTED                           ║");
    }
    println!("╚══════════════════════════════════════════════════════════════╝");

    if !all_passed {
        std::process::exit(1);
    }
}
