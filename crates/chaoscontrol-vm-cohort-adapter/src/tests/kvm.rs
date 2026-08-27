use chaoscontrol_vmm::snapshot::{VcpuSnapshot, VmSnapshot};
use kvm_bindings::{
    kvm_irqchip, kvm_mp_state, kvm_pit_config, KVM_IRQCHIP_IOAPIC, KVM_IRQCHIP_PIC_MASTER,
    KVM_IRQCHIP_PIC_SLAVE, KVM_MP_STATE_RUNNABLE,
};
use kvm_ioctls::{Kvm, VcpuFd, VmFd};
use vm_cohort_core::{CohortPhase, FailureClass};
use vm_cohort_kvm::{FailureSchedule, KvmCohortRuntime};

use crate::{
    execute_snapshot_cohort, execute_snapshot_cohort_with_runtime, map_snapshot_cohort,
    restore_vcpu_snapshot, ChaosCohortExecution, SnapshotCohortMappingRequest,
};

use super::support::{
    compatibility_facts, profile, replace_snapshot_topology, resource_ref, synthetic_block,
    synthetic_snapshot,
};

const RESTORED_RIP: u64 = 0x1000;
const REQUIRED_RFLAGS: u64 = 2;
const EXTENDED_STATE_MARKER: u8 = 0xa5;
const EVENT_SET: u8 = 1;
const KVM_WORKER_COUNT: u32 = 1;

// r[verify chaoscontrol.vm_cohort.restore]
#[test]
#[ignore = "requires readable /dev/kvm"]
fn exact_vcpu_state_restores_through_vm_cohort_descriptors() {
    let source = SourceVm::new();
    let snapshot = source.vcpu_snapshot();
    let fixture = vm_cohort_conformance::standard_fixture().expect("standard VM Cohort fixture");
    let mut runtime = KvmCohortRuntime::new(
        fixture.profile,
        &fixture.memory,
        &fixture.plan.clones[0].memory_base_ref,
        fixture.disk,
        &fixture.plan.clones[0].disk_base_ref,
    )
    .expect("VM Cohort runtime");
    let clone_ref = fixture.plan.clones[0].clone_ref.clone();
    for effect in fixture
        .plan
        .effects
        .iter()
        .take_while(|effect| effect.clone_ref == clone_ref)
    {
        runtime
            .execute_effect(&fixture.plan, effect)
            .expect("prepare exact clone");
        if effect.kind == vm_cohort_core::EffectKind::RestoreVcpu {
            break;
        }
    }
    restore_vcpu_snapshot(&snapshot, &runtime, &clone_ref, 0)
        .expect("restore through shared descriptors");
    assert_restored_vcpu(&runtime, &clone_ref);
}

// r[verify chaoscontrol.vm_cohort.restore]
// r[verify chaoscontrol.vm_cohort.selection]
#[test]
#[ignore = "requires readable /dev/kvm"]
fn complete_snapshot_restores_before_shared_cohort_activation() {
    let source = SourceVm::new();
    let snapshot = source.complete_snapshot();
    let block = synthetic_block();
    let kvm_profile = profile();
    let facts = compatibility_facts(&kvm_profile);
    let mapped = map_snapshot_cohort(SnapshotCohortMappingRequest {
        snapshot: &snapshot,
        block: &block,
        facts: &facts,
        kvm_profile,
        workers: KVM_WORKER_COUNT,
        context_ref: resource_ref("live-kvm-activation"),
    })
    .expect("map live exact snapshot");

    let execution = execute_snapshot_cohort(&mapped, &snapshot).expect("execute exact cohort");
    let ChaosCohortExecution::Active(active) = execution else {
        panic!("exact cohort did not activate");
    };
    assert_eq!(active.outcome.state.phase, CohortPhase::Active);
    assert_eq!(active.runtime.active_clone_count(), 1);
    assert!(!active.outcome.fault_authority_granted);
    assert!(!active.outcome.replay_authority_granted);
    assert!(!active.outcome.release_authority_granted);
    assert_restored_vcpu(&active.runtime, &mapped.plan.clones[0].clone_ref);
}

