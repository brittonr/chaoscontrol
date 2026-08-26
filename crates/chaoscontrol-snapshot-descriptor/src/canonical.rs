use sha2::Digest as _;

use crate::model::{
    ClosureKind, ClosureRole, ContentIdentity, DeviceBackend, DigestAlgorithm, GuestArtifactRole,
    PayloadClosure, RuntimeCohort, SnapshotDescriptor, SnapshotTopology, TaggedDigest,
};
use crate::observations::{
    DestinationObservation, PreflightDecision, PreflightStatus, RestorePhase,
};
use crate::validation::{validate_descriptor, DescriptorError};

const DESCRIPTOR_IDENTITY_DOMAIN: &str = "chaoscontrol.snapshot-descriptor.identity.v1";
const DESTINATION_IDENTITY_DOMAIN: &str = "chaoscontrol.snapshot-destination.identity.v1";
const PREFLIGHT_IDENTITY_DOMAIN: &str = "chaoscontrol.snapshot-preflight.identity.v1";
const HEX_ALPHABET_BYTES: usize = 16;
const HEX_CHARACTERS_PER_BYTE: usize = 2;
const HEX_HIGH_NIBBLE_SHIFT: u32 = 4;
const HEX_LOW_NIBBLE_MASK: u8 = 0x0f;
const HEX_DIGITS: &[u8; HEX_ALPHABET_BYTES] = b"0123456789abcdef";

struct Framer {
    hasher: blake3::Hasher,
}

impl Framer {
    fn new(domain: &str) -> Self {
        let mut value = Self {
            hasher: blake3::Hasher::new(),
        };
        value.write_text("domain", domain);
        value
    }

    fn write_bytes(&mut self, tag: &str, bytes: &[u8]) {
        let tag_length = usize_to_u64(tag.len());
        let value_length = usize_to_u64(bytes.len());
        self.hasher.update(&tag_length.to_le_bytes());
        self.hasher.update(tag.as_bytes());
        self.hasher.update(&value_length.to_le_bytes());
        self.hasher.update(bytes);
    }

    fn write_text(&mut self, tag: &str, text: &str) {
        self.write_bytes(tag, text.as_bytes());
    }

    fn write_u32(&mut self, tag: &str, value: u32) {
        self.write_bytes(tag, &value.to_le_bytes());
    }

    fn write_u64(&mut self, tag: &str, value: u64) {
        self.write_bytes(tag, &value.to_le_bytes());
    }

    fn write_count(&mut self, tag: &str, count: usize) -> Result<(), DescriptorError> {
        let count = u64::try_from(count)
            .map_err(|_| DescriptorError::new("count-overflow", format!("{tag} exceeds u64")))?;
        self.write_u64(tag, count);
        Ok(())
    }

    fn finish(self) -> TaggedDigest {
        TaggedDigest {
            algorithm: DigestAlgorithm::Blake3,
            hex: self.hasher.finalize().to_hex().to_string(),
        }
    }
}

pub fn digest_bytes(algorithm: DigestAlgorithm, bytes: &[u8]) -> TaggedDigest {
    let hex = match algorithm {
        DigestAlgorithm::Blake3 => blake3::hash(bytes).to_hex().to_string(),
        DigestAlgorithm::Sha256 => {
            let digest = sha2::Sha256::digest(bytes);
            hex_lower(digest.as_slice())
        }
    };
    TaggedDigest { algorithm, hex }
}

pub fn verify_content(content: &ContentIdentity, bytes: &[u8]) -> bool {
    let Ok(length_bytes) = u64::try_from(bytes.len()) else {
        return false;
    };
    length_bytes == content.length_bytes
        && digest_bytes(content.digest.algorithm, bytes) == content.digest
}

