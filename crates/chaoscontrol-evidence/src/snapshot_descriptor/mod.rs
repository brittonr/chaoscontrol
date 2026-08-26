mod closure;
pub mod contracts;
pub mod fixture;

use std::path::Path;

use chaoscontrol_snapshot_descriptor as snapshot_core;
use serde::{Deserialize, Serialize};

use crate::{EvidenceError, EvidenceResult};

pub use closure::{chunked_closure_from_manifest, monolithic_closure_from_file};

const SCHEDULER_PROFILE: &str = "exact-single-step-v1";
const TIME_PROFILE: &str = "virtual-tsc-v1";
const ENTROPY_PROFILE: &str = "seeded-chacha20-v1";
const CONTINUATION_STEPS: u64 = 64;

#[derive(Clone, Debug)]
pub struct DescriptorBuildInput {
    pub memory_bytes: u64,
    pub runtime_build: snapshot_core::TaggedDigest,
    pub guest_artifacts: Vec<snapshot_core::GuestArtifact>,
    pub payload: snapshot_core::PayloadClosure,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DescriptorFixtureBundle {
    pub monolithic_descriptor: snapshot_core::TaggedDigest,
    pub chunked_descriptor: snapshot_core::TaggedDigest,
    pub preflight: snapshot_core::TaggedDigest,
    pub restore_completed: bool,
    pub consumer_claim_count: usize,
}

pub fn descriptor_from_metadata(
    metadata: &chaoscontrol_vmm::snapshot::SnapshotMetadata,
    mut input: DescriptorBuildInput,
) -> EvidenceResult<snapshot_core::SnapshotDescriptor> {
    chaoscontrol_vmm::snapshot::validate_snapshot_metadata(Some(metadata), &metadata.topology)
        .map_err(|error| {
            EvidenceError::new(format!(
                "snapshot metadata is not exact and complete: {error:?}"
            ))
        })?;
    let mut devices = metadata
        .topology
        .virtio_devices
        .iter()
        .map(|(identity, queue_count)| {
            Ok(snapshot_core::DeviceCohort {
                identity: snapshot_core::DeviceIdentity {
                    base_address: identity.base_addr,
                    irq: identity.irq,
                    device_id: identity.device_id,
                },
                queue_count: *queue_count,
                backend: backend_for_device(identity.device_id)?,
            })
        })
        .collect::<EvidenceResult<Vec<_>>>()?;
    devices.sort();
    let topology = snapshot_core::SnapshotTopology {
        vcpu_count: metadata.topology.vcpu_count,
        memory_bytes: input.memory_bytes,
        msr_indices: metadata.topology.msr_indices.clone(),
        devices,
    };
    input.guest_artifacts.sort();
    let descriptor = snapshot_core::SnapshotDescriptor {
        schema: snapshot_core::DESCRIPTOR_SCHEMA.to_string(),
        descriptor_version: snapshot_core::DESCRIPTOR_VERSION,
        completeness_profile: metadata.completeness_profile.clone(),
        state_schema_version: metadata.state_schema_version,
        architecture: snapshot_core::EXACT_ARCHITECTURE.to_string(),
        runtime: snapshot_core::RuntimeCohort {
            runtime_build: input.runtime_build,
            kvm_operations: snapshot_core::REQUIRED_KVM_OPERATIONS
                .map(str::to_string)
                .to_vec(),
            scheduler_profile: SCHEDULER_PROFILE.to_string(),
            time_profile: TIME_PROFILE.to_string(),
            entropy_profile: ENTROPY_PROFILE.to_string(),
        },
        state_owners: snapshot_core::expected_state_owners(&topology),
        topology,
        guest_artifacts: input.guest_artifacts,
        payload: input.payload,
    };
    snapshot_core::validate_descriptor(&descriptor)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    Ok(descriptor)
}

pub fn envelope(
    descriptor: snapshot_core::SnapshotDescriptor,
) -> EvidenceResult<snapshot_core::SnapshotDescriptorEnvelope> {
    let descriptor_id = snapshot_core::descriptor_identity(&descriptor)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    Ok(snapshot_core::SnapshotDescriptorEnvelope {
        descriptor_id,
        descriptor,
    })
}

pub fn write_envelope(
    path: impl AsRef<Path>,
    envelope: &snapshot_core::SnapshotDescriptorEnvelope,
) -> EvidenceResult<()> {
    let expected = snapshot_core::descriptor_identity(&envelope.descriptor)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    if expected != envelope.descriptor_id {
        return Err(EvidenceError::new(
            "snapshot descriptor envelope identity mismatch",
        ));
    }
    let bytes = serde_json::to_vec_pretty(envelope)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn read_envelope(
    path: impl AsRef<Path>,
) -> EvidenceResult<snapshot_core::SnapshotDescriptorEnvelope> {
    let bytes = std::fs::read(path)?;
    let envelope: snapshot_core::SnapshotDescriptorEnvelope = serde_json::from_slice(&bytes)?;
    let expected = snapshot_core::descriptor_identity(&envelope.descriptor)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    if expected != envelope.descriptor_id {
        return Err(EvidenceError::new(
            "snapshot descriptor read-back identity mismatch",
        ));
    }
    Ok(envelope)
}

pub fn matching_destination(
    descriptor: &snapshot_core::SnapshotDescriptor,
    destination_id: &str,
) -> snapshot_core::DestinationObservation {
    snapshot_core::DestinationObservation {
        destination_id: destination_id.to_string(),
        completeness_profile: descriptor.completeness_profile.clone(),
        state_schema_version: descriptor.state_schema_version,
        architecture: descriptor.architecture.clone(),
        runtime: descriptor.runtime.clone(),
        topology: descriptor.topology.clone(),
        available_memory_bytes: descriptor.topology.memory_bytes,
    }
}

pub fn successful_restore_receipt(
    descriptor: &snapshot_core::SnapshotDescriptor,
    decision: &snapshot_core::PreflightDecision,
) -> EvidenceResult<snapshot_core::RestoreReceipt> {
    let plan = decision
        .plan
        .as_ref()
        .ok_or_else(|| EvidenceError::new("cannot complete a denied snapshot preflight"))?;
    let receipt = snapshot_core::RestoreReceipt {
        descriptor_id: plan.descriptor_id.clone(),
        destination_id: plan.destination_id.clone(),
        preflight_id: snapshot_core::preflight_identity(decision)
            .map_err(|error| EvidenceError::new(error.to_string()))?,
        materialized: true,
        mutation_started: true,
        phases: plan
            .phases
            .iter()
            .map(|phase| snapshot_core::RestorePhaseObservation {
                phase: *phase,
                status: snapshot_core::PhaseStatus::Succeeded,
                diagnostic: None,
            })
            .collect(),
        poisoned: false,
        completed: true,
        continuation: Some(snapshot_core::ContinuationObservation {
            checked_steps: CONTINUATION_STEPS,
            deterministic_trace_matches: true,
        }),
        non_claims: snapshot_core::RESTORE_NON_CLAIMS
            .map(str::to_string)
            .to_vec(),
    };
    let expected_descriptor = snapshot_core::descriptor_identity(descriptor)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    if receipt.descriptor_id != expected_descriptor {
        return Err(EvidenceError::new(
            "restore receipt descriptor identity mismatch",
        ));
    }
    snapshot_core::validate_restore_receipt(&receipt)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    Ok(receipt)
}

pub fn consumer_reference(
    descriptor: &snapshot_core::SnapshotDescriptor,
    decision: &snapshot_core::PreflightDecision,
) -> EvidenceResult<snapshot_core::ConsumerSnapshotReference> {
    let mut closure_members = descriptor
        .payload
        .members
        .iter()
        .map(|member| member.content.clone())
        .collect::<Vec<_>>();
    closure_members.sort();
    let reference = snapshot_core::ConsumerSnapshotReference {
        descriptor_id: snapshot_core::descriptor_identity(descriptor)
            .map_err(|error| EvidenceError::new(error.to_string()))?,
        completeness_profile: descriptor.completeness_profile.clone(),
        logical_payload: descriptor.payload.logical_payload.clone(),
        closure_members,
        preflight_id: snapshot_core::preflight_identity(decision)
            .map_err(|error| EvidenceError::new(error.to_string()))?,
        disallowed_claims: Vec::new(),
    };
    snapshot_core::validate_consumer_reference(&reference)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    Ok(reference)
}

fn backend_for_device(device_id: u32) -> EvidenceResult<snapshot_core::DeviceBackend> {
    match device_id {
        snapshot_core::VIRTIO_NETWORK_DEVICE_ID => Ok(snapshot_core::DeviceBackend::Network),
        snapshot_core::VIRTIO_BLOCK_DEVICE_ID => Ok(snapshot_core::DeviceBackend::Block),
        snapshot_core::VIRTIO_ENTROPY_DEVICE_ID => Ok(snapshot_core::DeviceBackend::Entropy),
        _ => Err(EvidenceError::new(format!(
            "unsupported exact-snapshot virtio device id {device_id}"
        ))),
    }
}
