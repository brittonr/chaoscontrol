use std::sync::Arc;

use chaoscontrol_vmm::devices::block::DeterministicBlock;
use chaoscontrol_vmm::snapshot::{validate_snapshot_metadata, SnapshotTopology, VmSnapshot};
use vm_cohort_core::{
    admit_checkpoint, plan_cohort, CheckpointCandidate, CohortRequest, CompatibilityProfile,
    ReceiptRef, ReferenceError, ResourceRef,
};
use vm_cohort_kvm::{identify_bytes, validate_profile, KvmRuntimeProfile};

use crate::{AdapterError, ChaosCompatibilityFacts, MappedChaosCohort};

const IN_KERNEL_DEVICE_COUNT: u32 = 4;

/// Inputs for one exact ChaosControl snapshot mapping.
pub struct SnapshotCohortMappingRequest<'a> {
    /// Complete ChaosControl VM snapshot.
    pub snapshot: &'a VmSnapshot,
    /// Deterministic block state captured with the snapshot.
    pub block: &'a DeterministicBlock,
    /// ChaosControl-owned compatibility facts.
    pub facts: &'a ChaosCompatibilityFacts,
    /// Selected VM Cohort KVM profile.
    pub kvm_profile: KvmRuntimeProfile,
    /// Requested clone count.
    pub workers: u32,
    /// Product-owned context identity.
    pub context_ref: ResourceRef,
}

/// Maps one complete exact ChaosControl snapshot into VM Cohort facts.
///
/// # Errors
///
/// Returns a bounded error for profile drift, incomplete snapshot facts,
/// serialization failure, identity failure, or core denial.
// r[impl chaoscontrol.vm_cohort.adapter]
pub fn map_snapshot_cohort(
    request: SnapshotCohortMappingRequest<'_>,
) -> Result<MappedChaosCohort, AdapterError> {
    let SnapshotCohortMappingRequest {
        snapshot,
        block,
        facts,
        kvm_profile,
        workers,
        context_ref,
    } = request;
    validate_mapping_facts(snapshot, facts, &kvm_profile)?;
    let (memory, disk) = materialize_bases(snapshot, block);
    let memory_ref =
        identify_bytes(&memory).map_err(|_| AdapterError::Admission("memory base identity"))?;
    let disk_ref =
        identify_bytes(&disk).map_err(|_| AdapterError::Admission("disk base identity"))?;
    let snapshot_bytes = serde_json::to_vec(snapshot)?;
    let vcpu_bytes = serde_json::to_vec(&snapshot.vcpu_snapshots)?;
    let memory_layout_bytes = serde_json::to_vec(&(
        snapshot.memory.memory_size(),
        kvm_profile.guest_physical_address,
        kvm_profile.memory_slot,
    ))?;
    let device_count = u32::try_from(snapshot.virtio_snapshots.len())
        .map_err(|_| AdapterError::Admission("virtio device count exceeds u32"))?
        .checked_add(IN_KERNEL_DEVICE_COUNT)
        .ok_or(AdapterError::Admission("device count overflow"))?;
    let compatibility = CompatibilityProfile {
        profile_ref: facts.profile_ref.clone(),
        architecture: kvm_profile.architecture.clone(),
        vcpu_state_ref: resource_from_bytes(&vcpu_bytes)?,
        memory_layout_ref: resource_from_bytes(&memory_layout_bytes)?,
        kernel_ref: facts.kernel_ref.clone(),
        guest_image_ref: facts.guest_image_ref.clone(),
        device_model_ref: resource_from_bytes(&snapshot_bytes)?,
        disk_format_ref: facts.disk_format_ref.clone(),
        runtime_ref: facts.runtime_ref.clone(),
        adapter_ref: facts.adapter_ref.clone(),
    };
    let checkpoint = admit_checkpoint(&CheckpointCandidate {
        compatibility: compatibility.clone(),
        effective_memory_base_ref: memory_ref,
        effective_disk_base_ref: disk_ref,
        memory_bytes: u64::try_from(memory.len())
            .map_err(|_| AdapterError::Admission("memory length exceeds u64"))?,
        disk_bytes: u64::try_from(disk.len())
            .map_err(|_| AdapterError::Admission("disk length exceeds u64"))?,
        vcpu_count: u32::try_from(snapshot.vcpu_snapshots.len())
            .map_err(|_| AdapterError::Admission("vCPU count exceeds u32"))?,
        device_count,
        complete: true,
        host_handles_present: false,
        bases_mutable: false,
    })
    .map_err(|_| AdapterError::Core("checkpoint admission"))?;
    let plan = plan_cohort(&CohortRequest {
        checkpoint,
        expected_compatibility: compatibility,
        workers,
        limits: kvm_profile.limits.clone(),
        context_ref,
    })
    .map_err(|_| AdapterError::Core("cohort planning"))?;
    Ok(MappedChaosCohort {
        plan,
        kvm_profile,
        memory,
        disk,
        snapshot_ref: receipt_from_bytes(&snapshot_bytes)?,
    })
}