// r[impl chaoscontrol.snapshot_descriptor.contract]
pub fn descriptor_identity(
    descriptor: &SnapshotDescriptor,
) -> Result<TaggedDigest, DescriptorError> {
    validate_descriptor(descriptor)?;
    let mut framer = Framer::new(DESCRIPTOR_IDENTITY_DOMAIN);
    framer.write_text("schema", &descriptor.schema);
    framer.write_u32("descriptor-version", descriptor.descriptor_version);
    framer.write_text("completeness-profile", &descriptor.completeness_profile);
    framer.write_u32("state-schema-version", descriptor.state_schema_version);
    framer.write_text("architecture", &descriptor.architecture);
    write_runtime(&mut framer, &descriptor.runtime)?;
    write_topology(&mut framer, &descriptor.topology)?;
    framer.write_count("state-owner-count", descriptor.state_owners.len())?;
    for owner in &descriptor.state_owners {
        framer.write_text("state-owner-id", &owner.owner_id);
        framer.write_text("state-owner-schema", &owner.schema_id);
    }
    framer.write_count("guest-artifact-count", descriptor.guest_artifacts.len())?;
    for artifact in &descriptor.guest_artifacts {
        framer.write_text("guest-artifact-role", guest_role_name(artifact.role));
        write_content(&mut framer, &artifact.content);
    }
    write_closure(&mut framer, &descriptor.payload)?;
    Ok(framer.finish())
}

pub fn destination_identity(
    destination: &DestinationObservation,
) -> Result<TaggedDigest, DescriptorError> {
    let mut framer = Framer::new(DESTINATION_IDENTITY_DOMAIN);
    framer.write_text("destination-id", &destination.destination_id);
    framer.write_text("completeness-profile", &destination.completeness_profile);
    framer.write_u32("state-schema-version", destination.state_schema_version);
    framer.write_text("architecture", &destination.architecture);
    write_runtime(&mut framer, &destination.runtime)?;
    write_topology(&mut framer, &destination.topology)?;
    framer.write_u64("available-memory-bytes", destination.available_memory_bytes);
    Ok(framer.finish())
}

pub fn preflight_identity(decision: &PreflightDecision) -> Result<TaggedDigest, DescriptorError> {
    let mut framer = Framer::new(PREFLIGHT_IDENTITY_DOMAIN);
    framer.write_text("status", preflight_status_name(decision.status));
    framer.write_count("blocker-count", decision.blockers.len())?;
    for blocker in &decision.blockers {
        framer.write_text("blocker-code", &blocker.code);
        framer.write_text("blocker-expected", &blocker.expected);
        framer.write_text("blocker-observed", &blocker.observed);
    }
    match &decision.plan {
        Some(plan) => {
            framer.write_u32("plan-present", 1);
            write_digest(&mut framer, &plan.descriptor_id);
            write_digest(&mut framer, &plan.destination_id);
            framer.write_count("phase-count", plan.phases.len())?;
            for phase in &plan.phases {
                framer.write_text("phase", restore_phase_name(*phase));
            }
        }
        None => framer.write_u32("plan-present", 0),
    }
    Ok(framer.finish())
}

fn write_runtime(framer: &mut Framer, runtime: &RuntimeCohort) -> Result<(), DescriptorError> {
    write_digest(framer, &runtime.runtime_build);
    framer.write_count("kvm-operation-count", runtime.kvm_operations.len())?;
    for operation in &runtime.kvm_operations {
        framer.write_text("kvm-operation", operation);
    }
    framer.write_text("scheduler-profile", &runtime.scheduler_profile);
    framer.write_text("time-profile", &runtime.time_profile);
    framer.write_text("entropy-profile", &runtime.entropy_profile);
    Ok(())
}

fn write_topology(framer: &mut Framer, topology: &SnapshotTopology) -> Result<(), DescriptorError> {
    framer.write_u32("vcpu-count", topology.vcpu_count);
    framer.write_u64("memory-bytes", topology.memory_bytes);
    framer.write_count("msr-count", topology.msr_indices.len())?;
    for index in &topology.msr_indices {
        framer.write_u32("msr-index", *index);
    }
    framer.write_count("device-count", topology.devices.len())?;
    for device in &topology.devices {
        framer.write_u64("device-base-address", device.identity.base_address);
        framer.write_u32("device-irq", device.identity.irq);
        framer.write_u32("device-id", device.identity.device_id);
        framer.write_u32("device-queue-count", device.queue_count);
        framer.write_text("device-backend", device_backend_name(device.backend));
    }
    Ok(())
}

