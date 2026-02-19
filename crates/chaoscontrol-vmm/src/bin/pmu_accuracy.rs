//! PMU accuracy test: verify counter is deterministic across runs.

use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
use std::env;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <kernel> <initrd>", args[0]);
        std::process::exit(1);
    }

    let counter = match chaoscontrol_vmm::perf::InstructionCounter::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("PMU not available: {}", e);
            std::process::exit(1);
        }
    };

    // Run single-vCPU boot, recording PMU counter at each step
    let mut config = VmConfig::default();
    config.num_vcpus = 1;
    let mut vm = DeterministicVm::new(config).expect("create VM");
    vm.load_kernel(&args[1], Some(&args[2]))
        .expect("load kernel");

    counter.reset_and_enable();
    counter.disable();

    let mut values = Vec::new();
    let mut total_steps = 0;

    // Record first 200 step counter values
    for _ in 0..200 {
        counter.resume();
        let (_, halted) = vm.run_bounded(1).expect("run");
        counter.disable();
        let val = counter.read();
        values.push(val);
        total_steps += 1;
        if halted {
            break;
        }
    }

    // Print first 50 values
    for (i, v) in values.iter().enumerate().take(50) {
        println!("step {:4}: counter={}", i, v);
    }
    println!("... total {} steps", total_steps);
}
