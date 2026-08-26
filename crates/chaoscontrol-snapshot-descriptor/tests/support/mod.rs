#![allow(dead_code)]

use chaoscontrol_snapshot_descriptor::{
    descriptor_identity, digest_bytes, expected_state_owners, preflight, preflight_identity,
    ClosureKind, ClosureMember, ClosureRole, ConsumerSnapshotReference, ContentIdentity,
    ContinuationObservation, DestinationObservation, DeviceBackend, DeviceCohort, DeviceIdentity,
    DigestAlgorithm, GuestArtifact, GuestArtifactRole, PayloadClosure, PhaseStatus,
    RestorePhaseObservation, RestoreReceipt, RuntimeCohort, SnapshotDescriptor, SnapshotTopology,
    CHUNK_MANIFEST_CODEC, CURRENT_PAYLOAD_CODEC, DESCRIPTOR_SCHEMA, DESCRIPTOR_VERSION,
    EXACT_ARCHITECTURE, EXACT_SNAPSHOT_PROFILE, EXACT_STATE_SCHEMA_VERSION,
    REQUIRED_KVM_OPERATIONS, REQUIRED_RESTORE_PHASES, RESTORE_NON_CLAIMS, SNAPSHOT_CHUNK_CODEC,
};

const MIB_BYTES: u64 = 1024 * 1024;
const MEMORY_MIB: u64 = 128;
const MEMORY_BYTES: u64 = MEMORY_MIB * MIB_BYTES;
const AVAILABLE_MEMORY_MULTIPLIER: u64 = 2;
const FIRST_MSR_INDEX: u32 = 0x10;
const SECOND_MSR_INDEX: u32 = 0x1b;
const BLOCK_BASE: u64 = 0x1000;
const NETWORK_BASE: u64 = 0x2000;
const ENTROPY_BASE: u64 = 0x3000;
const BLOCK_IRQ: u32 = 5;
const NETWORK_IRQ: u32 = 6;
const ENTROPY_IRQ: u32 = 7;
const NETWORK_DEVICE_ID: u32 = 1;
const BLOCK_DEVICE_ID: u32 = 2;
const ENTROPY_DEVICE_ID: u32 = 4;
const DEVICE_QUEUE_COUNT: u32 = 1;
const VCPU_COUNT: u32 = 2;
const CONTINUATION_STEPS: u64 = 64;

pub const PAYLOAD_BYTES: &[u8] = b"chaoscontrol-exact-snapshot-payload-v1";
pub const FIRST_CHUNK_BYTES: &[u8] = b"chaoscontrol-exact-";
pub const SECOND_CHUNK_BYTES: &[u8] = b"snapshot-payload-v1";
const KERNEL_BYTES: &[u8] = b"kernel-fixture";
const INITRD_BYTES: &[u8] = b"initrd-fixture";
const MANIFEST_BYTES: &[u8] = b"snapshot-chunk-manifest-fixture-v1";
const RUNTIME_BYTES: &[u8] = b"chaoscontrol-runtime-build-fixture-v1";

pub fn content(bytes: &[u8], algorithm: DigestAlgorithm, codec: &str) -> ContentIdentity {
    ContentIdentity {
        digest: digest_bytes(algorithm, bytes),
        length_bytes: u64::try_from(bytes.len()).expect("fixture byte length fits u64"),
        codec: codec.to_string(),
    }
}

pub fn topology() -> SnapshotTopology {
    SnapshotTopology {
        vcpu_count: VCPU_COUNT,
        memory_bytes: MEMORY_BYTES,
        msr_indices: vec![FIRST_MSR_INDEX, SECOND_MSR_INDEX],
        devices: vec![
            DeviceCohort {
                identity: DeviceIdentity {
                    base_address: BLOCK_BASE,
                    irq: BLOCK_IRQ,
                    device_id: BLOCK_DEVICE_ID,
                },
                queue_count: DEVICE_QUEUE_COUNT,
                backend: DeviceBackend::Block,
            },
            DeviceCohort {
                identity: DeviceIdentity {
                    base_address: NETWORK_BASE,
                    irq: NETWORK_IRQ,
                    device_id: NETWORK_DEVICE_ID,
                },
                queue_count: DEVICE_QUEUE_COUNT,
                backend: DeviceBackend::Network,
            },
            DeviceCohort {
                identity: DeviceIdentity {
                    base_address: ENTROPY_BASE,
                    irq: ENTROPY_IRQ,
                    device_id: ENTROPY_DEVICE_ID,
                },
                queue_count: DEVICE_QUEUE_COUNT,
                backend: DeviceBackend::Entropy,
            },
        ],
    }
}

pub fn runtime() -> RuntimeCohort {
    RuntimeCohort {
        runtime_build: digest_bytes(DigestAlgorithm::Blake3, RUNTIME_BYTES),
        kvm_operations: REQUIRED_KVM_OPERATIONS.map(str::to_string).to_vec(),
        scheduler_profile: "exact-single-step-v1".to_string(),
        time_profile: "virtual-tsc-v1".to_string(),
        entropy_profile: "seeded-chacha20-v1".to_string(),
    }
}

