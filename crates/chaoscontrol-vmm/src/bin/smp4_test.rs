//! Actual-run 4/8-vCPU boot diagnostic; strict admission remains the default.
use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};

#[path = "common/smp_progress.rs"]
mod smp_progress;
use smp_progress::parse_progress_args;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let (args, progress) =
        parse_progress_args(std::env::args().skip(1), false).unwrap_or_else(|error| {
            eprintln!("error: smp4_test: {error}");
            std::process::exit(2);
        });
    let (kernel, initrd) = match args.as_slice() {
        [] => ("result-dev/vmlinux", "guest/initrd.gz"),
        [kernel, initrd] => (kernel.as_str(), initrd.as_str()),
        _ => {
            eprintln!("Usage: smp4_test [<kernel> <initrd>] [--smp-progress-mode exact-single-step|kvm-debug-boundary] [--smp-boundary-profile kvm-debug-exit-boundary-v1 --smp-boundary-quantum count]");
            std::process::exit(2);
        }
    };
    println!("SMP selection: {}", progress.describe());

    let mut all_online = true;
    for num_vcpus in [4, 8] {
        println!("\n=== Testing {} vCPUs ===", num_vcpus);
        let mut config = VmConfig {
            num_vcpus,
            ..Default::default()
        };
        progress.apply(&mut config).unwrap_or_else(|error| {
            eprintln!("error: smp4_test: coarse profile rejected: {error}");
            std::process::exit(2);
        });
        let mut vm = DeterministicVm::new(config).unwrap_or_else(|error| {
            eprintln!("error: smp4_test: cannot create {num_vcpus}-vCPU VM: {error}");
            std::process::exit(1);
        });
        vm.load_kernel(kernel, Some(initrd)).expect("load kernel");

        let mut output = String::new();
        for chunk in 0..30 {
            let (_exits, halted) = vm.run_bounded(10_000).expect("run");
            output.push_str(&vm.take_serial_output());
            if progress.is_coarse() {
                let trace = vm
                    .take_schedule_trace()
                    .expect("drain coarse schedule evidence");
                println!(
                    "  Boundary trace chunk {chunk}: {} records",
                    trace.records.len()
                );
            }
            if output.contains("Brought up") || output.contains("login:") || halted {
                println!(
                    "  Completed at chunk {} (total exits={})",
                    chunk,
                    vm.exit_count()
                );
                break;
            }
            if chunk % 5 == 4 {
                println!("  chunk {} exits={}", chunk, vm.exit_count());
            }
        }

        // Check CPU detection
        let expected = format!("{} CPUs", num_vcpus);
        let brought_up = output.contains(&format!("Brought up 1 node, {}", expected));
        let detected = output.contains(&format!("Allowing {} present CPUs", num_vcpus));
        let activated = output.contains(&format!("Total of {} processors activated", num_vcpus));

        println!("  Brought up: {}", brought_up);
        println!("  Detected: {}", detected);
        println!("  Activated: {}", activated);
        println!("  Total exits: {}", vm.exit_count());
        println!("  Virtual TSC: {}", vm.virtual_tsc());

        // Print SMP-related lines
        for line in output.lines() {
            let l = line.to_lowercase();
            if l.contains("cpu")
                || l.contains("smp")
                || l.contains("brought")
                || l.contains("apic")
                || l.contains("processor")
            {
                println!("  | {}", line.trim());
            }
        }

        if !brought_up {
            println!("  ❌ FAILED to bring up all CPUs");
            all_online = false;
        } else {
            println!("  ✅ All {} CPUs online", num_vcpus);
        }
    }
    if !all_online {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_vmm::scheduler::core::{ProgressMode, ScheduleError};
    use chaoscontrol_vmm::vm::VmError;

    #[test]
    fn larger_smp_guests_reject_both_exact_modes_before_boot() {
        for num_vcpus in [4, 8] {
            for mode in [
                ProgressMode::ExactSingleStep,
                ProgressMode::PmuAccelerated {
                    exact_step_margin: 64,
                },
            ] {
                let error = DeterministicVm::new(VmConfig {
                    num_vcpus,
                    smp_progress_mode: mode,
                    ..Default::default()
                })
                .err()
                .expect("portable KVM must reject SMP");
                assert!(matches!(
                    error,
                    VmError::Schedule {
                        source: ScheduleError::MissingCapability {
                            capability: "exact guest-instruction boundaries across IRQ/NMI/synchronous exception entry",
                        },
                    }
                ));
            }
        }
    }
}
