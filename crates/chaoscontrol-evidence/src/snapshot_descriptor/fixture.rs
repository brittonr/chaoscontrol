use chaoscontrol_snapshot_descriptor as snapshot_core;

use crate::snapshot_descriptor::{
    chunked_closure_from_manifest, consumer_reference, descriptor_from_metadata, envelope,
    matching_destination, monolithic_closure_from_file, read_envelope, successful_restore_receipt,
    write_envelope, DescriptorBuildInput, DescriptorFixtureBundle,
};
use crate::{EvidenceError, EvidenceResult};

const MIB_BYTES: u64 = 1024 * 1024;
const FIXTURE_MEMORY_MIB: u64 = 128;
const FIXTURE_MEMORY_BYTES: u64 = FIXTURE_MEMORY_MIB * MIB_BYTES;
const MAX_FIXTURE_PAYLOAD_BYTES: u64 = MIB_BYTES;
const FIXTURE_VCPU_COUNT: u32 = 2;
const FIRST_MSR_INDEX: u32 = 0x10;
const SECOND_MSR_INDEX: u32 = 0x1b;
const BLOCK_BASE: u64 = 0x1000;
const NETWORK_BASE: u64 = 0x2000;
const ENTROPY_BASE: u64 = 0x3000;
const BLOCK_IRQ: u32 = 5;
const NETWORK_IRQ: u32 = 6;
const ENTROPY_IRQ: u32 = 7;
const DEVICE_QUEUE_COUNT: u32 = 1;
const FIXTURE_PAYLOAD: &[u8] = b"chaoscontrol-exact-snapshot-payload-v1";
const FIXTURE_RUNTIME: &[u8] = b"chaoscontrol-runtime-build-fixture-v1";
const FIXTURE_KERNEL: &[u8] = b"chaoscontrol-kernel-fixture-v1";
const FIXTURE_INITRD: &[u8] = b"chaoscontrol-initrd-fixture-v1";

const MONOLITHIC_PAYLOAD_FILE: &str = "snapshot.payload.bin";
const MONOLITHIC_DESCRIPTOR_FILE: &str = "snapshot-descriptor.monolithic.json";
const CHUNKED_DESCRIPTOR_FILE: &str = "snapshot-descriptor.chunked.json";
const DESTINATION_FILE: &str = "destination-observation.json";
const PREFLIGHT_FILE: &str = "snapshot-preflight.json";
const RESTORE_RECEIPT_FILE: &str = "snapshot-restore-receipt.json";
const LOCATOR_FILE: &str = "snapshot-locator-sidecar.json";
const CONSUMER_FILE: &str = "molten-shaped-snapshot-reference.json";
const BUNDLE_FILE: &str = "snapshot-descriptor-fixture-bundle.json";

pub fn example_descriptor() -> EvidenceResult<snapshot_core::SnapshotDescriptor> {
    let logical_payload = content(
        FIXTURE_PAYLOAD,
        snapshot_core::DigestAlgorithm::Sha256,
        snapshot_core::CURRENT_PAYLOAD_CODEC,
    )?;
    let payload = snapshot_core::PayloadClosure {
        kind: snapshot_core::ClosureKind::Monolithic,
        logical_payload: logical_payload.clone(),
        manifest: None,
        members: vec![snapshot_core::ClosureMember {
            order: 0,
            role: snapshot_core::ClosureRole::SnapshotPayload,
            content: logical_payload,
        }],
    };
    descriptor_from_metadata(
        &example_metadata(),
        DescriptorBuildInput {
            memory_bytes: FIXTURE_MEMORY_BYTES,
            runtime_build: snapshot_core::digest_bytes(
                snapshot_core::DigestAlgorithm::Blake3,
                FIXTURE_RUNTIME,
            ),
            guest_artifacts: example_guest_artifacts()?,
            payload,
        },
    )
}

pub fn write_fixture_bundle(
    root: impl AsRef<std::path::Path>,
) -> EvidenceResult<DescriptorFixtureBundle> {
    let root = root.as_ref();
    std::fs::create_dir_all(root)?;
    let payload_path = root.join(MONOLITHIC_PAYLOAD_FILE);
    std::fs::write(&payload_path, FIXTURE_PAYLOAD)?;
    let monolithic_payload =
        monolithic_closure_from_file(&payload_path, MAX_FIXTURE_PAYLOAD_BYTES)?;
    let monolithic =
        descriptor_from_metadata(&example_metadata(), build_input(monolithic_payload)?)?;
    let monolithic_envelope = envelope(monolithic.clone())?;
    let monolithic_path = root.join(MONOLITHIC_DESCRIPTOR_FILE);
    write_envelope(&monolithic_path, &monolithic_envelope)?;
    let read_back = read_envelope(&monolithic_path)?;
    if read_back != monolithic_envelope {
        return Err(EvidenceError::new(
            "monolithic descriptor read-back changed fields",
        ));
    }

    let manifest_path = crate::write_snapshot_chunk_fixture(root)?;
    let chunked_payload =
        chunked_closure_from_manifest(&manifest_path, root, MAX_FIXTURE_PAYLOAD_BYTES)?;
    let chunked = descriptor_from_metadata(&example_metadata(), build_input(chunked_payload)?)?;
    let chunked_envelope = envelope(chunked)?;
    write_envelope(root.join(CHUNKED_DESCRIPTOR_FILE), &chunked_envelope)?;

    let destination = matching_destination(&monolithic, "fixture-destination");
    let decision = snapshot_core::preflight(&monolithic, &destination)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    let receipt = successful_restore_receipt(&monolithic, &decision)?;
    let locator = snapshot_core::LocatorSidecar {
        descriptor_id: monolithic_envelope.descriptor_id.clone(),
        hints: vec![snapshot_core::LocatorHint {
            kind: snapshot_core::LocatorKind::File,
            locator: MONOLITHIC_PAYLOAD_FILE.to_string(),
        }],
    };
    snapshot_core::validate_locator_sidecar(&locator)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    let consumer = consumer_reference(&monolithic, &decision)?;
    write_json(root.join(DESTINATION_FILE), &destination)?;
    write_json(root.join(PREFLIGHT_FILE), &decision)?;
    write_json(root.join(RESTORE_RECEIPT_FILE), &receipt)?;
    write_json(root.join(LOCATOR_FILE), &locator)?;
    write_json(root.join(CONSUMER_FILE), &consumer)?;

    let bundle = DescriptorFixtureBundle {
        monolithic_descriptor: monolithic_envelope.descriptor_id,
        chunked_descriptor: chunked_envelope.descriptor_id,
        preflight: snapshot_core::preflight_identity(&decision)
            .map_err(|error| EvidenceError::new(error.to_string()))?,
        restore_completed: receipt.completed,
        consumer_claim_count: consumer.disallowed_claims.len(),
    };
    write_json(root.join(BUNDLE_FILE), &bundle)?;
    Ok(bundle)
}

