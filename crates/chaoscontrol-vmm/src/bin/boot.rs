//! Boot a Linux kernel in the deterministic VMM.
//!
//! Usage: cargo run --bin boot -- <kernel-path> [initrd-path]

use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
use std::env;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <kernel-path> [initrd-path]", args[0]);
        std::process::exit(1);
    }

    let kernel_path = &args[1];
    let initrd_path = args.get(2).map(|s| s.as_str());

    let config = VmConfig::default();
    log::info!(
        "Creating VM with {}MB memory, TSC {} kHz",
        config.memory_size / 1024 / 1024,
        config.cpu.tsc_khz,
    );

    let mut vm = DeterministicVm::new(config).expect("Failed to create VM");

    log::info!("Loading kernel: {}", kernel_path);
    if let Some(initrd) = initrd_path {
        log::info!("Loading initrd: {}", initrd);
    }
    vm.load_kernel(kernel_path, initrd_path)
        .expect("Failed to load kernel");

    log::info!("Running VM...");
    match vm.run() {
        Ok(()) => log::info!(
            "VM exited normally ({} exits, virtual TSC: {})",
            vm.exit_count(),
            vm.virtual_tsc(),
        ),
        Err(e) => log::error!("VM error: {}", e),
    }
}
