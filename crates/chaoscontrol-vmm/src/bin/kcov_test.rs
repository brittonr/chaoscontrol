//! Quick test to verify KCOV kernel coverage collection works.
//!
//! Usage:
//!   cargo run --release --bin kcov_test -- <kcov-vmlinux> <initrd>
//!
//! Example:
//!   cargo run --release --bin kcov_test -- result-kcov/vmlinux guest/initrd-sdk.gz

use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <kernel> <initrd>", args[0]);
        std::process::exit(1);
    }

    let kernel = &args[1];
    let initrd = &args[2];

    println!("╔══════════════════════════════════════════════╗");
    println!("║       KCOV Kernel Coverage Test              ║");
    println!("╚══════════════════════════════════════════════╝");
    println!();

    // ── Boot with KCOV kernel ────────────────────────────────
    println!("Booting with: {kernel}");
    let config = VmConfig::default();
    let mut vm = DeterministicVm::new(config).expect("create VM");
    vm.load_kernel(kernel, Some(initrd)).expect("load kernel");

    // ── Run to setup_complete (after KCOV init line) ───────────
    println!("Running to setup_complete...");
    let output = vm
        .run_until("setup_complete")
        .expect("run to setup_complete");

    // Check KCOV status — the full line includes "(kcov=active)" or "(kcov=unavailable)"
    let kcov_active = output.contains("kcov=active");
    let kcov_unavailable = output.contains("kcov=unavailable");

    if kcov_active {
        println!("✅ KCOV is ACTIVE — kernel has CONFIG_KCOV=y");
    } else if kcov_unavailable {
        println!("⚠️  KCOV unavailable — kernel lacks CONFIG_KCOV");
        println!("   Build KCOV kernel: nix build .#kcov-vmlinux -o result-kcov");
        return;
    } else {
        println!("❓ No KCOV status detected in guest output");
        println!("   Guest output lines:");
        for line in output.lines().filter(|l| l.contains("chaoscontrol-guest")) {
            println!("     {line}");
        }
        return;
    }

    // ── Run workload to collect kernel coverage ──────────────
    println!("Running workload to collect kernel coverage...");
    let _ = vm.run_until("done, idling");
    let serial = vm.take_serial_output();

    // ── Report results ───────────────────────────────────────
    println!();
    println!("Guest output:");
    for line in serial.lines() {
        if line.contains("chaoscontrol-guest")
            || line.contains("heartbeat")
            || line.contains("kcov")
        {
            println!("  {line}");
        }
    }

    // ── Coverage bitmap stats ────────────────────────────────
    let bitmap = vm.read_coverage_bitmap();
    let edges = bitmap.iter().filter(|&&b| b > 0).count();
    let total_hits: u64 = bitmap.iter().map(|&b| b as u64).sum();
    let max_hit = bitmap.iter().max().unwrap_or(&0);

    println!();
    println!("Coverage bitmap:");
    println!("  Unique edges hit:  {edges}");
    println!("  Total hit count:   {total_hits}");
    println!("  Max single edge:   {max_hit}");
    println!();

    if edges > 100 {
        println!("✅ Kernel coverage working — {edges} edges (userspace + kernel)");
    } else {
        println!("⚠️  Only {edges} edges — KCOV may not be collecting");
    }
}
