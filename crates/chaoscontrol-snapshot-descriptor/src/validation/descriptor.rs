use std::collections::BTreeSet;

use crate::model::{
    ClosureKind, ClosureRole, ContentIdentity, DeviceBackend, DeviceCohort, DigestAlgorithm,
    GuestArtifactRole, PayloadClosure, SnapshotDescriptor, SnapshotTopology, StateOwner,
    TaggedDigest, CHUNK_MANIFEST_CODEC, CURRENT_PAYLOAD_CODEC, DESCRIPTOR_SCHEMA,
    DESCRIPTOR_VERSION, EXACT_ARCHITECTURE, EXACT_SNAPSHOT_PROFILE, EXACT_STATE_SCHEMA_VERSION,
    FIXED_STATE_OWNERS, MAX_CLOSURE_MEMBERS, MAX_DEVICES, MAX_DEVICE_QUEUES, MAX_GUEST_ARTIFACTS,
    MAX_KVM_OPERATIONS, MAX_MEMORY_BYTES, MAX_MSR_INDICES, MAX_STATE_OWNERS, MAX_TEXT_BYTES,
    MAX_VCPUS, REQUIRED_KVM_OPERATIONS, SNAPSHOT_CHUNK_CODEC, STATE_OWNER_SCHEMA,
    VIRTIO_BLOCK_DEVICE_ID, VIRTIO_ENTROPY_DEVICE_ID, VIRTIO_NETWORK_DEVICE_ID,
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

// r[impl chaoscontrol.snapshot_descriptor.closure]
pub fn validate_payload_closure(closure: &PayloadClosure) -> Result<(), DescriptorError> {
    validate_content(&closure.logical_payload)?;
    require_equal(
        "payload-codec",
        &closure.logical_payload.codec,
        CURRENT_PAYLOAD_CODEC,
    )?;
    if closure.members.is_empty() || closure.members.len() > MAX_CLOSURE_MEMBERS {
        return invalid("closure-members", "closure is empty or too large");
    }
    match closure.kind {
        ClosureKind::Monolithic => validate_monolithic(closure),
        ClosureKind::Chunked => validate_chunked(closure),
    }
}

fn validate_monolithic(closure: &PayloadClosure) -> Result<(), DescriptorError> {
    if closure.manifest.is_some() || closure.members.len() != 1 {
        return invalid(
            "monolithic-closure",
            "monolithic closure must contain one payload member",
        );
    }
    let member = &closure.members[0];
    if member.order != 0
        || member.role != ClosureRole::SnapshotPayload
        || member.content != closure.logical_payload
    {
        return invalid(
            "monolithic-closure",
            "monolithic payload member does not match logical payload",
        );
    }
    Ok(())
}

fn validate_chunked(closure: &PayloadClosure) -> Result<(), DescriptorError> {
    let manifest = closure.manifest.as_ref().ok_or_else(|| {
        DescriptorError::new("chunk-manifest", "chunk manifest is missing".into())
    })?;
    validate_content(manifest)?;
    require_equal(
        "chunk-manifest-codec",
        &manifest.codec,
        CHUNK_MANIFEST_CODEC,
    )?;
    let mut total = 0_u64;
    for (index, member) in closure.members.iter().enumerate() {
        let order = u32::try_from(index)
            .map_err(|_| DescriptorError::new("chunk-order", "chunk order exceeds u32".into()))?;
        if member.order != order || member.role != ClosureRole::SnapshotChunk {
            return invalid(
                "chunk-order",
                "chunk members are missing, reordered, or have the wrong role",
            );
        }
        validate_content(&member.content)?;
        require_equal("chunk-codec", &member.content.codec, SNAPSHOT_CHUNK_CODEC)?;
        total = total
            .checked_add(member.content.length_bytes)
            .ok_or_else(|| {
                DescriptorError::new("chunk-length", "chunk length sum overflowed".into())
            })?;
    }
    if total != closure.logical_payload.length_bytes {
        return invalid(
            "chunk-length",
            "chunk lengths do not cover the logical payload",
        );
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
