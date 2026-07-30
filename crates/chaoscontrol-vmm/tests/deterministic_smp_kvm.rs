//! KVM evidence for exact deterministic SMP scheduling.

use chaoscontrol_vmm::scheduler::core::{
    plan_execution_observation, validate_transition_trace, ExecutionProgressObservation,
    ProgressMode, RunnableChange, ScheduleAction, ScheduleError, ScheduleEvent, ScheduleTrace,
};
use chaoscontrol_vmm::scheduler::{SchedulerConfig, SchedulingStrategy, VcpuScheduler};
use kvm_bindings::{
    kvm_guest_debug, kvm_mp_state, kvm_userspace_memory_region, KVM_GUESTDBG_ENABLE,
    KVM_GUESTDBG_SINGLESTEP, KVM_MP_STATE_RUNNABLE,
};
use kvm_ioctls::{Kvm, VcpuExit, VcpuFd, VmFd};
use std::hint::black_box;
use std::thread;
use std::time::Duration;
use vm_memory::{Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

const TEST_VCPU_COUNT: usize = 2;
const TEST_QUANTUM: u64 = 8;
const TEST_INSTRUCTION_COUNT: usize = 64;
const TEST_JOURNAL_LIMIT: usize = 128;
const TEST_MEMORY_BYTES: usize = 4096;
const HOST_DELAY_MICROSECONDS: u64 = 50;
const CONTENTION_ITERATIONS: usize = 10_000;
const GUEST_RFLAGS_RESERVED: u64 = 2;
const MEMORY_SLOT: u32 = 0;
const GUEST_CODE: [u8; 3] = [0x40, 0xeb, 0xfd]; // inc ax; jmp 0
const HLT_CODE: [u8; 1] = [0xf4];

#[derive(Clone, Copy)]
enum HostPerturbation {
    None,
    Delay,
    Contention,
}

struct ExactSpinHarness {
    _vm: VmFd,
    _memory: GuestMemoryMmap,
    vcpus: Vec<VcpuFd>,
    scheduler: VcpuScheduler,
}

impl ExactSpinHarness {
    fn new() -> Self {
        Self::new_with_code(&GUEST_CODE)
    }

    fn new_with_code(code: &[u8]) -> Self {
        let kvm = Kvm::new().expect("open KVM");
        let vm = kvm.create_vm().expect("create KVM VM");
        let memory = GuestMemoryMmap::from_ranges(&[(GuestAddress(0), TEST_MEMORY_BYTES)])
            .expect("allocate guest memory");
        memory
            .write_slice(code, GuestAddress(0))
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
        // SAFETY: the mmap remains owned by this harness for the VM lifetime.
        unsafe { vm.set_user_memory_region(region) }.expect("register guest memory");

        let mut vcpus = Vec::with_capacity(TEST_VCPU_COUNT);
        for id in 0..TEST_VCPU_COUNT {
            let vcpu = vm.create_vcpu(id as u64).expect("create vCPU");
            let mut sregs = vcpu.get_sregs().expect("read special registers");
            sregs.cs.base = 0;
            sregs.cs.selector = 0;
            vcpu.set_sregs(&sregs).expect("set real-mode code segment");
            let mut regs = vcpu.get_regs().expect("read registers");
            regs.rip = 0;
            regs.rflags = GUEST_RFLAGS_RESERVED;
            regs.rax = id as u64;
            vcpu.set_regs(&regs).expect("set initial registers");
            vcpu.set_mp_state(kvm_mp_state {
                mp_state: KVM_MP_STATE_RUNNABLE,
            })
            .expect("make vCPU runnable");
            set_single_step(&vcpu, true);
            vcpus.push(vcpu);
        }

        let scheduler_config = SchedulerConfig {
            num_vcpus: TEST_VCPU_COUNT,
            quantum: TEST_QUANTUM,
            strategy: SchedulingStrategy::RoundRobin,
            seed: 42,
        };
        let scheduler = VcpuScheduler::try_new_with_journal_limit(
            &scheduler_config,
            ProgressMode::ExactSingleStep,
            vec![true; TEST_VCPU_COUNT],
            TEST_JOURNAL_LIMIT,
        )
        .expect("create bounded scheduler");
        Self {
            _vm: vm,
            _memory: memory,
            vcpus,
            scheduler,
        }
    }

    fn step(&mut self) {
        let active = self.scheduler.active();
        let reservation = self
            .scheduler
            .reserve_transition()
            .expect("reserve evidence before KVM_RUN");
        assert!(matches!(
            self.vcpus[active].run().expect("single-step guest"),
            VcpuExit::Debug(_)
        ));
        let planned = plan_execution_observation(
            self.scheduler.state(),
            ExecutionProgressObservation::ExactInstruction { vcpu: active },
        )
        .expect("validate exact progress")
        .expect("exact instruction produces a transition");
        self.scheduler
            .commit(reservation, planned)
            .expect("commit reserved evidence");
    }

    fn retire_hlt(&mut self) {
        let active = self.scheduler.active();
        let reservation = self.scheduler.reserve_transition().unwrap();
        assert!(matches!(
            self.vcpus[active].run().expect("run HLT"),
            VcpuExit::Hlt
        ));
        let planned = plan_execution_observation(
            self.scheduler.state(),
            ExecutionProgressObservation::ExactInstructionHalt { vcpu: active },
        )
        .unwrap()
        .unwrap();
        self.scheduler.commit(reservation, planned).unwrap();
    }

    fn wake_vcpu(&mut self, vcpu: usize) {
        self.vcpus[vcpu]
            .set_mp_state(kvm_mp_state {
                mp_state: KVM_MP_STATE_RUNNABLE,
            })
            .expect("wake halted vCPU");
        let reservation = self.scheduler.reserve_transition().unwrap();
        let event = ScheduleEvent::RunnableObservation {
            expected_state_id: self.scheduler.state_id(),
            runnable_changes: vec![RunnableChange {
                vcpu,
                runnable: true,
            }],
        };
        let planned = self.scheduler.plan(&event).unwrap();
        self.scheduler.commit(reservation, planned).unwrap();
    }

    fn run(mut self, perturbation: HostPerturbation) -> (ScheduleTrace, Vec<u64>) {
        for instruction in 0..TEST_INSTRUCTION_COUNT {
            apply_host_perturbation(perturbation, instruction);
            self.step();
        }
        let registers = self
            .vcpus
            .iter()
            .map(|vcpu| vcpu.get_regs().expect("read final registers").rax)
            .collect();
        let trace = self.scheduler.drain_trace().expect("drain KVM trace");
        validate_transition_trace(&trace, TEST_JOURNAL_LIMIT).expect("validate KVM trace");
        (trace, registers)
    }
}

fn set_single_step(vcpu: &VcpuFd, enabled: bool) {
    let control = if enabled {
        KVM_GUESTDBG_ENABLE | KVM_GUESTDBG_SINGLESTEP
    } else {
        0
    };
    vcpu.set_guest_debug(&kvm_guest_debug {
        control,
        pad: 0,
        arch: Default::default(),
    })
    .expect("configure KVM single-step");
}

fn apply_host_perturbation(perturbation: HostPerturbation, instruction: usize) {
    match perturbation {
        HostPerturbation::None => {}
        HostPerturbation::Delay => {
            if instruction.is_multiple_of(TEST_VCPU_COUNT) {
                thread::sleep(Duration::from_micros(HOST_DELAY_MICROSECONDS));
            }
        }
        HostPerturbation::Contention => {
            let mut value = instruction;
            for iteration in 0..CONTENTION_ITERATIONS {
                value = value.wrapping_add(iteration).rotate_left(1);
                black_box(value);
            }
        }
    }
}

#[test]
fn exact_spin_loop_trace_survives_host_delay_and_contention() {
    let baseline = ExactSpinHarness::new().run(HostPerturbation::None);
    let delayed = ExactSpinHarness::new().run(HostPerturbation::Delay);
    let contended = ExactSpinHarness::new().run(HostPerturbation::Contention);

    assert_eq!(baseline, delayed);
    assert_eq!(baseline, contended);
    assert!(baseline
        .0
        .records
        .iter()
        .any(|record| matches!(record.action, ScheduleAction::Switch { .. })));
}

#[test]
fn bsp_and_ap_hlt_retire_once_then_block_until_an_explicit_wake() {
    let mut harness = ExactSpinHarness::new_with_code(&HLT_CODE);

    harness.retire_hlt();
    assert_eq!(harness.scheduler.state().instruction_progress[0], 1);
    assert!(!harness.scheduler.state().runnable_vcpus[0]);
    assert_eq!(harness.scheduler.active(), 1);
    assert_eq!(
        harness.vcpus[0].get_mp_state().unwrap().mp_state,
        KVM_MP_STATE_RUNNABLE,
        "KVM_EXIT_HLT leaves userspace-owned MP state runnable"
    );

    harness.retire_hlt();
    assert_eq!(harness.scheduler.state().instruction_progress, vec![1, 1]);
    assert!(harness.scheduler.state().halted);
    assert_eq!(
        harness.vcpus[1].get_mp_state().unwrap().mp_state,
        KVM_MP_STATE_RUNNABLE,
        "KVM_EXIT_HLT leaves AP MP state runnable"
    );

    harness.wake_vcpu(0);
    assert!(!harness.scheduler.state().halted);
    assert!(harness.scheduler.state().runnable_vcpus[0]);
    assert_eq!(harness.scheduler.active(), 0);
    assert_eq!(harness.scheduler.state().instruction_progress, vec![1, 1]);
}

#[test]
fn exact_kvm_harness_rejects_an_insufficient_journal_before_guest_progress() {
    let config = SchedulerConfig {
        num_vcpus: TEST_VCPU_COUNT,
        quantum: TEST_QUANTUM,
        strategy: SchedulingStrategy::RoundRobin,
        seed: 42,
    };
    let mut scheduler = VcpuScheduler::try_new_with_journal_limit(
        &config,
        ProgressMode::ExactSingleStep,
        vec![true; TEST_VCPU_COUNT],
        0,
    )
    .expect("zero-capacity journal is an admitted explicit bound");
    let before = scheduler.state_id();

    assert_eq!(
        scheduler.reserve_transition(),
        Err(ScheduleError::JournalCapacityExceeded { limit: 0 })
    );
    assert_eq!(scheduler.state_id(), before);
    assert!(scheduler
        .drain_trace()
        .expect("drain empty bounded trace")
        .records
        .is_empty());
}
