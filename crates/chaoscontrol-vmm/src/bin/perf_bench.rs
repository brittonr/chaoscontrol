//! Performance benchmarks for incremental snapshots.
//!
//! Usage:
//!   cargo run --release --bin perf_bench -- <kernel> <initrd>

use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController};
use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
use std::sync::Arc;
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
    println!("║     Incremental Snapshot Performance Benchmark          ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // ── Task 7.1: Benchmark full vs incremental snapshots ─────────
    bench_single_vm(kernel, initrd);
    bench_controller(kernel, initrd);

    // ── Task 7.3: Memory usage ────────────────────────────────────
    measure_memory(kernel, initrd);
}

fn bench_single_vm(kernel: &str, initrd: &str) {
    println!("── Single VM: full vs incremental snapshot ──");
    println!();

    let config = VmConfig::default();
    let mut vm = DeterministicVm::new(config).expect("create VM");
    vm.load_kernel(kernel, Some(initrd)).expect("load kernel");

    // Boot to a stable state.
    vm.run_bounded(50_000).expect("boot");

    // Take a full snapshot (this is our base).
    let t0 = Instant::now();
    let base_snap = vm.snapshot().expect("base snapshot");
    let full_snap_time = t0.elapsed();
    let base_memory = Arc::new(base_snap.memory.materialize());
    println!(
        "  Base snapshot:       {:>8.2} ms  ({} MB)",
        full_snap_time.as_secs_f64() * 1000.0,
        base_memory.len() / (1024 * 1024)
    );

    // Drain dirty bits accumulated during boot.
    let _ = vm.get_dirty_bitmap().unwrap();

    // Benchmark: run N exits then snapshot (full vs incremental), repeat.
    let tick_counts = [100, 500, 1_000, 5_000, 10_000];

    println!();
    println!(
        "  {:>8} {:>12} {:>12} {:>10} {:>8}",
        "Exits", "Full (ms)", "Incr (ms)", "Speedup", "Dirty"
    );
    println!("  {}", "-".repeat(58));

    for &ticks in &tick_counts {
        // Full snapshot path.
        vm.restore(&base_snap).expect("restore");
        let _ = vm.get_dirty_bitmap(); // drain
        vm.run_bounded(ticks).expect("run");

        let t0 = Instant::now();
        let _full = vm.snapshot().expect("full snap");
        let full_time = t0.elapsed();

        // Incremental snapshot path.
        vm.restore(&base_snap).expect("restore");
        let _ = vm.get_dirty_bitmap(); // drain
        vm.run_bounded(ticks).expect("run");

        let t0 = Instant::now();
        let (_, dirty) = vm.snapshot_incremental(&base_memory).expect("incr snap");
        let incr_time = t0.elapsed();

        let full_ms = full_time.as_secs_f64() * 1000.0;
        let incr_ms = incr_time.as_secs_f64() * 1000.0;
        let speedup = if incr_ms > 0.001 {
            full_ms / incr_ms
        } else {
            f64::INFINITY
        };

        println!(
            "  {:>8} {:>12.2} {:>12.2} {:>9.1}x {:>7}",
            ticks, full_ms, incr_ms, speedup, dirty
        );
    }
    println!();
}