pub fn monolithic_descriptor() -> SnapshotDescriptor {
    let topology = topology();
    let logical_payload = content(
        PAYLOAD_BYTES,
        DigestAlgorithm::Sha256,
        CURRENT_PAYLOAD_CODEC,
    );
    SnapshotDescriptor {
        schema: DESCRIPTOR_SCHEMA.to_string(),
        descriptor_version: DESCRIPTOR_VERSION,
        completeness_profile: EXACT_SNAPSHOT_PROFILE.to_string(),
        state_schema_version: EXACT_STATE_SCHEMA_VERSION,
        architecture: EXACT_ARCHITECTURE.to_string(),
        runtime: runtime(),
        state_owners: expected_state_owners(&topology),
        topology,
        guest_artifacts: vec![
            GuestArtifact {
                role: GuestArtifactRole::Kernel,
                content: content(
                    KERNEL_BYTES,
                    DigestAlgorithm::Blake3,
                    "linux-kernel-image-v1",
                ),
            },
            GuestArtifact {
                role: GuestArtifactRole::Initrd,
                content: content(INITRD_BYTES, DigestAlgorithm::Blake3, "linux-initrd-v1"),
            },
        ],
        payload: PayloadClosure {
            kind: ClosureKind::Monolithic,
            logical_payload: logical_payload.clone(),
            manifest: None,
            members: vec![ClosureMember {
                order: 0,
                role: ClosureRole::SnapshotPayload,
                content: logical_payload,
            }],
        },
    }
}

pub fn chunked_descriptor() -> SnapshotDescriptor {
    let mut descriptor = monolithic_descriptor();
    descriptor.payload = PayloadClosure {
        kind: ClosureKind::Chunked,
        logical_payload: content(
            PAYLOAD_BYTES,
            DigestAlgorithm::Sha256,
            CURRENT_PAYLOAD_CODEC,
        ),
        manifest: Some(content(
            MANIFEST_BYTES,
            DigestAlgorithm::Blake3,
            CHUNK_MANIFEST_CODEC,
        )),
        members: vec![
            ClosureMember {
                order: 0,
                role: ClosureRole::SnapshotChunk,
                content: content(
                    FIRST_CHUNK_BYTES,
                    DigestAlgorithm::Sha256,
                    SNAPSHOT_CHUNK_CODEC,
                ),
            },
            ClosureMember {
                order: 1,
                role: ClosureRole::SnapshotChunk,
                content: content(
                    SECOND_CHUNK_BYTES,
                    DigestAlgorithm::Sha256,
                    SNAPSHOT_CHUNK_CODEC,
                ),
            },
        ],
    };
    descriptor
}

pub fn destination() -> DestinationObservation {
    DestinationObservation {
        destination_id: "fixture-destination".to_string(),
        completeness_profile: EXACT_SNAPSHOT_PROFILE.to_string(),
        state_schema_version: EXACT_STATE_SCHEMA_VERSION,
        architecture: EXACT_ARCHITECTURE.to_string(),
        runtime: runtime(),
        topology: topology(),
        available_memory_bytes: MEMORY_BYTES * AVAILABLE_MEMORY_MULTIPLIER,
    }
}

pub fn successful_receipt(descriptor: &SnapshotDescriptor) -> RestoreReceipt {
    let decision = preflight(descriptor, &destination()).expect("matching preflight");
    let plan = decision.plan.as_ref().expect("admitted plan");
    RestoreReceipt {
        descriptor_id: descriptor_identity(descriptor).expect("descriptor identity"),
        destination_id: plan.destination_id.clone(),
        preflight_id: preflight_identity(&decision).expect("preflight identity"),
        materialized: true,
        mutation_started: true,
        phases: REQUIRED_RESTORE_PHASES
            .iter()
            .map(|phase| RestorePhaseObservation {
                phase: *phase,
                status: PhaseStatus::Succeeded,
                diagnostic: None,
            })
            .collect(),
        poisoned: false,
        completed: true,
        continuation: Some(ContinuationObservation {
            checked_steps: CONTINUATION_STEPS,
            deterministic_trace_matches: true,
        }),
        non_claims: RESTORE_NON_CLAIMS.map(str::to_string).to_vec(),
    }
}

pub fn consumer_reference(descriptor: &SnapshotDescriptor) -> ConsumerSnapshotReference {
    let decision = preflight(descriptor, &destination()).expect("matching preflight");
    let mut closure_members = descriptor
        .payload
        .members
        .iter()
        .map(|member| member.content.clone())
        .collect::<Vec<_>>();
    closure_members.sort();
    ConsumerSnapshotReference {
        descriptor_id: descriptor_identity(descriptor).expect("descriptor identity"),
        completeness_profile: descriptor.completeness_profile.clone(),
        logical_payload: descriptor.payload.logical_payload.clone(),
        closure_members,
        preflight_id: preflight_identity(&decision).expect("preflight identity"),
        disallowed_claims: vec![],
    }
}
