//! KVM continuation evidence for the exact vCPU snapshot profile.

use chaoscontrol_vmm::snapshot::VcpuSnapshot;
use kvm_bindings::{kvm_mp_state, kvm_userspace_memory_region, KVM_MP_STATE_RUNNABLE};
use kvm_ioctls::{Kvm, VcpuExit, VcpuFd, VmFd};
use vm_memory::{Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

const TEST_MEMORY_BYTES: usize = 4096;
const MEMORY_SLOT: u32 = 0;
const GUEST_RFLAGS_RESERVED: u64 = 2;
const OUTPUT_PORT: u16 = 0x10;
const FIRST_OUTPUT: u8 = 0x11;
const SECOND_OUTPUT: u8 = 0x22;
const THIRD_OUTPUT: u8 = 0x33;
const SNAPSHOT_XMM_MARKER: u8 = 0xA5;
const MUTATED_XMM_MARKER: u8 = 0x5A;
const EVENT_SET: u8 = 1;
const EVENT_CLEAR: u8 = 0;
const GUEST_CODE: [u8; 12] = [
    0xB0,
    FIRST_OUTPUT,
    0xE6,
    OUTPUT_PORT as u8,
    0xB0,
    SECOND_OUTPUT,
    0xE6,
    OUTPUT_PORT as u8,
    0xB0,
    THIRD_OUTPUT,
    0xE6,
    OUTPUT_PORT as u8,
];

struct ContinuationHarness {
    _vm: VmFd,
    _memory: GuestMemoryMmap,
    vcpu: VcpuFd,
    msr_indices: Vec<u32>,
}

impl ContinuationHarness {
    fn new() -> Self {
        let kvm = Kvm::new().expect("open KVM");
        let mut msr_indices = kvm
            .get_msr_index_list()
            .expect("read KVM MSR inventory")
            .as_slice()
            .to_vec();
        msr_indices.sort_unstable();
        msr_indices.dedup();
        let vm = kvm.create_vm().expect("create KVM VM");
        vm.create_irq_chip().expect("create in-kernel IRQ chip");
        let memory = GuestMemoryMmap::from_ranges(&[(GuestAddress(0), TEST_MEMORY_BYTES)])
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
            memory_size: TEST_MEMORY_BYTES as u64,
            userspace_addr,
            flags: 0,
        };
        // SAFETY: this harness owns the mmap for longer than the VM and vCPU.
        unsafe { vm.set_user_memory_region(region) }.expect("register guest memory");

        let vcpu = vm.create_vcpu(0).expect("create vCPU");
        let mut sregs = vcpu.get_sregs().expect("read special registers");
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        vcpu.set_sregs(&sregs).expect("set real-mode code segment");
        let mut regs = vcpu.get_regs().expect("read registers");
        regs.rip = 0;
        regs.rflags = GUEST_RFLAGS_RESERVED;
        vcpu.set_regs(&regs).expect("set initial registers");
        vcpu.set_mp_state(kvm_mp_state {
            mp_state: KVM_MP_STATE_RUNNABLE,
        })
        .expect("make vCPU runnable");

        Self {
            _vm: vm,
            _memory: memory,
            vcpu,
            msr_indices,
        }
    }

    fn next_output(&mut self) -> (u16, Vec<u8>) {
        match self.vcpu.run().expect("run guest") {
            VcpuExit::IoOut(port, data) => (port, data.to_vec()),
            other => panic!("expected guest output, got {other:?}"),
        }
    }

    fn complete_pending_exit(&mut self) {
        self.vcpu.set_kvm_immediate_exit(1);
        let interrupted = match self.vcpu.run() {
            Ok(VcpuExit::Intr) => true,
            Err(error) => error.errno() == libc::EINTR,
            Ok(_) => false,
        };
        self.vcpu.set_kvm_immediate_exit(0);
        assert!(interrupted);
    }

    fn remaining_trace(&mut self) -> Vec<(u16, Vec<u8>)> {
        vec![self.next_output(), self.next_output()]
    }

    fn stage_pending_event_and_extended_state(&self) {
        let mut fpu = self.vcpu.get_fpu().expect("read FPU state");
        fpu.xmm[0][0] = SNAPSHOT_XMM_MARKER;
        self.vcpu.set_fpu(&fpu).expect("stage extended state");

        let mut events = self.vcpu.get_vcpu_events().expect("read vCPU events");
        events.nmi.pending = EVENT_SET;
        events.nmi.masked = EVENT_SET;
        self.vcpu
            .set_vcpu_events(&events)
            .expect("stage masked pending NMI");
    }

    fn mutate_pending_event_and_extended_state(&self) {
        let mut fpu = self.vcpu.get_fpu().expect("read FPU state");
        fpu.xmm[0][0] = MUTATED_XMM_MARKER;
        self.vcpu.set_fpu(&fpu).expect("mutate extended state");

        let mut events = self.vcpu.get_vcpu_events().expect("read vCPU events");
        events.nmi.pending = EVENT_CLEAR;
        events.nmi.masked = EVENT_CLEAR;
        self.vcpu
            .set_vcpu_events(&events)
            .expect("clear pending NMI");
    }

    fn assert_pending_event_and_extended_state_restored(&self) {
        let fpu = self.vcpu.get_fpu().expect("read restored FPU state");
        assert_eq!(fpu.xmm[0][0], SNAPSHOT_XMM_MARKER);

        let events = self
            .vcpu
            .get_vcpu_events()
            .expect("read restored vCPU events");
        assert_eq!(events.nmi.pending, EVENT_SET);
        assert_eq!(events.nmi.masked, EVENT_SET);
    }
}

#[test]
fn serialized_vcpu_snapshot_replays_identical_guest_outputs_and_exit_sequence() {
    let mut harness = ContinuationHarness::new();
    assert_eq!(harness.next_output(), (OUTPUT_PORT, vec![FIRST_OUTPUT]));
    harness.complete_pending_exit();
    harness.stage_pending_event_and_extended_state();

    let snapshot = VcpuSnapshot::capture(&harness.vcpu, &harness.msr_indices)
        .expect("capture exact vCPU state");
    let encoded = serde_json::to_vec(&snapshot).expect("serialize exact vCPU state");
    let decoded: VcpuSnapshot =
        serde_json::from_slice(&encoded).expect("deserialize exact vCPU state");

    let uninterrupted = harness.remaining_trace();
    harness.complete_pending_exit();
    harness.mutate_pending_event_and_extended_state();
    decoded
        .restore(&harness.vcpu)
        .expect("restore exact vCPU state");
    harness.assert_pending_event_and_extended_state_restored();
    let replayed = harness.remaining_trace();

    let expected = vec![
        (OUTPUT_PORT, vec![SECOND_OUTPUT]),
        (OUTPUT_PORT, vec![THIRD_OUTPUT]),
    ];
    assert_eq!(uninterrupted, expected);
    assert_eq!(replayed, expected);
}