fn bench_controller(kernel: &str, initrd: &str) {
    println!("── Controller (2 VMs): per-branch breakdown ──");
    println!();

    let sim_config = SimulationConfig {
        num_vms: 2,
        vm_config: VmConfig::default(),
        kernel_path: kernel.to_string(),
        initrd_path: Some(initrd.to_string()),
        seed: 42,
        quantum: 100,
        ..Default::default()
    };
    let mut ctrl = SimulationController::new(sim_config).expect("create controller");
    ctrl.force_setup_complete();

    // Boot to stable state.
    ctrl.run(500).expect("boot run");

    // Take base snapshot.
    let base_snap = ctrl.snapshot_all().expect("base snap");
    let bases = SimulationController::extract_memory_bases(&base_snap);
    ctrl.set_memory_bases(bases);

    let tick_counts = [100, 500, 1_000];

    for &ticks in &tick_counts {
        println!("  --- {} ticks ---", ticks);

        // Measure full restore alone.
        let t0 = Instant::now();
        ctrl.restore_all(&base_snap).expect("restore");
        ctrl.reset_vm_statuses();
        let full_restore_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // Drain dirty bits.
        for i in 0..ctrl.num_vms() {
            let _ = ctrl.vm_mut(i).get_dirty_bitmap();
        }

        // Measure run alone.
        let t0 = Instant::now();
        ctrl.run(ticks).expect("run");
        let run_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // Measure full snapshot alone.
        let t0 = Instant::now();
        let _ = ctrl.snapshot_all().expect("full snap");
        let full_snap_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // Now take a branch snapshot for incremental restore benchmark.
        ctrl.restore_all(&base_snap).expect("restore");
        ctrl.reset_vm_statuses();
        for i in 0..ctrl.num_vms() {
            let _ = ctrl.vm_mut(i).get_dirty_bitmap();
        }
        ctrl.run(ticks).expect("run");
        let (branch_snap, dirty) = ctrl.snapshot_all_incremental().expect("incr snap");

        // Seed: full restore to put base in guest memory.
        ctrl.restore_all(&base_snap).expect("seed");
        ctrl.reset_vm_statuses();

        // Measure incremental restore alone.
        let t0 = Instant::now();
        ctrl.restore_all_incremental(&branch_snap)
            .expect("incr restore");
        ctrl.reset_vm_statuses();
        let incr_restore_ms = t0.elapsed().as_secs_f64() * 1000.0;

        for i in 0..ctrl.num_vms() {
            let _ = ctrl.vm_mut(i).get_dirty_bitmap();
        }

        // Run again (same ticks, for incr snap measurement).
        ctrl.run(ticks).expect("run");

        // Measure incremental snapshot alone.
        let t0 = Instant::now();
        let _ = ctrl.snapshot_all_incremental();
        let incr_snap_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let full_total = full_restore_ms + run_ms + full_snap_ms;
        let incr_total = incr_restore_ms + run_ms + incr_snap_ms;

        println!(
            "    Restore:   full {:>8.2} ms   incr {:>8.2} ms   ({:.0}x)",
            full_restore_ms,
            incr_restore_ms,
            if incr_restore_ms > 0.001 {
                full_restore_ms / incr_restore_ms
            } else {
                0.0
            }
        );
        println!("    Run:                 {:>8.2} ms", run_ms);
        println!(
            "    Snapshot:  full {:>8.2} ms   incr {:>8.2} ms   ({:.0}x)",
            full_snap_ms,
            incr_snap_ms,
            if incr_snap_ms > 0.001 {
                full_snap_ms / incr_snap_ms
            } else {
                0.0
            }
        );
        println!(
            "    Total:     full {:>8.2} ms   incr {:>8.2} ms   ({:.1}x)  [dirty={}]",
            full_total,
            incr_total,
            if incr_total > 0.001 {
                full_total / incr_total
            } else {
                0.0
            },
            dirty
        );
        println!();
    }
}

fn measure_memory(kernel: &str, initrd: &str) {
    println!("── Memory usage: overlay snapshot frontier ──");
    println!();

    let config = VmConfig::default();
    let mut vm = DeterministicVm::new(config).expect("create VM");
    vm.load_kernel(kernel, Some(initrd)).expect("load kernel");
    vm.run_bounded(50_000).expect("boot");

    let base_snap = vm.snapshot().expect("base snapshot");
    let base_memory = Arc::new(base_snap.memory.materialize());
    let _ = vm.get_dirty_bitmap();

    let rss_before = get_rss_kb();

    // Simulate storing 50 frontier entries with overlay snapshots.
    let mut overlays = Vec::new();
    for i in 0..50 {
        vm.restore(&base_snap).expect("restore");
        let _ = vm.get_dirty_bitmap();
        vm.run_bounded(1_000).expect("run");
        let (snap, dirty) = vm.snapshot_incremental(&base_memory).expect("incr snap");
        if i == 0 {
            println!(
                "  Base memory:         {} MB",
                base_memory.len() / (1024 * 1024)
            );
            println!("  Dirty pages/overlay: {}", dirty);
            println!("  Overlay size:        {} KB", dirty * 4);
        }
        overlays.push(snap);
    }

    let rss_after = get_rss_kb();
    let rss_delta = rss_after.saturating_sub(rss_before);

    println!("  Frontier entries:    50");
    println!("  RSS before:          {} MB", rss_before / 1024);
    println!("  RSS after:           {} MB", rss_after / 1024);
    println!("  RSS delta:           {} MB", rss_delta / 1024);

    // Expected: ~256 MB base + 50 × dirty × 4 KB ≈ 258 MB
    // Without overlay: 50 × 256 MB = 12.5 GB
    let expected_full_mb = 50 * base_memory.len() / (1024 * 1024);
    println!(
        "  Expected (full):     {} MB  (50 × {} MB)",
        expected_full_mb,
        base_memory.len() / (1024 * 1024)
    );
    println!();

    // Verify reasonable: delta should be well under 4 GB.
    let ok = rss_delta < 4 * 1024 * 1024; // < 4 GB in KB
    println!("  {}  RSS delta < 4 GB", if ok { "✅" } else { "❌" });

    // Keep overlays alive until after measurement.
    drop(overlays);
    println!();
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