fn materialize_bases(snapshot: &VmSnapshot, block: &DeterministicBlock) -> (Vec<u8>, Arc<[u8]>) {
    (snapshot.memory.materialize(), block.materialize().into())
}

fn validate_mapping_facts(
    snapshot: &VmSnapshot,
    facts: &ChaosCompatibilityFacts,
    kvm_profile: &KvmRuntimeProfile,
) -> Result<(), AdapterError> {
    validate_profile(kvm_profile).map_err(|error| AdapterError::Kvm(error.to_string()))?;
    if facts.profile_ref != kvm_profile.profile_ref || facts.adapter_ref != kvm_profile.adapter_ref
    {
        return Err(AdapterError::Admission(
            "snapshot profile or adapter facts are inconsistent",
        ));
    }
    validate_exact_snapshot(snapshot)
}

fn validate_exact_snapshot(snapshot: &VmSnapshot) -> Result<(), AdapterError> {
    let topology = observed_topology(snapshot)?;
    validate_snapshot_metadata(snapshot.metadata.as_ref(), &topology)
        .map_err(|_| AdapterError::Admission("snapshot metadata is incomplete or inconsistent"))?;
    for vcpu in &snapshot.vcpu_snapshots {
        vcpu.validate_msr_inventory(&topology.msr_indices)
            .map_err(|_| AdapterError::Admission("snapshot vCPU state is incomplete"))?;
    }
    snapshot
        .scheduler_snapshot
        .validate()
        .map_err(|_| AdapterError::Admission("snapshot scheduler state is invalid"))?;
    let scheduler_vcpus = u32::try_from(snapshot.scheduler_snapshot.state.num_vcpus)
        .map_err(|_| AdapterError::Admission("snapshot scheduler vCPU count exceeds u32"))?;
    if scheduler_vcpus != topology.vcpu_count {
        return Err(AdapterError::Admission(
            "snapshot scheduler topology is inconsistent",
        ));
    }
    snapshot
        .validate_assertion_identity()
        .map_err(|_| AdapterError::Admission("snapshot assertion state is invalid"))?;
    let latch_count = u32::try_from(snapshot.hlt_latched_vcpus.len())
        .map_err(|_| AdapterError::Admission("snapshot HLT latch count exceeds u32"))?;
    if latch_count != topology.vcpu_count {
        return Err(AdapterError::Admission(
            "snapshot HLT latch topology is inconsistent",
        ));
    }
    Ok(())
}

fn observed_topology(snapshot: &VmSnapshot) -> Result<SnapshotTopology, AdapterError> {
    let metadata = snapshot
        .metadata
        .as_ref()
        .ok_or(AdapterError::Admission("snapshot metadata is missing"))?;
    let vcpu_count = u32::try_from(snapshot.vcpu_snapshots.len())
        .map_err(|_| AdapterError::Admission("snapshot vCPU count exceeds u32"))?;
    if vcpu_count == 0 {
        return Err(AdapterError::Admission("snapshot has no vCPU state"));
    }
    let mut virtio_devices = snapshot
        .virtio_snapshots
        .iter()
        .map(|device| {
            u32::try_from(device.transport.queues.len())
                .map(|queue_count| (device.identity(), queue_count))
                .map_err(|_| AdapterError::Admission("virtio queue count exceeds u32"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    virtio_devices.sort();
    Ok(SnapshotTopology {
        vcpu_count,
        msr_indices: metadata.topology.msr_indices.clone(),
        virtio_devices,
    })
}

fn resource_from_bytes(bytes: &[u8]) -> Result<ResourceRef, AdapterError> {
    ResourceRef::new(format!("blake3:{}", blake3::hash(bytes).to_hex()))
        .map_err(|ReferenceError| AdapterError::Admission("resource identity"))
}

pub(crate) fn receipt_from_bytes(bytes: &[u8]) -> Result<ReceiptRef, AdapterError> {
    ReceiptRef::new(format!("blake3:{}", blake3::hash(bytes).to_hex()))
        .map_err(|ReferenceError| AdapterError::Admission("snapshot identity"))
}