fn write_closure(framer: &mut Framer, closure: &PayloadClosure) -> Result<(), DescriptorError> {
    framer.write_text("closure-kind", closure_kind_name(closure.kind));
    write_content(framer, &closure.logical_payload);
    match &closure.manifest {
        Some(manifest) => {
            framer.write_u32("manifest-present", 1);
            write_content(framer, manifest);
        }
        None => framer.write_u32("manifest-present", 0),
    }
    framer.write_count("closure-member-count", closure.members.len())?;
    for member in &closure.members {
        framer.write_u32("closure-member-order", member.order);
        framer.write_text("closure-member-role", closure_role_name(member.role));
        write_content(framer, &member.content);
    }
    Ok(())
}

fn write_content(framer: &mut Framer, content: &ContentIdentity) {
    write_digest(framer, &content.digest);
    framer.write_u64("content-length", content.length_bytes);
    framer.write_text("content-codec", &content.codec);
}

fn write_digest(framer: &mut Framer, digest: &TaggedDigest) {
    framer.write_text("digest-algorithm", digest_algorithm_name(digest.algorithm));
    framer.write_text("digest-hex", &digest.hex);
}

fn usize_to_u64(value: usize) -> u64 {
    #[cfg(target_pointer_width = "64")]
    {
        u64::from_ne_bytes(value.to_ne_bytes())
    }
    #[cfg(target_pointer_width = "32")]
    {
        u64::from(u32::from_ne_bytes(value.to_ne_bytes()))
    }
    #[cfg(target_pointer_width = "16")]
    {
        u64::from(u16::from_ne_bytes(value.to_ne_bytes()))
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let capacity = bytes.len().saturating_mul(HEX_CHARACTERS_PER_BYTE);
    let mut output = String::with_capacity(capacity);
    for byte in bytes {
        output.push(char::from(
            HEX_DIGITS[usize::from(byte >> HEX_HIGH_NIBBLE_SHIFT)],
        ));
        output.push(char::from(
            HEX_DIGITS[usize::from(byte & HEX_LOW_NIBBLE_MASK)],
        ));
    }
    output
}

fn digest_algorithm_name(value: DigestAlgorithm) -> &'static str {
    match value {
        DigestAlgorithm::Blake3 => "blake3",
        DigestAlgorithm::Sha256 => "sha256",
    }
}

fn closure_kind_name(value: ClosureKind) -> &'static str {
    match value {
        ClosureKind::Monolithic => "monolithic",
        ClosureKind::Chunked => "chunked",
    }
}

fn closure_role_name(value: ClosureRole) -> &'static str {
    match value {
        ClosureRole::SnapshotPayload => "snapshot-payload",
        ClosureRole::SnapshotChunk => "snapshot-chunk",
    }
}

fn device_backend_name(value: DeviceBackend) -> &'static str {
    match value {
        DeviceBackend::Network => "network",
        DeviceBackend::Block => "block",
        DeviceBackend::Entropy => "entropy",
    }
}

fn guest_role_name(value: GuestArtifactRole) -> &'static str {
    match value {
        GuestArtifactRole::Kernel => "kernel",
        GuestArtifactRole::Initrd => "initrd",
        GuestArtifactRole::DiskImage => "disk-image",
        GuestArtifactRole::GuestBinary => "guest-binary",
    }
}

fn preflight_status_name(value: PreflightStatus) -> &'static str {
    match value {
        PreflightStatus::Admitted => "admitted",
        PreflightStatus::Denied => "denied",
    }
}

fn restore_phase_name(value: RestorePhase) -> &'static str {
    match value {
        RestorePhase::Materialize => "materialize",
        RestorePhase::Quiesce => "quiesce",
        RestorePhase::GuestMemory => "guest-memory",
        RestorePhase::IrqChip => "irq-chip",
        RestorePhase::Pit => "pit",
        RestorePhase::Clock => "clock",
        RestorePhase::Vcpu => "vcpu",
        RestorePhase::Scheduler => "scheduler",
        RestorePhase::Devices => "devices",
        RestorePhase::HostHandles => "host-handles",
        RestorePhase::Continuation => "continuation",
    }
}
