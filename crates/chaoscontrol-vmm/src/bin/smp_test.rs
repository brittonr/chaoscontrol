//! SMP determinism test: verify identical exit counts across runs.

use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
use std::env;

fn strip_nondeterministic(s: &str) -> String {
    let re_ts = regex::Regex::new(r"\[\s*\d+\.\d+\]\s*").unwrap();
    let re_mem = regex::Regex::new(r"Memory: \d+K/\d+K available").unwrap();
    let re_tsc = regex::Regex::new(r"tsc: Detected [\d.]+ MHz processor").unwrap();
    let re_tsc2 = regex::Regex::new(r"tsc: Fast TSC calibration.*").unwrap();
    let s = re_ts.replace_all(s, "");
    let s = re_mem.replace_all(&s, "Memory: XXXK/YYYK available");
    let s = re_tsc.replace_all(&s, "tsc: Detected X MHz processor");
    let s = re_tsc2.replace_all(&s, "tsc: Fast TSC calibration (stripped)");
    s.to_string()
}

fn run_boot(kernel: &str, initrd: &str, num_vcpus: usize) -> (u64, u64, String) {
    let mut config = VmConfig::default();
    config.num_vcpus = num_vcpus;
    let mut vm = DeterministicVm::new(config).expect("create VM");
    vm.load_kernel(kernel, Some(initrd)).expect("load kernel");

    let mut output = String::new();
    for _ in 0..30 {
        let (_, halted) = vm.run_bounded(10_000).expect("run");
        output.push_str(&vm.take_serial_output());
        if output.contains("Brought up") || output.contains("login:") || halted {
            break;
        }
    }
    (vm.exit_count(), vm.virtual_tsc(), output)
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <kernel> <initrd>", args[0]);
        std::process::exit(1);
    }

    // Single-vCPU baseline
    println!("=== Single-vCPU (3 runs) ===");
    let mut s_ref = None;
    let mut s_ok = true;
    for i in 0..3 {
        let (e, t, _) = run_boot(&args[1], &args[2], 1);
        println!("  Run {}: exits={} vtsc={}", i + 1, e, t);
        if let Some((re, rt)) = s_ref {
            if e != re || t != rt {
                s_ok = false;
            }
        } else {
            s_ref = Some((e, t));
        }
    }
    println!(
        "  {}",
        if s_ok {
            "✅ DETERMINISTIC"
        } else {
            "❌ DIFFERS"
        }
    );

    // SMP
    println!("\n=== SMP 2-vCPU (5 runs) ===");
    let mut results = Vec::new();
    for i in 0..5 {
        let (e, t, o) = run_boot(&args[1], &args[2], 2);
        let ok = o.contains("Brought up 1 node, 2 CPUs");
        println!("  Run {}: exits={} vtsc={} 2cpus={}", i + 1, e, t, ok);
        results.push((e, t, o));
    }

    let exits_match = results.iter().all(|r| r.0 == results[0].0);
    let stripped: Vec<String> = results
        .iter()
        .map(|r| strip_nondeterministic(&r.2))
        .collect();
    let serial_match = stripped.iter().all(|s| *s == stripped[0]);

    println!(
        "\n  Exits:    {}",
        if exits_match {
            "✅ identical"
        } else {
            "❌ differs"
        }
    );
    println!(
        "  Serial:   {}",
        if serial_match {
            "✅ identical"
        } else {
            "❌ differs"
        }
    );

    let all_ok = exits_match
        && serial_match
        && results
            .iter()
            .all(|r| r.2.contains("Brought up 1 node, 2 CPUs"));

    if all_ok {
        println!("  ✅ FULLY DETERMINISTIC SMP");
    } else {
        println!("  ❌ NON-DETERMINISTIC");
        if !exits_match {
            for (i, r) in results.iter().enumerate() {
                if r.0 != results[0].0 {
                    println!("  Run {} exits={} (ref={})", i + 1, r.0, results[0].0);
                }
            }
        }
        if !serial_match {
            for (i, s) in stripped.iter().enumerate().skip(1) {
                if *s != stripped[0] {
                    let l1: Vec<&str> = stripped[0].lines().collect();
                    let l2: Vec<&str> = s.lines().collect();
                    for (j, (a, b)) in l1.iter().zip(l2.iter()).enumerate() {
                        if a != b {
                            println!("  First diff at line {}: ", j);
                            println!("    ref:  '{}'", a.trim());
                            println!("    run{}: '{}'", i + 1, b.trim());
                            break;
                        }
                    }
                    break;
                }
            }
        }
        std::process::exit(1);
    }
}
