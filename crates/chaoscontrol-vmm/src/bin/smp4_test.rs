// Quick test: boot with 4 vCPUs
use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    for num_vcpus in [4, 8] {
        println!("\n=== Testing {} vCPUs ===", num_vcpus);
        let config = VmConfig {
            num_vcpus,
            ..Default::default()
        };
        let mut vm = DeterministicVm::new(config).expect("create VM");
        vm.load_kernel("result-dev/vmlinux", Some("guest/initrd.gz"))
            .expect("load kernel");

        let mut output = String::new();
        for chunk in 0..30 {
            let (_exits, halted) = vm.run_bounded(10_000).expect("run");
            output.push_str(&vm.take_serial_output());
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
        } else {
            println!("  ✅ All {} CPUs online", num_vcpus);
        }
    }
}