// r[verify chaoscontrol.vm_cohort.verification]
#[test]
#[ignore = "requires readable /dev/kvm"]
fn injected_partial_creation_fails_and_confirms_cleanup_without_activation() {
    let snapshot = synthetic_snapshot();
    let block = synthetic_block();
    let kvm_profile = profile();
    let facts = compatibility_facts(&kvm_profile);
    let mapped = map_snapshot_cohort(SnapshotCohortMappingRequest {
        snapshot: &snapshot,
        block: &block,
        facts: &facts,
        kvm_profile: kvm_profile.clone(),
        workers: KVM_WORKER_COUNT,
        context_ref: resource_ref("partial-creation"),
    })
    .expect("map partial-creation fixture");
    let first_clone = &mapped.plan.clones[0];
    let mut runtime = KvmCohortRuntime::new(
        kvm_profile,
        &mapped.memory,
        &first_clone.memory_base_ref,
        mapped.disk.clone(),
        &first_clone.disk_base_ref,
    )
    .expect("VM Cohort runtime");
    let failed_effect = mapped
        .plan
        .effects
        .iter()
        .find(|effect| effect.kind == vm_cohort_core::EffectKind::PrepareDisk)
        .expect("planned disk preparation");
    let mut failures = FailureSchedule::default();
    failures.insert(failed_effect.operation_ref.clone(), FailureClass::Permanent);
    runtime.set_failure_schedule(failures);

    let execution = execute_snapshot_cohort_with_runtime(&mapped, &snapshot, runtime)
        .expect("typed failed execution");
    let ChaosCohortExecution::Failed(failed) = execution else {
        panic!("injected failure activated the cohort");
    };
    assert_eq!(failed.state.phase, CohortPhase::Cleaned);
    assert!(!failed.cleanup_uncertain);
    assert!(failed.runtime.is_none());
    assert!(failed.state.activated_clones.is_empty());
    assert!(!failed.fault_authority_granted);
    assert!(!failed.replay_authority_granted);
    assert!(!failed.release_authority_granted);
}

fn assert_restored_vcpu(runtime: &KvmCohortRuntime, clone_ref: &vm_cohort_core::CloneRef) {
    let (_vm, vcpus) = runtime
        .clone_descriptors(clone_ref)
        .expect("target descriptors");
    let target = &vcpus[0];
    let regs = target.get_regs().expect("restored registers");
    let fpu = target.get_fpu().expect("restored FPU");
    let events = target.get_vcpu_events().expect("restored events");
    let mp_state = target.get_mp_state().expect("restored MP state");
    assert_eq!(regs.rip, RESTORED_RIP);
    assert_eq!(regs.rflags, REQUIRED_RFLAGS);
    assert_eq!(fpu.xmm[0][0], EXTENDED_STATE_MARKER);
    assert_eq!(events.nmi.pending, EVENT_SET);
    assert_eq!(events.nmi.masked, EVENT_SET);
    assert_eq!(mp_state.mp_state, KVM_MP_STATE_RUNNABLE);
}

struct SourceVm {
    vm: VmFd,
    vcpu: VcpuFd,
    msr_indices: Vec<u32>,
}

impl SourceVm {
    fn new() -> Self {
        let kvm = Kvm::new().expect("open KVM");
        let mut msr_indices = kvm
            .get_msr_index_list()
            .expect("MSR inventory")
            .as_slice()
            .to_vec();
        msr_indices.sort_unstable();
        msr_indices.dedup();
        let vm = kvm.create_vm().expect("source VM");
        vm.create_irq_chip().expect("source IRQ chip");
        vm.create_pit2(kvm_pit_config::default())
            .expect("source PIT");
        let vcpu = vm.create_vcpu(0).expect("source vCPU");
        let mut regs = vcpu.get_regs().expect("source registers");
        regs.rip = RESTORED_RIP;
        regs.rflags = REQUIRED_RFLAGS;
        vcpu.set_regs(&regs).expect("stage source registers");
        let mut fpu = vcpu.get_fpu().expect("source FPU");
        fpu.xmm[0][0] = EXTENDED_STATE_MARKER;
        vcpu.set_fpu(&fpu).expect("stage source FPU");
        let mut events = vcpu.get_vcpu_events().expect("source events");
        events.nmi.pending = EVENT_SET;
        events.nmi.masked = EVENT_SET;
        vcpu.set_vcpu_events(&events).expect("stage source events");
        vcpu.set_mp_state(kvm_mp_state {
            mp_state: KVM_MP_STATE_RUNNABLE,
        })
        .expect("stage source MP state");
        Self {
            vm,
            vcpu,
            msr_indices,
        }
    }

    fn vcpu_snapshot(&self) -> VcpuSnapshot {
        VcpuSnapshot::capture(&self.vcpu, &self.msr_indices).expect("capture source vCPU")
    }

    fn complete_snapshot(&self) -> VmSnapshot {
        let mut snapshot = synthetic_snapshot();
        snapshot.vcpu_snapshots = vec![self.vcpu_snapshot()];
        snapshot.pic_master = self.irqchip(KVM_IRQCHIP_PIC_MASTER);
        snapshot.pic_slave = self.irqchip(KVM_IRQCHIP_PIC_SLAVE);
        snapshot.ioapic = self.irqchip(KVM_IRQCHIP_IOAPIC);
        snapshot.pit = self.vm.get_pit2().expect("source PIT state");
        snapshot.clock = self.vm.get_clock().expect("source KVM clock");
        replace_snapshot_topology(&mut snapshot, self.msr_indices.clone());
        snapshot
    }

    fn irqchip(&self, chip_id: u32) -> kvm_irqchip {
        let mut state = kvm_irqchip {
            chip_id,
            ..kvm_irqchip::default()
        };
        self.vm.get_irqchip(&mut state).expect("source IRQ state");
        state
    }
}
