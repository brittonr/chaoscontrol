//! KVM continuation evidence for the exact vCPU snapshot profile.

use chaoscontrol_vmm::snapshot::VcpuSnapshot;
use kvm_bindings::{
    kvm_mp_state, kvm_msr_entry, kvm_userspace_memory_region, Msrs, KVM_MP_STATE_RUNNABLE,
};
use kvm_ioctls::{Kvm, VcpuExit, VcpuFd, VmFd};
use vm_memory::{Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

const TEST_MEMORY_BYTES: usize = 4096;
const CODE_ADDRESS: u64 = 0x200;
const NMI_HANDLER_ADDRESS: u16 = 0x100;
const ARCHITECTURAL_RETURN_ADDRESS: u16 = CODE_ADDRESS as u16 + 10;
const XCR0_X87_AND_SSE: u64 = 3;
const NMI_OUTPUT: u8 = 0x44;
const MSR_OUTPUT: u8 = 0xa6;
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
const IA32_SYSENTER_ESP: u32 = 0x175;
const SNAPSHOT_SYSENTER_ESP: u64 = 0x1234_50a6;
const MUTATED_SYSENTER_ESP: u64 = 0x5678_9000;
const GUEST_CODE: [u8; 34] = [
    0xB0,
    FIRST_OUTPUT,
    0xE6,
    OUTPUT_PORT as u8,
    // Leave the masked-NMI region through an actual architectural IRET.
    // Rewriting KVM event state to unmask is not guest continuation.
    0x9c, // pushf
    0x0e, // push cs
    0x68,
    ARCHITECTURAL_RETURN_ADDRESS as u8,
    (ARCHITECTURAL_RETURN_ADDRESS >> 8) as u8, // push return IP
    0xcf,                                      // iret
    // Observe the restored required MSR, then the restored XMM register.
    0x66,
    0xb9,
    0x75,
    0x01,
    0x00,
    0x00, // mov ecx, IA32_SYSENTER_ESP
    0x0f,
    0x32, // rdmsr
    0xe6,
    OUTPUT_PORT as u8,
    0x66,
    0x0f,
    0x7e,
    0xc0, // movd eax, xmm0
    0xe6,
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
            .write_slice(&GUEST_CODE, GuestAddress(CODE_ADDRESS))
            .expect("write guest code");
        memory
            .write_obj(NMI_HANDLER_ADDRESS, GuestAddress(2 * 4))
            .expect("write NMI vector offset");
        memory
            .write_obj(0u16, GuestAddress(2 * 4 + 2))
            .expect("write NMI vector selector");
        memory
            .write_slice(
                &[0xb0, NMI_OUTPUT, 0xe6, OUTPUT_PORT as u8, 0xcf], // out; iret
                GuestAddress(u64::from(NMI_HANDLER_ADDRESS)),
            )
            .expect("write NMI handler");
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
        let cpuid =
            chaoscontrol_vmm::cpu::filter_cpuid(&kvm, &chaoscontrol_vmm::cpu::CpuConfig::default())
                .expect("admit production CPU feature profile");
        vcpu.set_cpuid2(&cpuid).expect("set CPU feature profile");
        let mut xcrs = vcpu.get_xcrs().expect("read extended-state controls");
        assert_eq!(xcrs.nr_xcrs, 1);
        assert_eq!(xcrs.xcrs[0].xcr, 0);
        xcrs.xcrs[0].value = XCR0_X87_AND_SSE;
        vcpu.set_xcrs(&xcrs)
            .expect("enable retained x87 and SSE state");
        let mut sregs = vcpu.get_sregs().expect("read special registers");
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        sregs.cr4 |= 1 << 9; // OSFXSR: permit the guest's XMM observation.
        vcpu.set_sregs(&sregs).expect("set real-mode code segment");
        let mut regs = vcpu.get_regs().expect("read registers");
        regs.rip = CODE_ADDRESS;
        regs.rsp = 0xf00; // Real-mode NMI frame and IRET use the retained guest stack.
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
        (0..5).map(|_| self.next_output()).collect()
    }

    fn stage_pending_event_and_extended_state(&self) {
        self.set_sysenter_esp(SNAPSHOT_SYSENTER_ESP);
        self.set_xmm_marker(SNAPSHOT_XMM_MARKER);

        let mut events = self.vcpu.get_vcpu_events().expect("read vCPU events");
        events.nmi.pending = EVENT_SET;
        events.nmi.masked = EVENT_SET;
        self.vcpu
            .set_vcpu_events(&events)
            .expect("stage masked pending NMI");
    }

    fn mutate_pending_event_and_extended_state(&self) {
        self.set_sysenter_esp(MUTATED_SYSENTER_ESP);
        self.set_xmm_marker(MUTATED_XMM_MARKER);

        let mut events = self.vcpu.get_vcpu_events().expect("read vCPU events");
        events.nmi.pending = EVENT_CLEAR;
        events.nmi.masked = EVENT_CLEAR;
        self.vcpu
            .set_vcpu_events(&events)
            .expect("clear pending NMI");
    }

    fn assert_pending_event_and_extended_state_restored(&self) {
        let mut msrs = Msrs::from_entries(&[kvm_msr_entry {
            index: IA32_SYSENTER_ESP,
            ..Default::default()
        }])
        .unwrap();
        assert_eq!(self.vcpu.get_msrs(&mut msrs).unwrap(), 1);
        assert_eq!(msrs.as_slice()[0].data, SNAPSHOT_SYSENTER_ESP);
        let fpu = self.vcpu.get_fpu().expect("read restored FPU state");
        assert_eq!(fpu.xmm[0][0], SNAPSHOT_XMM_MARKER);

        let events = self
            .vcpu
            .get_vcpu_events()
            .expect("read restored vCPU events");
        assert_eq!(events.nmi.pending, EVENT_SET);
        assert_eq!(events.nmi.masked, EVENT_SET);
    }

    fn set_xmm_marker(&self, marker: u8) {
        // Standard XSAVE: XMM0 starts at byte160; XSTATE_BV at byte512.
        // KVM_SET_FPU alone does not mark the SSE component active.
        const XMM0_WORD: usize = 160 / std::mem::size_of::<u32>();
        const XSTATE_BV_WORD: usize = 512 / std::mem::size_of::<u32>();
        const SSE_ACTIVE: u32 = 1 << 1;
        let mut xsave = self.vcpu.get_xsave().expect("read selected extended state");
        xsave.region[XMM0_WORD] = (xsave.region[XMM0_WORD] & !0xff) | u32::from(marker);
        xsave.region[XSTATE_BV_WORD] |= SSE_ACTIVE;
        self.vcpu.set_xsave(&xsave).expect("stage active SSE state");
    }

    fn set_sysenter_esp(&self, value: u64) {
        assert!(self.msr_indices.contains(&IA32_SYSENTER_ESP));
        let msrs = Msrs::from_entries(&[kvm_msr_entry {
            index: IA32_SYSENTER_ESP,
            data: value,
            ..Default::default()
        }])
        .unwrap();
        assert_eq!(self.vcpu.set_msrs(&msrs).unwrap(), 1);
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
        (OUTPUT_PORT, vec![NMI_OUTPUT]),
        (OUTPUT_PORT, vec![MSR_OUTPUT]),
        (OUTPUT_PORT, vec![SNAPSHOT_XMM_MARKER]),
        (OUTPUT_PORT, vec![SECOND_OUTPUT]),
        (OUTPUT_PORT, vec![THIRD_OUTPUT]),
    ];
    assert_eq!(uninterrupted, expected);
    assert_eq!(replayed, expected);
}
