use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_sim_core::scheduler::core::ProgressMode;
use chaoscontrol_sim_core::scheduler::{SchedulerConfig, VcpuScheduler};
use chaoscontrol_vmm::devices::block::DeterministicBlock;
use chaoscontrol_vmm::devices::entropy::DeterministicEntropy;
use chaoscontrol_vmm::devices::pit::DeterministicPit;
use chaoscontrol_vmm::snapshot::{
    build_snapshot_inventory, MsrSnapshot, SnapshotMemory, SnapshotMetadata, SnapshotTopology,
    VcpuSnapshot, VmSnapshot, SNAPSHOT_PROFILE_EXACT_X86_KVM_V1, SNAPSHOT_STATE_SCHEMA_VERSION,
};
use kvm_bindings::{
    kvm_clock_data, kvm_debugregs, kvm_fpu, kvm_irqchip, kvm_lapic_state, kvm_mp_state,
    kvm_pit_state2, kvm_regs, kvm_sregs, kvm_vcpu_events, kvm_xcrs, kvm_xsave,
};
use vm_cohort_conformance::standard_fixture;
use vm_cohort_core::{ProfileRef, ResourceRef};
use vm_cohort_kvm::KvmRuntimeProfile;

use crate::{
    map_snapshot_cohort, ChaosCompatibilityFacts, MappedChaosCohort, SnapshotCohortMappingRequest,
};

pub const TEST_MSR_INDEX: u32 = 0x10;
pub const WORKER_COUNT: u32 = 2;
const MEMORY_PAGE_COUNT: usize = 2;
const DISK_PAGE_COUNT: usize = 2;
const TEST_TSC_KHZ: u32 = 3_000_000;
const ENTROPY_SEED: u64 = 17;
const MEMORY_FILL: u8 = 0x11;
const DISK_FILL: u8 = 0x22;

pub fn synthetic_snapshot() -> VmSnapshot {
    let topology = SnapshotTopology {
        vcpu_count: 1,
        msr_indices: vec![TEST_MSR_INDEX],
        virtio_devices: Vec::new(),
    };
    let scheduler = VcpuScheduler::try_new(
        &SchedulerConfig::default(),
        ProgressMode::ExactSingleStep,
        vec![true],
    )
    .expect("synthetic scheduler");
    VmSnapshot {
        metadata: Some(SnapshotMetadata {
            state_schema_version: SNAPSHOT_STATE_SCHEMA_VERSION,
            completeness_profile: SNAPSHOT_PROFILE_EXACT_X86_KVM_V1.to_string(),
            inventory: build_snapshot_inventory(&topology),
            topology,
        }),
        vcpu_snapshots: vec![synthetic_vcpu_snapshot()],
        pic_master: kvm_irqchip::default(),
        pic_slave: kvm_irqchip::default(),
        ioapic: kvm_irqchip::default(),
        pit: kvm_pit_state2::default(),
        clock: kvm_clock_data::default(),
        memory: SnapshotMemory::Full(vec![
            MEMORY_FILL;
            chaoscontrol_vmm::snapshot::PAGE_SIZE
                * MEMORY_PAGE_COUNT
        ]),
        serial_state: vm_superio::SerialState::default(),
        entropy: DeterministicEntropy::new(ENTROPY_SEED).snapshot(),
        virtual_tsc: 0,
        exit_count: 0,
        io_exit_count: 0,
        exits_since_last_sdk: 0,
        panic_detected: false,
        panic_match_state: 0,
        pit_snapshot: DeterministicPit::new(TEST_TSC_KHZ).snapshot(),
        last_kvm_pit_mode: 0,
        fault_engine_snapshot: FaultEngine::new(EngineConfig::default()).snapshot(),
        virtio_snapshots: Vec::new(),
        coverage_active: false,
        scheduler_snapshot: scheduler.snapshot(),
        hlt_latched_vcpus: vec![false],
    }
}

pub fn synthetic_block() -> DeterministicBlock {
    DeterministicBlock::from_image(vec![
        DISK_FILL;
        chaoscontrol_vmm::snapshot::PAGE_SIZE * DISK_PAGE_COUNT
    ])
}

pub fn profile() -> KvmRuntimeProfile {
    standard_fixture()
        .expect("VM Cohort standard fixture")
        .profile
}

pub fn compatibility_facts(profile: &KvmRuntimeProfile) -> ChaosCompatibilityFacts {
    ChaosCompatibilityFacts {
        profile_ref: profile.profile_ref.clone(),
        kernel_ref: resource_ref("kernel"),
        guest_image_ref: resource_ref("guest-image"),
        disk_format_ref: resource_ref("raw-disk-v1"),
        runtime_ref: resource_ref("chaoscontrol-runtime"),
        adapter_ref: profile.adapter_ref.clone(),
    }
}

pub fn mapped_fixture(workers: u32) -> (VmSnapshot, DeterministicBlock, MappedChaosCohort) {
    let snapshot = synthetic_snapshot();
    let block = synthetic_block();
    let profile = profile();
    let facts = compatibility_facts(&profile);
    let mapped = map_snapshot_cohort(SnapshotCohortMappingRequest {
        snapshot: &snapshot,
        block: &block,
        facts: &facts,
        kvm_profile: profile,
        workers,
        context_ref: resource_ref("consumer-context"),
    })
    .expect("map synthetic exact snapshot");
    (snapshot, block, mapped)
}

pub fn profile_ref(label: &str) -> ProfileRef {
    ProfileRef::new(format!(
        "blake3:{}",
        blake3::hash(label.as_bytes()).to_hex()
    ))
    .expect("profile reference")
}

pub fn resource_ref(label: &str) -> ResourceRef {
    ResourceRef::new(format!(
        "blake3:{}",
        blake3::hash(label.as_bytes()).to_hex()
    ))
    .expect("resource reference")
}

pub fn replace_snapshot_topology(snapshot: &mut VmSnapshot, msr_indices: Vec<u32>) {
    let topology = SnapshotTopology {
        vcpu_count: 1,
        msr_indices,
        virtio_devices: Vec::new(),
    };
    snapshot.metadata = Some(SnapshotMetadata {
        state_schema_version: SNAPSHOT_STATE_SCHEMA_VERSION,
        completeness_profile: SNAPSHOT_PROFILE_EXACT_X86_KVM_V1.to_string(),
        inventory: build_snapshot_inventory(&topology),
        topology,
    });
}

fn synthetic_vcpu_snapshot() -> VcpuSnapshot {
    VcpuSnapshot {
        regs: kvm_regs::default(),
        sregs: kvm_sregs::default(),
        fpu: kvm_fpu::default(),
        debug_regs: kvm_debugregs::default(),
        lapic: kvm_lapic_state::default(),
        xcrs: kvm_xcrs::default(),
        xsave: vec![0; core::mem::size_of::<kvm_xsave>()],
        events: kvm_vcpu_events::default(),
        msrs: vec![MsrSnapshot {
            index: TEST_MSR_INDEX,
            data: 0,
        }],
        mp_state: kvm_mp_state::default(),
    }
}
