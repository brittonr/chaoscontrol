//! Boot a Linux kernel in the deterministic VMM.
//!
//! Usage: cargo run --bin boot -- <path-to-vmlinux>

use chaoscontrol_vmm::vm::DeterministicVm;
use std::env;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <kernel-path>", args[0]);
        eprintln!("  kernel-path: Path to an uncompressed vmlinux ELF kernel");
        std::process::exit(1);
    }

    let kernel_path = &args[1];
    let memory_size = 256 * 1024 * 1024; // 256MB

    log::info!("Creating VM with {}MB memory", memory_size / 1024 / 1024);
    let mut vm = DeterministicVm::new(memory_size).expect("Failed to create VM");

    log::info!("Loading kernel: {}", kernel_path);
    vm.load_kernel(kernel_path).expect("Failed to load kernel");

    log::info!("Running VM...");
    match vm.run() {
        Ok(()) => log::info!("VM exited normally"),
        Err(e) => log::error!("VM error: {}", e),
    }
}
