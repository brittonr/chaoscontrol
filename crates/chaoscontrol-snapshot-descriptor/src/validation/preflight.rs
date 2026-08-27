#![allow(
    non_trait_imports,
    reason = "preflight compares the complete descriptor and destination vocabularies in one pure decision function"
)]
#![allow(
    path_segment_repetition,
    reason = "qualified validation calls keep descriptor admission separate from destination comparison"
)]

use crate::canonical::{descriptor_identity, destination_identity};
use crate::model::SnapshotDescriptor;
use crate::observations::{
    DestinationObservation, PreflightBlocker, PreflightDecision, PreflightStatus, RestorePlan,
    REQUIRED_RESTORE_PHASES,
};
use crate::validation::descriptor::{validate_digest, validate_text, validate_topology};
use crate::validation::DescriptorError;

// r[impl chaoscontrol.snapshot_descriptor.preflight]
pub fn preflight(
    descriptor: &SnapshotDescriptor,
    destination: &DestinationObservation,
) -> Result<PreflightDecision, DescriptorError> {
    crate::validation::validate_descriptor(descriptor)?;
    validate_destination_shape(destination)?;
    let descriptor_id = descriptor_identity(descriptor)?;
    let destination_id = destination_identity(destination)?;
    let mut blockers = Vec::new();
    compare_text(
        &mut blockers,
        "profile-mismatch",
        &descriptor.completeness_profile,
        &destination.completeness_profile,
    );
    compare_value(
        &mut blockers,
        "state-schema-mismatch",
        descriptor.state_schema_version,
        destination.state_schema_version,
    );
    compare_text(
        &mut blockers,
        "architecture-mismatch",
        &descriptor.architecture,
        &destination.architecture,
    );
    compare_value(
        &mut blockers,
        "runtime-build-mismatch",
        &descriptor.runtime.runtime_build,
        &destination.runtime.runtime_build,
    );
    compare_value(
        &mut blockers,
        "kvm-operation-mismatch",
        &descriptor.runtime.kvm_operations,
        &destination.runtime.kvm_operations,
    );
    compare_text(
        &mut blockers,
        "scheduler-profile-mismatch",
        &descriptor.runtime.scheduler_profile,
        &destination.runtime.scheduler_profile,
    );
    compare_text(
        &mut blockers,
        "time-profile-mismatch",
        &descriptor.runtime.time_profile,
        &destination.runtime.time_profile,
    );
    compare_text(
        &mut blockers,
        "entropy-profile-mismatch",
        &descriptor.runtime.entropy_profile,
        &destination.runtime.entropy_profile,
    );
    compare_value(
        &mut blockers,
        "vcpu-topology-mismatch",
        descriptor.topology.vcpu_count,
        destination.topology.vcpu_count,
    );
    compare_value(
        &mut blockers,
        "memory-shape-mismatch",
        descriptor.topology.memory_bytes,
        destination.topology.memory_bytes,
    );
    compare_value(
        &mut blockers,
        "msr-inventory-mismatch",
        &descriptor.topology.msr_indices,
        &destination.topology.msr_indices,
    );
    compare_value(
        &mut blockers,
        "device-cohort-mismatch",
        &descriptor.topology.devices,
        &destination.topology.devices,
    );
    if destination.available_memory_bytes < descriptor.topology.memory_bytes {
        blockers.push(PreflightBlocker {
            code: "memory-resource-insufficient".to_string(),
            expected: descriptor.topology.memory_bytes.to_string(),
            observed: destination.available_memory_bytes.to_string(),
        });
    }
    if blockers.is_empty() {
        Ok(PreflightDecision {
            status: PreflightStatus::Admitted,
            blockers,
            plan: Some(RestorePlan {
                descriptor_id,
                destination_id,
                phases: REQUIRED_RESTORE_PHASES.to_vec(),
            }),
        })
    } else {
        Ok(PreflightDecision {
            status: PreflightStatus::Denied,
            blockers,
            plan: None,
        })
    }
}

fn validate_destination_shape(destination: &DestinationObservation) -> Result<(), DescriptorError> {
    validate_text("destination-id", &destination.destination_id)?;
    validate_text("destination-profile", &destination.completeness_profile)?;
    validate_text("destination-architecture", &destination.architecture)?;
    validate_digest(&destination.runtime.runtime_build)?;
    validate_text(
        "destination-scheduler",
        &destination.runtime.scheduler_profile,
    )?;
    validate_text("destination-time", &destination.runtime.time_profile)?;
    validate_text("destination-entropy", &destination.runtime.entropy_profile)?;
    validate_topology(&destination.topology)
}

fn compare_text(
    blockers: &mut Vec<PreflightBlocker>,
    code: &'static str,
    expected: &str,
    observed: &str,
) {
    if expected != observed {
        blockers.push(PreflightBlocker {
            code: code.to_string(),
            expected: expected.to_string(),
            observed: observed.to_string(),
        });
    }
}

fn compare_value<T>(
    blockers: &mut Vec<PreflightBlocker>,
    code: &'static str,
    expected: T,
    observed: T,
) where
    T: PartialEq + std::fmt::Debug,
{
    if expected != observed {
        blockers.push(PreflightBlocker {
            code: code.to_string(),
            expected: format!("{expected:?}"),
            observed: format!("{observed:?}"),
        });
    }
}
