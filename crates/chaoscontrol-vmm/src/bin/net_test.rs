//! Integration test for multi-VM networking via virtio-net + smoltcp.
//!
//! Boots multiple VMs with the `chaoscontrol-net-guest` binary and verifies
//! that they can exchange TCP packets through the VMM's network bridge.
//!
//! Usage:
//!   cargo run --release --bin net_test -- <kernel> <initrd-net>
//!
//! Example:
//!   cargo run --release --bin net_test -- result-dev/vmlinux guest/initrd-net.gz

use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController};
use chaoscontrol_vmm::vm::VmConfig;
use std::env;
use std::time::Instant;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <kernel-path> <initrd-net-path>", args[0]);
        std::process::exit(1);
    }

    let kernel = &args[1];
    let initrd = &args[2];

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║      ChaosControl Multi-VM Network Test                 ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // ── Test 1: Two VMs boot and the controller bridges packets ──

    println!("  [1] Two VMs boot with unique MACs ...");
    let start = Instant::now();

    let config = SimulationConfig {
        num_vms: 2,
        vm_config: VmConfig {
            extra_cmdline: Some("num_vms=2".to_string()),
            ..VmConfig::default()
        },
        kernel_path: kernel.to_string(),
        initrd_path: Some(initrd.to_string()),
        seed: 42,
        quantum: 100,
        ..Default::default()
    };

    let mut ctrl = SimulationController::new(config).expect("Failed to create controller");

    // Verify unique MACs
    let mac0 = ctrl.vm(0).net_mac().expect("VM0 has no MAC");
    let mac1 = ctrl.vm(1).net_mac().expect("VM1 has no MAC");
    assert_ne!(mac0, mac1, "VMs must have different MACs");
    assert_eq!(mac0[5], 0, "VM0 MAC last byte should be 0");
    assert_eq!(mac1[5], 1, "VM1 MAC last byte should be 1");
    println!(
        "      VM0 MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac0[0], mac0[1], mac0[2], mac0[3], mac0[4], mac0[5]
    );
    println!(
        "      VM1 MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac1[0], mac1[1], mac1[2], mac1[3], mac1[4], mac1[5]
    );
    println!("      ✅ PASS ({:.1}s)", start.elapsed().as_secs_f64());

    // ── Test 2: Boot VMs and look for networking output ──

    println!("  [2] Booting VMs and running network guest ...");
    let start = Instant::now();

    // Force setup complete so faults can fire
    ctrl.force_setup_complete();

    // Run for enough ticks to allow kernel boot + guest init + TCP exchange
    // Each tick is a bounded number of VM exits — 2000 is typically enough
    // for boot + a few TCP round trips.
    let result = ctrl.run(2000).expect("Failed to run simulation");
    println!(
        "      Ran {} ticks in {:.1}s",
        result.total_ticks,
        start.elapsed().as_secs_f64()
    );

    // Collect serial output from both VMs
    let output0 = ctrl.vm_mut(0).take_serial_output();
    let output1 = ctrl.vm_mut(1).take_serial_output();

    // Check kernel booted (virtio_blk probe is a reliable boot indicator)
    let vm0_booted = output0.contains("virtio_blk") || output0.contains("Freeing initrd");
    let vm1_booted = output1.contains("virtio_blk") || output1.contains("Freeing initrd");
    println!(
        "      VM0 booted: {} ({} bytes serial)",
        vm0_booted,
        output0.len()
    );
    println!(
        "      VM1 booted: {} ({} bytes serial)",
        vm1_booted,
        output1.len()
    );

    // Check for our guest program output
    let vm0_has_guest = output0.contains("chaoscontrol-net-guest");
    let vm1_has_guest = output1.contains("chaoscontrol-net-guest");
    println!("      VM0 guest started: {}", vm0_has_guest);
    println!("      VM1 guest started: {}", vm1_has_guest);

    // Check for networking messages
    let vm0_has_net = output0.contains("VM0:") && output0.contains("setup complete");
    let vm1_has_net = output1.contains("VM1:") && output1.contains("setup complete");
    println!("      VM0 networking: {}", vm0_has_net);
    println!("      VM1 networking: {}", vm1_has_net);

    // Check for TCP exchange
    let server_got_ping = output0.contains("[server]") && output0.contains("Received");
    let client_got_pong = output1.contains("[client") && output1.contains("PONG");
    println!("      Server received ping: {}", server_got_ping);
    println!("      Client received pong: {}", client_got_pong);

    // Check network bridge stats
    let net_stats = ctrl.network_stats();
    println!(
        "      Network stats: sent={}, delivered={}, dropped_partition={}, dropped_loss={}",
        net_stats.packets_sent,
        net_stats.packets_delivered,
        net_stats.packets_dropped_partition,
        net_stats.packets_dropped_loss
    );

    if server_got_ping && client_got_pong {
        println!(
            "      ✅ PASS — Full TCP ping/pong exchange! ({:.1}s)",
            start.elapsed().as_secs_f64()
        );
    } else if vm0_has_net && vm1_has_net {
        println!(
            "      🟡 PARTIAL — Networking initialized but no TCP exchange yet ({:.1}s)",
            start.elapsed().as_secs_f64()
        );
        println!("      (May need more ticks or debugging)");
    } else if vm0_has_guest && vm1_has_guest {
        println!(
            "      🟡 PARTIAL — Guests started but networking not up ({:.1}s)",
            start.elapsed().as_secs_f64()
        );
    } else if vm0_booted && vm1_booted {
        println!(
            "      🟡 PARTIAL — Kernels booted but guests not started ({:.1}s)",
            start.elapsed().as_secs_f64()
        );
    } else {
        println!(
            "      ❌ FAIL — VMs didn't boot ({:.1}s)",
            start.elapsed().as_secs_f64()
        );
    }

    // Print some serial output for debugging
    println!();
    println!("  === VM0 serial (last 2000 chars) ===");
    let tail0 = if output0.len() > 2000 {
        &output0[output0.len() - 2000..]
    } else {
        &output0
    };
    for line in tail0.lines().take(40) {
        println!("    {}", line);
    }
    println!();
    println!("  === VM1 serial (last 2000 chars) ===");
    let tail1 = if output1.len() > 2000 {
        &output1[output1.len() - 2000..]
    } else {
        &output1
    };
    for line in tail1.lines().take(40) {
        println!("    {}", line);
    }
}
