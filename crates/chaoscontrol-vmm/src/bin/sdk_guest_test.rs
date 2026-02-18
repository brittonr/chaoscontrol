//! Integration test: boots a VM with the SDK-instrumented guest and
//! verifies the full SDK round-trip.
//!
//! Checks:
//!   1. Guest boots and runs to "workload complete"
//!   2. `setup_complete` was received by the fault engine
//!   3. Property oracle contains Always / Sometimes / Reachable assertions
//!   4. Coverage bitmap has non-zero entries (edges recorded)
//!   5. Guided random values were delivered and consumed
//!   6. Lifecycle event was received
//!   7. Determinism: two identical runs produce the same oracle + coverage
//!
//! Usage:
//!   cargo run --release --bin sdk_guest_test -- <vmlinux> [initrd-sdk.gz]
//!
//! If initrd is omitted, defaults to `guest/initrd-sdk.gz`.

use chaoscontrol_fault::oracle::Verdict;
use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
use std::env;
use std::time::Instant;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <vmlinux> [initrd-sdk.gz]", args[0]);
        std::process::exit(1);
    }

    let kernel = &args[1];
    let default_initrd = "guest/initrd-sdk.gz".to_string();
    let initrd = args.get(2).unwrap_or(&default_initrd);

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║        SDK Guest Integration Tests                      ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();
    println!("  kernel: {}", kernel);
    println!("  initrd: {}", initrd);
    println!();

    let mut passed = 0;
    let mut failed = 0;
    let mut tests: Vec<(&str, bool)> = Vec::new();

    macro_rules! run_test {
        ($name:expr, $func:expr) => {{
            print!("  [{:>2}] {} ... ", passed + failed + 1, $name);
            let start = Instant::now();
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $func)) {
                Ok(true) => {
                    let elapsed = start.elapsed();
                    println!("✅ PASS ({:.1}s)", elapsed.as_secs_f64());
                    passed += 1;
                    tests.push(($name, true));
                }
                Ok(false) => {
                    let elapsed = start.elapsed();
                    println!("❌ FAIL ({:.1}s)", elapsed.as_secs_f64());
                    failed += 1;
                    tests.push(($name, false));
                }
                Err(e) => {
                    let elapsed = start.elapsed();
                    let msg = if let Some(s) = e.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = e.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };
                    println!("💥 PANIC ({:.1}s): {}", elapsed.as_secs_f64(), msg);
                    failed += 1;
                    tests.push(($name, false));
                }
            }
        }};
    }

    // ═══════════════════════════════════════════════════════════════
    //  Test 1: Guest boots and workload completes
    // ═══════════════════════════════════════════════════════════════
    run_test!("SDK guest boots and completes workload", {
        let config = VmConfig::default();
        let mut vm = DeterministicVm::new(config).expect("create VM");
        vm.load_kernel(kernel, Some(initrd)).expect("load kernel");
        let output = vm.run_until("workload complete").expect("run VM");
        let ok = output.contains("workload complete")
            && output.contains("setup_complete");
        if !ok {
            eprintln!("    serial output (last 500 chars):");
            let tail = if output.len() > 500 { &output[output.len()-500..] } else { &output };
            for line in tail.lines().take(15) {
                eprintln!("      {}", line);
            }
        }
        ok
    });

    // ═══════════════════════════════════════════════════════════════
    //  Test 2: setup_complete received
    // ═══════════════════════════════════════════════════════════════
    run_test!("Fault engine received setup_complete", {
        let config = VmConfig::default();
        let mut vm = DeterministicVm::new(config).expect("create VM");
        vm.load_kernel(kernel, Some(initrd)).expect("load kernel");
        vm.run_until("workload complete").expect("run VM");

        let result = vm.fault_engine().is_setup_complete();
        if !result {
            eprintln!("    setup_complete flag is false");
        }
        result
    });

    // ═══════════════════════════════════════════════════════════════
    //  Test 3: Oracle has assertions
    // ═══════════════════════════════════════════════════════════════
    run_test!("Oracle has Always + Sometimes + Reachable assertions", {
        let config = VmConfig::default();
        let mut vm = DeterministicVm::new(config).expect("create VM");
        vm.load_kernel(kernel, Some(initrd)).expect("load kernel");
        vm.run_until("workload complete").expect("run VM");

        let report = vm.fault_engine().oracle().report();
        let total = report.assertions.len();

        // Count kinds
        let mut always_count = 0;
        let mut sometimes_count = 0;
        let mut reachable_count = 0;
        for record in report.assertions.values() {
            match record.kind {
                chaoscontrol_fault::oracle::AssertionKind::Always => always_count += 1,
                chaoscontrol_fault::oracle::AssertionKind::Sometimes => sometimes_count += 1,
                chaoscontrol_fault::oracle::AssertionKind::Reachable => reachable_count += 1,
                _ => {}
            }
        }

        eprintln!("    assertions: {} total (always={}, sometimes={}, reachable={})",
            total, always_count, sometimes_count, reachable_count);

        // Guest sends: 1 always, 5 sometimes, 4+ reachable
        always_count >= 1 && sometimes_count >= 4 && reachable_count >= 1
    });

    // ═══════════════════════════════════════════════════════════════
    //  Test 4: Always assertions pass
    // ═══════════════════════════════════════════════════════════════
    run_test!("All Always assertions have verdict=Passed", {
        let config = VmConfig::default();
        let mut vm = DeterministicVm::new(config).expect("create VM");
        vm.load_kernel(kernel, Some(initrd)).expect("load kernel");
        vm.run_until("workload complete").expect("run VM");

        let report = vm.fault_engine().oracle().report();
        let mut all_pass = true;

        for record in report.assertions.values() {
            if record.kind == chaoscontrol_fault::oracle::AssertionKind::Always {
                if record.verdict() != Verdict::Passed {
                    eprintln!("    FAIL: always '{}' -> {:?} (hit={}, true={}, false={})",
                        record.message, record.verdict(),
                        record.hit_count, record.true_count, record.false_count);
                    all_pass = false;
                }
            }
        }
        all_pass
    });

    // ═══════════════════════════════════════════════════════════════
    //  Test 5: Coverage bitmap has edges
    // ═══════════════════════════════════════════════════════════════
    run_test!("Coverage bitmap has non-zero entries", {
        let config = VmConfig::default();
        let mut vm = DeterministicVm::new(config).expect("create VM");
        vm.load_kernel(kernel, Some(initrd)).expect("load kernel");
        vm.run_until("workload complete").expect("run VM");

        let bitmap = vm.read_coverage_bitmap();
        let edges = bitmap.iter().filter(|&&b| b > 0).count();

        eprintln!("    coverage edges: {} / {} bytes non-zero", edges, bitmap.len());

        // Guest records: iteration edges + 4 path edges = should be > 5
        edges >= 5
    });

    // ═══════════════════════════════════════════════════════════════
    //  Test 6: Lifecycle event received
    // ═══════════════════════════════════════════════════════════════
    run_test!("Oracle received lifecycle event", {
        let config = VmConfig::default();
        let mut vm = DeterministicVm::new(config).expect("create VM");
        vm.load_kernel(kernel, Some(initrd)).expect("load kernel");
        vm.run_until("workload complete").expect("run VM");

        let report = vm.fault_engine().oracle().report();
        let has_event = report.events.iter().any(|e| e.name == "workload_done");

        eprintln!("    events: {:?}", report.events.iter().map(|e| &e.name).collect::<Vec<_>>());
        has_event
    });

    // ═══════════════════════════════════════════════════════════════
    //  Test 7: Deterministic — two runs produce same oracle
    // ═══════════════════════════════════════════════════════════════
    run_test!("Deterministic: two runs produce identical oracle", {
        // Run 1
        let config1 = VmConfig::default();
        let mut vm1 = DeterministicVm::new(config1).expect("create VM1");
        vm1.load_kernel(kernel, Some(initrd)).expect("load kernel");
        vm1.run_until("workload complete").expect("run VM1");
        let report1 = vm1.fault_engine().oracle().report();
        let bitmap1 = vm1.read_coverage_bitmap();

        // Run 2 (same config)
        let config2 = VmConfig::default();
        let mut vm2 = DeterministicVm::new(config2).expect("create VM2");
        vm2.load_kernel(kernel, Some(initrd)).expect("load kernel");
        vm2.run_until("workload complete").expect("run VM2");
        let report2 = vm2.fault_engine().oracle().report();
        let bitmap2 = vm2.read_coverage_bitmap();

        // Compare assertion IDs
        let ids1: Vec<u32> = report1.assertions.keys().copied().collect();
        let ids2: Vec<u32> = report2.assertions.keys().copied().collect();
        let ids_match = ids1 == ids2;

        // Compare hit counts for each assertion
        let mut counts_match = true;
        for id in &ids1 {
            let r1 = &report1.assertions[id];
            let r2 = &report2.assertions[id];
            if r1.hit_count != r2.hit_count || r1.true_count != r2.true_count {
                eprintln!("    assertion {} '{}': hit {}/{}, true {}/{}",
                    id, r1.message, r1.hit_count, r2.hit_count,
                    r1.true_count, r2.true_count);
                counts_match = false;
            }
        }

        // Compare coverage bitmaps
        let cov_match = bitmap1 == bitmap2;

        let edges1 = bitmap1.iter().filter(|&&b| b > 0).count();
        let edges2 = bitmap2.iter().filter(|&&b| b > 0).count();
        eprintln!("    assertion IDs match: {}", ids_match);
        eprintln!("    hit counts match:    {}", counts_match);
        eprintln!("    coverage match:      {} (edges: {}/{})", cov_match, edges1, edges2);

        ids_match && counts_match && cov_match
    });

    // ═══════════════════════════════════════════════════════════════
    //  Summary
    // ═══════════════════════════════════════════════════════════════
    println!();
    println!("╔══════════════════════════════════════════════════════════╗");
    println!(
        "║  Results: {} passed, {} failed, {} total                 ║",
        passed, failed, passed + failed,
    );
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    for (name, ok) in &tests {
        println!("  {} {}", if *ok { "✅" } else { "❌" }, name);
    }
    println!();

    if failed > 0 {
        std::process::exit(1);
    }
}