pub fn fixture_paths(root: impl AsRef<std::path::Path>) -> Vec<std::path::PathBuf> {
    let root = root.as_ref();
    [
        MONOLITHIC_DESCRIPTOR_FILE,
        CHUNKED_DESCRIPTOR_FILE,
        DESTINATION_FILE,
        PREFLIGHT_FILE,
        RESTORE_RECEIPT_FILE,
        LOCATOR_FILE,
        CONSUMER_FILE,
        BUNDLE_FILE,
    ]
    .iter()
    .map(|name| root.join(name))
    .collect()
}

fn build_input(payload: snapshot_core::PayloadClosure) -> EvidenceResult<DescriptorBuildInput> {
    Ok(DescriptorBuildInput {
        memory_bytes: FIXTURE_MEMORY_BYTES,
        runtime_build: snapshot_core::digest_bytes(
            snapshot_core::DigestAlgorithm::Blake3,
            FIXTURE_RUNTIME,
        ),
        guest_artifacts: example_guest_artifacts()?,
        payload,
    })
}

fn example_metadata() -> chaoscontrol_vmm::snapshot::SnapshotMetadata {
    let topology = chaoscontrol_vmm::snapshot::SnapshotTopology {
        vcpu_count: FIXTURE_VCPU_COUNT,
        msr_indices: vec![FIRST_MSR_INDEX, SECOND_MSR_INDEX],
        virtio_devices: vec![
            (
                device(BLOCK_BASE, BLOCK_IRQ, snapshot_core::VIRTIO_BLOCK_DEVICE_ID),
                DEVICE_QUEUE_COUNT,
            ),
            (
                device(
                    NETWORK_BASE,
                    NETWORK_IRQ,
                    snapshot_core::VIRTIO_NETWORK_DEVICE_ID,
                ),
                DEVICE_QUEUE_COUNT,
            ),
            (
                device(
                    ENTROPY_BASE,
                    ENTROPY_IRQ,
                    snapshot_core::VIRTIO_ENTROPY_DEVICE_ID,
                ),
                DEVICE_QUEUE_COUNT,
            ),
        ],
    };
    chaoscontrol_vmm::snapshot::SnapshotMetadata {
        state_schema_version: chaoscontrol_vmm::snapshot::SNAPSHOT_STATE_SCHEMA_VERSION,
        completeness_profile: chaoscontrol_vmm::snapshot::SNAPSHOT_PROFILE_EXACT_X86_KVM_V1
            .to_string(),
        inventory: chaoscontrol_vmm::snapshot::build_snapshot_inventory(&topology),
        topology,
    }
}

fn device(
    base_addr: u64,
    irq: u32,
    device_id: u32,
) -> chaoscontrol_vmm::snapshot::VirtioDeviceIdentity {
    chaoscontrol_vmm::snapshot::VirtioDeviceIdentity {
        base_addr,
        irq,
        device_id,
    }
}

fn example_guest_artifacts() -> EvidenceResult<Vec<snapshot_core::GuestArtifact>> {
    Ok(vec![
        snapshot_core::GuestArtifact {
            role: snapshot_core::GuestArtifactRole::Kernel,
            content: content(
                FIXTURE_KERNEL,
                snapshot_core::DigestAlgorithm::Blake3,
                "linux-kernel-image-v1",
            )?,
        },
        snapshot_core::GuestArtifact {
            role: snapshot_core::GuestArtifactRole::Initrd,
            content: content(
                FIXTURE_INITRD,
                snapshot_core::DigestAlgorithm::Blake3,
                "linux-initrd-v1",
            )?,
        },
    ])
}

fn content(
    bytes: &[u8],
    algorithm: snapshot_core::DigestAlgorithm,
    codec: &str,
) -> EvidenceResult<snapshot_core::ContentIdentity> {
    Ok(snapshot_core::ContentIdentity {
        digest: snapshot_core::digest_bytes(algorithm, bytes),
        length_bytes: u64::try_from(bytes.len())
            .map_err(|_| EvidenceError::new("fixture content length exceeds u64"))?,
        codec: codec.to_string(),
    })
}

fn write_json(
    path: impl AsRef<std::path::Path>,
    value: &impl serde::Serialize,
) -> EvidenceResult<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
