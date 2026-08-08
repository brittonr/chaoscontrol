//! Privileged eBPF/KVM behavior rail with real exits and IRQ-line traffic.

use std::path::Path;
use std::thread;
use std::time::Duration;

use chaoscontrol_trace::collector::{CollectorBounds, EvidenceTargetConfig, TraceCollector};
use chaoscontrol_trace::events::EventType;
use chaoscontrol_trace::evidence::{
    cleanup_complete, reconcile_accounting, CaptureBounds, CompletenessStatus,
    COMPILED_RING_BUFFER_BYTES,
};
use kvm_bindings::kvm_userspace_memory_region;
use kvm_ioctls::{Kvm, VcpuExit};
use vm_memory::{Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

const TEST_MEMORY_BYTES: usize = 4_096;
const MEMORY_SLOT: u32 = 0;
const GUEST_RFLAGS_RESERVED: u64 = 2;
const GUEST_CODE: [u8; 1] = [0xf4];
const TEST_IRQ: u32 = 4;
const CAPTURE_WAIT: Duration = Duration::from_millis(250);
const MAXIMUM_EVENTS: u64 = 4_096;
const MAXIMUM_POLLS: u64 = 4_096;
const MAXIMUM_ARTIFACT_BYTES: u64 = 4 * 1_024 * 1_024;
const AGGREGATE_WINDOW_EVENTS: u64 = 256;
const VMM_PROFILE_REF: &str =
    "blake3:0000000000000000000000000000000000000000000000000000000000000000";
const REQUIRED_PATHS: [&str; 3] = [
    "/dev/kvm",
    "/sys/kernel/btf/vmlinux",
    "/sys/kernel/tracing/events/kvm/kvm_exit/format",
];

#[test]
#[ignore = "requires root BPF capability, tracefs, exact KVM tracepoints, BTF, and /dev/kvm"]
fn privileged_capture_reconciles_real_kvm_exit_and_irq_traffic() {
    let missing: Vec<_> = REQUIRED_PATHS
        .iter()
        .filter(|path| !Path::new(path).exists())
        .copied()
        .collect();
    assert!(
        unsafe { libc::geteuid() } == 0 && missing.is_empty(),
        "ebpf KVM smoke blocked: root and required paths are required; missing={missing:?}"
    );

    let target_pid = std::process::id();
    let mut collector = TraceCollector::with_evidence_target(
        target_pid,
        CollectorBounds {
            maximum_events: MAXIMUM_EVENTS,
            maximum_polls: MAXIMUM_POLLS,
        },
        EvidenceTargetConfig {
            run_id: "ebpf-kvm-smoke".to_string(),
            vmm_profile_ref: VMM_PROFILE_REF.to_string(),
        },
    )
    .expect("open stable evidence target");
    collector.start().expect("attach bounded eBPF collector");

    run_interrupt_driven_guest();
    thread::sleep(CAPTURE_WAIT);
    collector.stop().expect("stop bounded eBPF collector");

    let events = collector.events();
    let accounting = reconcile_accounting(
        &collector.producer_accounting(),
        &collector.userspace_accounting(),
        &CaptureBounds {
            maximum_ring_bytes: COMPILED_RING_BUFFER_BYTES,
            maximum_queue_events: MAXIMUM_EVENTS,
            maximum_polls: MAXIMUM_POLLS,
            maximum_events: MAXIMUM_EVENTS,
            maximum_artifact_bytes: MAXIMUM_ARTIFACT_BYTES,
            aggregate_window_events: AGGREGATE_WINDOW_EVENTS,
        },
    )
    .expect("reconcile producer and userspace accounting");

    assert_eq!(accounting.status, CompletenessStatus::Complete);
    assert!(cleanup_complete(&collector.cleanup_outcome()));
    assert_eq!(
        collector
            .target_binding_report()
            .expect("target binding")
            .status,
        chaoscontrol_trace::evidence::TargetBindingStatus::Stable
    );
    assert!(events
        .iter()
        .any(|event| event.event_type() == EventType::KvmExit));
    assert!(
        events.iter().any(|event| matches!(
            event.event_type(),
            EventType::KvmSetIrq | EventType::KvmPicIrq | EventType::KvmInjVirq
        )),
        "expected real IRQ-line or interrupt-injection trace traffic"
    );
}

fn run_interrupt_driven_guest() {
    let kvm = Kvm::new().expect("open KVM");
    let vm = kvm.create_vm().expect("create VM");
    vm.create_irq_chip().expect("create in-kernel IRQ chip");

    let memory: GuestMemoryMmap<()> =
        GuestMemoryMmap::from_ranges(&[(GuestAddress(0), TEST_MEMORY_BYTES)])
            .expect("allocate guest memory");
    memory
        .write_slice(&GUEST_CODE, GuestAddress(0))
        .expect("write guest code");
    let userspace_addr = memory
        .get_host_address(GuestAddress(0))
        .expect("resolve guest memory") as u64;
    let region = kvm_userspace_memory_region {
        slot: MEMORY_SLOT,
        guest_phys_addr: 0,
        memory_size: u64::try_from(TEST_MEMORY_BYTES).expect("memory size fits u64"),
        userspace_addr,
        flags: 0,
    };
    // SAFETY: `memory` stays alive until the VM and vCPU are dropped.
    unsafe { vm.set_user_memory_region(region) }.expect("register guest memory");

    let mut vcpu = vm.create_vcpu(0).expect("create vCPU");
    let mut sregs = vcpu.get_sregs().expect("read special registers");
    sregs.cs.base = 0;
    sregs.cs.selector = 0;
    vcpu.set_sregs(&sregs).expect("set real-mode code segment");
    let mut regs = vcpu.get_regs().expect("read registers");
    regs.rip = 0;
    regs.rflags = GUEST_RFLAGS_RESERVED;
    vcpu.set_regs(&regs).expect("set registers");

    vm.set_irq_line(TEST_IRQ, true).expect("assert IRQ line");
    vm.set_irq_line(TEST_IRQ, false).expect("deassert IRQ line");
    assert!(matches!(vcpu.run().expect("run guest"), VcpuExit::Hlt));
}
