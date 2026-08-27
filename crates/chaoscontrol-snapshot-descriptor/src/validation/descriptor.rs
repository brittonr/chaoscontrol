#![allow(
    non_trait_imports,
    reason = "complete descriptor validation must compare every named model field against one closed cohort contract"
)]
#![allow(
    path_segment_repetition,
    reason = "qualified validation helpers preserve the distinction between descriptor and observation validation"
)]

mod closure;

pub use closure::validate_payload_closure;

use std::collections::BTreeSet;

use crate::model::{
    ContentIdentity, DeviceBackend, DeviceCohort, DigestAlgorithm, GuestArtifactRole,
    SnapshotDescriptor, SnapshotTopology, StateOwner, TaggedDigest, DESCRIPTOR_SCHEMA,
    DESCRIPTOR_VERSION, EXACT_ARCHITECTURE, EXACT_SNAPSHOT_PROFILE, EXACT_STATE_SCHEMA_VERSION,
    FIXED_STATE_OWNERS, MAX_DEVICES, MAX_DEVICE_QUEUES, MAX_GUEST_ARTIFACTS, MAX_KVM_OPERATIONS,
    MAX_MEMORY_BYTES, MAX_MSR_INDICES, MAX_STATE_OWNERS, MAX_TEXT_BYTES, MAX_VCPUS,
    REQUIRED_KVM_OPERATIONS, STATE_OWNER_SCHEMA, VIRTIO_BLOCK_DEVICE_ID, VIRTIO_ENTROPY_DEVICE_ID,
    VIRTIO_NETWORK_DEVICE_ID,
};
use crate::validation::DescriptorError;

const DIGEST_HEX_BYTES: usize = 64;
const DEVICE_ADDRESS_HEX_WIDTH: usize = 16;

// r[impl chaoscontrol.snapshot_descriptor.complete_cohort]
pub fn validate_descriptor(descriptor: &SnapshotDescriptor) -> Result<(), DescriptorError> {
    require_equal("schema", &descriptor.schema, DESCRIPTOR_SCHEMA)?;
    if descriptor.descriptor_version != DESCRIPTOR_VERSION {
        return invalid("descriptor-version", "unsupported descriptor version");
    }
    require_equal(
        "completeness-profile",
        &descriptor.completeness_profile,
        EXACT_SNAPSHOT_PROFILE,
    )?;
    if descriptor.state_schema_version != EXACT_STATE_SCHEMA_VERSION {
        return invalid("state-schema-version", "unsupported snapshot state schema");
    }
    require_equal("architecture", &descriptor.architecture, EXACT_ARCHITECTURE)?;
    validate_runtime(descriptor)?;
    validate_topology(&descriptor.topology)?;
    validate_state_owners(descriptor)?;
    validate_guest_artifacts(descriptor)?;
    validate_payload_closure(&descriptor.payload)
}

pub fn expected_state_owners(topology: &SnapshotTopology) -> Vec<StateOwner> {
    let mut owners = BTreeSet::new();
    for owner in FIXED_STATE_OWNERS {
        owners.insert(owner.to_string());
    }
    for vcpu_id in 0..topology.vcpu_count {
        for component in ["architecture", "events", "msrs", "xsave"] {
            owners.insert(format!("vcpu:{vcpu_id}:{component}"));
        }
    }
    for device in &topology.devices {
        let identity = &device.identity;
        let prefix = format!(
            "virtio:{:0width$x}:{}:{}",
            identity.base_address,
            identity.irq,
            identity.device_id,
            width = DEVICE_ADDRESS_HEX_WIDTH,
        );
        owners.insert(format!("{prefix}:transport"));
        owners.insert(format!("{prefix}:backend:{:?}", device.backend).to_ascii_lowercase());
        for queue_index in 0..device.queue_count {
            owners.insert(format!("{prefix}:queue:{queue_index}"));
        }
    }
    owners
        .into_iter()
        .map(|owner_id| StateOwner {
            owner_id,
            schema_id: STATE_OWNER_SCHEMA.to_string(),
        })
        .collect()
}

pub(crate) fn validate_topology(topology: &SnapshotTopology) -> Result<(), DescriptorError> {
    if topology.vcpu_count == 0 || topology.vcpu_count > MAX_VCPUS {
        return invalid("vcpu-count", "vCPU count is outside the public bound");
    }
    if topology.memory_bytes == 0 || topology.memory_bytes > MAX_MEMORY_BYTES {
        return invalid("memory-bytes", "memory size is outside the public bound");
    }
    if topology.msr_indices.is_empty() || topology.msr_indices.len() > MAX_MSR_INDICES {
        return invalid("msr-inventory", "MSR inventory is empty or too large");
    }
    if topology
        .msr_indices
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
    {
        return invalid("msr-inventory", "MSR inventory is not strictly ordered");
    }
    if topology.devices.len() > MAX_DEVICES
        || topology.devices.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return invalid(
            "device-inventory",
            "device inventory is oversized or unordered",
        );
    }
    for device in &topology.devices {
        validate_device(device)?;
    }
    Ok(())
}

