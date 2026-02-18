//! Demonstrate snapshot and restore:
//! 1. Boot Linux to userspace
//! 2. Wait for heartbeat N
//! 3. Snapshot the VM
//! 4. Continue running, collect heartbeats N+1, N+2, ...
//! 5. Restore from snapshot
//! 6. Continue running again — should see heartbeats starting from N+1 again
//!
//! Usage: cargo run --release --bin snapshot_demo -- <kernel> <initrd>

use chaoscontrol_vmm::vm::DeterministicVm;
use std::env;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <kernel-path> <initrd-path>", args[0]);
        std::process::exit(1);
    }

    let kernel_path = &args[1];
    let initrd_path = &args[2];
    let memory_size = 256 * 1024 * 1024;

    // === Phase 1: Boot and wait for init ===
    log::info!("=== Phase 1: Boot VM ===");
    let mut vm = DeterministicVm::new(memory_size).expect("Failed to create VM");
    vm.load_kernel(kernel_path, Some(initrd_path))
        .expect("Failed to load kernel");

    // Run until we see "heartbeat 3"
    log::info!("Running until heartbeat 3...");
    let output = vm.run_until("heartbeat 3").expect("Failed to run VM");

    // Count heartbeats we've seen
    let heartbeats: Vec<&str> = output
        .lines()
        .filter(|l| l.contains("heartbeat"))
        .collect();
    log::info!(
        "Phase 1 complete. Saw {} heartbeats: {:?}",
        heartbeats.len(),
        heartbeats
    );

    // === Phase 2: Snapshot ===
    log::info!("=== Phase 2: Taking snapshot ===");
    let snapshot = vm.snapshot().expect("Failed to take snapshot");
    log::info!(
        "Snapshot taken: {} MB memory",
        snapshot.memory.len() / 1024 / 1024
    );

    // === Phase 3: Continue running after snapshot ===
    log::info!("=== Phase 3: Continue running (post-snapshot) ===");
    let post_snap_output = vm
        .run_until("heartbeat 6")
        .expect("Failed to continue running");
    let post_heartbeats: Vec<&str> = post_snap_output
        .lines()
        .filter(|l| l.contains("heartbeat"))
        .collect();
    log::info!(
        "Post-snapshot: saw {} heartbeats: {:?}",
        post_heartbeats.len(),
        post_heartbeats
    );

    // === Phase 4: Restore from snapshot ===
    log::info!("=== Phase 4: Restoring from snapshot ===");
    vm.restore(&snapshot).expect("Failed to restore snapshot");
    log::info!("Snapshot restored!");

    // === Phase 5: Run again — should see heartbeats from where snapshot was ===
    log::info!("=== Phase 5: Running from restored snapshot ===");
    let restored_output = vm
        .run_until("heartbeat 6")
        .expect("Failed to run after restore");
    let restored_heartbeats: Vec<&str> = restored_output
        .lines()
        .filter(|l| l.contains("heartbeat"))
        .collect();
    log::info!(
        "Restored run: saw {} heartbeats: {:?}",
        restored_heartbeats.len(),
        restored_heartbeats
    );

    // === Summary ===
    println!();
    println!("=== SNAPSHOT/RESTORE DEMO RESULTS ===");
    println!("Phase 1 (boot→snap): heartbeats {:?}", heartbeats);
    println!("Phase 3 (post-snap):  heartbeats {:?}", post_heartbeats);
    println!("Phase 5 (restored):   heartbeats {:?}", restored_heartbeats);
    println!();

    if !restored_heartbeats.is_empty() {
        println!("✅ Snapshot/restore working — VM resumed execution after restore");
    } else {
        println!("❌ No heartbeats after restore");
    }
}
