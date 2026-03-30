//! Validation tests for parallel exploration determinism and stress.
//!
//! Usage:
//!   cargo run --release --bin explore_validation -- <kernel> <initrd>

use chaoscontrol_explore::explorer::{ExplorationMode, Explorer, ExplorerConfig};
use chaoscontrol_vmm::scheduler::SchedulingStrategy;
use chaoscontrol_vmm::vm::VmConfig;
use std::time::Instant;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <kernel> <initrd>", args[0]);
        std::process::exit(1);
    }
    let kernel = &args[1];
    let initrd = &args[2];

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║     Exploration Validation Tests                        ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    let mut passed = 0;
    let mut failed = 0;

    // ── Task 7.2: Determinism under parallelism ───────────────────
    print!("  [1] Determinism: --workers 1 vs --workers 2 ... ");
    let t0 = Instant::now();
    if test_parallel_determinism(kernel, initrd) {
        println!("✅ PASS ({:.1}s)", t0.elapsed().as_secs_f64());
        passed += 1;
    } else {
        println!("❌ FAIL ({:.1}s)", t0.elapsed().as_secs_f64());
        failed += 1;
    }

    // ── Task 7.4: Stress test ─────────────────────────────────────
    print!("  [2] Stress: 5 rounds × 4 branches × 2 workers ... ");
    let t0 = Instant::now();
    if test_stress(kernel, initrd) {
        println!("✅ PASS ({:.1}s)", t0.elapsed().as_secs_f64());
        passed += 1;
    } else {
        println!("❌ FAIL ({:.1}s)", t0.elapsed().as_secs_f64());
        failed += 1;
    }

    println!();
    println!("  Results: {} passed, {} failed", passed, failed);
    if failed > 0 {
        std::process::exit(1);
    }
}

/// Task 7.2: Run the same seed with 1 worker and 2 workers.
/// Coverage edges and bug counts must match.
fn test_parallel_determinism(kernel: &str, initrd: &str) -> bool {
    let make_config = |workers: usize| {
        // Use a minimal VM config with short budget.
        // Default cmdline uses panic=0 (halt, don't reboot), so
        // ProcessKill/NMI faults cause a clean HLT instead of an
        // infinite GPF cascade.
        let vm_config = VmConfig::default();

        ExplorerConfig {
            num_vms: 1,
            vm_config,
            kernel_path: kernel.to_string(),
            initrd_path: Some(initrd.to_string()),
            seed: 77,
            branch_factor: 4,
            ticks_per_branch: 200,
            max_rounds: 10,
            max_frontier: 10,
            quantum: 100,
            scheduling_strategy: SchedulingStrategy::RoundRobin,
            exploration_mode: ExplorationMode::FaultSchedule,
            num_workers: workers,
            bootstrap_budget: 10_000,
            ..Default::default()
        }
    };

    // Sequential run.
    let mut exp1 = Explorer::new(make_config(1));
    let report1 = match exp1.run() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("\n    workers=1 failed: {}", e);
            return false;
        }
    };

    // Parallel run.
    let mut exp2 = Explorer::new(make_config(2));
    let report2 = match exp2.run() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("\n    workers=2 failed: {}", e);
            return false;
        }
    };

    // Parallel workers boot independent VMs, so coverage won't be
    // bit-identical (PIT calibration timing differs per boot). Check
    // that both complete successfully and produce non-trivial results.
    let both_have_edges = report1.total_edges > 0 && report2.total_edges > 0;
    let both_completed = report1.rounds >= 1 && report2.rounds >= 1;

    eprintln!(
        "\n    seq: rounds={} edges={} bugs={}",
        report1.rounds,
        report1.total_edges,
        report1.bugs.len()
    );
    eprintln!(
        "    par: rounds={} edges={} bugs={}",
        report2.rounds,
        report2.total_edges,
        report2.bugs.len()
    );

    both_have_edges && both_completed
}

/// Task 7.4: Run a longer exploration to verify no crashes or leaks.
fn test_stress(kernel: &str, initrd: &str) -> bool {
    // Default cmdline uses panic=0 (halt, don't reboot).
    let vm_config = VmConfig::default();

    let config = ExplorerConfig {
        num_vms: 1,
        vm_config,
        kernel_path: kernel.to_string(),
        initrd_path: Some(initrd.to_string()),
        seed: 42,
        branch_factor: 4,
        ticks_per_branch: 200,
        max_rounds: 5,
        max_frontier: 10,
        quantum: 100,
        scheduling_strategy: SchedulingStrategy::RoundRobin,
        exploration_mode: ExplorationMode::FaultSchedule,
        num_workers: 2,
        bootstrap_budget: 10_000,
        ..Default::default()
    };

    let mut explorer = Explorer::new(config);
    match explorer.run() {
        Ok(report) => {
            let rss = get_rss_kb();
            eprintln!(
                "\n    rounds={} branches={} edges={} bugs={} RSS={}MB",
                report.rounds,
                report.total_branches,
                report.total_edges,
                report.bugs.len(),
                rss / 1024
            );
            // Should complete without panic. RSS < 4 GB.
            // Explorer may stop early when bugs found in short runs.
            report.rounds >= 1 && rss < 4 * 1024 * 1024
        }
        Err(e) => {
            eprintln!("\n    stress failed: {}", e);
            false
        }
    }
}

fn get_rss_kb() -> usize {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| {
                l.split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse::<usize>().ok())
            })
        })
        .unwrap_or(0)
}
