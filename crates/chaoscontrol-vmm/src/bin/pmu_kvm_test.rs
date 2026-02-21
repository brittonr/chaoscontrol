//! Diagnostic: measure PMU instruction counting determinism in KVM.
//!
//! Boots a VM, takes a snapshot, then runs multiple times from the same
//! snapshot measuring per-exit guest instruction counts via `perf_event_open`
//! with `exclude_host=1`. Reports per-exit jitter statistics.
//!
//! On AMD Zen5: ~41 instruction skid per exit (SVM boundary imprecision).
//! On Intel: expected to be zero (VMX has precise PMU boundaries).
//!
//! Usage: pmu_kvm_test <kernel> <initrd> [boot_exits] [measure_exits] [runs]

use chaoscontrol_vmm::perf::InstructionCounter;
use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: pmu_kvm_test <kernel> <initrd> [boot_exits] [measure_exits] [runs]");
        std::process::exit(1);
    }
    let kernel = &args[1];
    let initrd = &args[2];
    let boot_exits: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(5000);
    let measure_exits: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(500);
    let runs: usize = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(5);

    println!(
        "=== PMU KVM Determinism Diagnostic ===\n\
         Kernel: {}\nInitrd: {}\n\
         Boot: {} exits, Measure: {} exits, Runs: {}\n",
        kernel, initrd, boot_exits, measure_exits, runs
    );

    // Boot and snapshot once
    let config = VmConfig::default();
    let mut vm = DeterministicVm::new(config).expect("create VM");
    vm.load_kernel(kernel, Some(initrd)).expect("load kernel");
    vm.run_bounded(boot_exits).expect("boot");
    let _ = vm.take_serial_output();
    let snap = vm.snapshot().expect("snapshot");

    // Collect per-exit cumulative instruction counts from each run
    let mut all_per_exit: Vec<Vec<u64>> = Vec::new();

    for i in 0..runs {
        vm.restore(&snap).expect("restore");
        let counter = InstructionCounter::new().expect("PMU not available");
        counter.reset_and_enable();

        let mut cumulative: Vec<u64> = Vec::new();
        for _ in 0..measure_exits {
            let (exits, halted) = vm.run_bounded(1).expect("step");
            cumulative.push(counter.read());
            if exits == 0 || halted {
                break;
            }
        }
        counter.disable();

        // Convert cumulative → per-exit deltas
        let mut per_exit = Vec::with_capacity(cumulative.len());
        for j in 0..cumulative.len() {
            per_exit.push(if j == 0 {
                cumulative[0]
            } else {
                cumulative[j] - cumulative[j - 1]
            });
        }

        let total: u64 = per_exit.iter().sum();
        println!("Run {}: {} exits, {} total insns", i + 1, per_exit.len(), total);
        all_per_exit.push(per_exit);
    }

    let len = all_per_exit.iter().map(|p| p.len()).min().unwrap();

    // Compute delta distribution (run N vs run 1)
    let mut abs_deltas: Vec<i64> = Vec::new();
    let mut delta_hist: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
    let mut exact_matches = 0usize;

    for idx in 0..len {
        let base = all_per_exit[0][idx];
        for run in &all_per_exit[1..] {
            let delta = run[idx] as i64 - base as i64;
            abs_deltas.push(delta.abs());
            *delta_hist.entry(delta).or_insert(0) += 1;
            if delta == 0 {
                exact_matches += 1;
            }
        }
    }

    abs_deltas.sort();
    let total_comparisons = len * (runs - 1);
    let p50 = abs_deltas[total_comparisons / 2];
    let p90 = abs_deltas[(total_comparisons as f64 * 0.9) as usize];
    let p99 = abs_deltas[(total_comparisons as f64 * 0.99) as usize];
    let max = *abs_deltas.last().unwrap();

    println!("\n=== Results ({} comparisons) ===\n", total_comparisons);
    println!(
        "Exact matches: {}/{} ({:.1}%)",
        exact_matches,
        total_comparisons,
        exact_matches as f64 / total_comparisons as f64 * 100.0
    );
    println!("|Δ| percentiles: p50={}, p90={}, p99={}, max={}", p50, p90, p99, max);

    println!("\nTop deltas:");
    let mut sorted_hist: Vec<(i64, usize)> = delta_hist.into_iter().collect();
    sorted_hist.sort_by(|a, b| b.1.cmp(&a.1));
    for (delta, count) in sorted_hist.iter().take(8) {
        println!(
            "  Δ={:>+6}: {} ({:.1}%)",
            delta,
            count,
            *count as f64 / total_comparisons as f64 * 100.0
        );
    }

    // First 10 exits detail
    println!("\nFirst 10 exits:");
    print!("{:>6}", "exit");
    for i in 0..runs {
        print!("  {:>10}", format!("run{}", i + 1));
    }
    println!();
    for idx in 0..10.min(len) {
        print!("{:>6}", idx);
        for run in &all_per_exit {
            print!("  {:>10}", run[idx]);
        }
        println!();
    }

    println!("\n=== Verdict ===");
    if exact_matches == total_comparisons {
        println!("✅ DETERMINISTIC — instructions-retired time model viable");
    } else if p50 == 0 && p90 <= 5 {
        println!("⚠️  Near-deterministic (p50=0, p90={}) — may work with single-step cleanup", p90);
    } else {
        println!(
            "❌ NON-DETERMINISTIC (p50={}, max={}) — SVM boundary skid, use exit-count model",
            p50, max
        );
    }
}