pub(crate) fn validate_digest(digest: &TaggedDigest) -> Result<(), DescriptorError> {
    if digest.hex.len() != DIGEST_HEX_BYTES
        || !digest
            .hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return invalid(
            "digest",
            "digest must be 64 lowercase hexadecimal characters",
        );
    }
    match digest.algorithm {
        DigestAlgorithm::Blake3 | DigestAlgorithm::Sha256 => Ok(()),
    }
}

pub(crate) fn validate_content(content: &ContentIdentity) -> Result<(), DescriptorError> {
    validate_digest(&content.digest)?;
    if content.length_bytes == 0 {
        return invalid("content-length", "content length must be positive");
    }
    validate_text("content-codec", &content.codec)
}

pub(crate) fn validate_text(field: &'static str, text: &str) -> Result<(), DescriptorError> {
    if text.is_empty() || text.len() > MAX_TEXT_BYTES || text.chars().any(char::is_control) {
        return invalid(field, "text is empty, oversized, or contains controls");
    }
    Ok(())
}

fn validate_runtime(descriptor: &SnapshotDescriptor) -> Result<(), DescriptorError> {
    let runtime = &descriptor.runtime;
    validate_digest(&runtime.runtime_build)?;
    if runtime.kvm_operations.len() > MAX_KVM_OPERATIONS
        || runtime
            .kvm_operations
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            != REQUIRED_KVM_OPERATIONS
    {
        return invalid("kvm-operations", "KVM operation cohort is not exact");
    }
    validate_text("scheduler-profile", &runtime.scheduler_profile)?;
    validate_text("time-profile", &runtime.time_profile)?;
    validate_text("entropy-profile", &runtime.entropy_profile)
}

fn validate_state_owners(descriptor: &SnapshotDescriptor) -> Result<(), DescriptorError> {
    if descriptor.state_owners.is_empty() || descriptor.state_owners.len() > MAX_STATE_OWNERS {
        return invalid(
            "state-owners",
            "state-owner inventory is empty or too large",
        );
    }
    for owner in &descriptor.state_owners {
        validate_text("state-owner-id", &owner.owner_id)?;
        require_equal("state-owner-schema", &owner.schema_id, STATE_OWNER_SCHEMA)?;
    }
    if descriptor.state_owners != expected_state_owners(&descriptor.topology) {
        return invalid(
            "state-owners",
            "state-owner inventory is incomplete or reordered",
        );
    }
    Ok(())
}

fn validate_guest_artifacts(descriptor: &SnapshotDescriptor) -> Result<(), DescriptorError> {
    let artifacts = &descriptor.guest_artifacts;
    if artifacts.is_empty()
        || artifacts.len() > MAX_GUEST_ARTIFACTS
        || artifacts.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return invalid(
            "guest-artifacts",
            "guest artifact inventory is incomplete or unordered",
        );
    }
    if !artifacts
        .iter()
        .any(|artifact| artifact.role == GuestArtifactRole::Kernel)
    {
        return invalid("guest-artifacts", "kernel artifact is required");
    }
    for artifact in artifacts {
        validate_content(&artifact.content)?;
    }
    Ok(())
}

fn validate_device(device: &DeviceCohort) -> Result<(), DescriptorError> {
    if device.queue_count == 0 || device.queue_count > MAX_DEVICE_QUEUES {
        return invalid(
            "device-queues",
            "device queue count is outside the public bound",
        );
    }
    let expected = match device.identity.device_id {
        VIRTIO_NETWORK_DEVICE_ID => DeviceBackend::Network,
        VIRTIO_BLOCK_DEVICE_ID => DeviceBackend::Block,
        VIRTIO_ENTROPY_DEVICE_ID => DeviceBackend::Entropy,
        _ => return invalid("device-id", "unsupported virtio device identity"),
    };
    if device.backend != expected {
        return invalid("device-backend", "device identity and backend class differ");
    }
    Ok(())
}

fn require_equal(field: &'static str, actual: &str, expected: &str) -> Result<(), DescriptorError> {
    if actual == expected {
        Ok(())
    } else {
        invalid(field, format!("expected {expected}, found {actual}"))
    }
}

fn invalid<T>(code: &'static str, detail: impl Into<String>) -> Result<T, DescriptorError> {
    Err(DescriptorError::new(code, detail.into()))
}
