mod cleanup;
mod model;

pub use model::*;

use chaoscontrol_vmm::snapshot::{VcpuSnapshot, VmSnapshot};
use vm_cohort_core::{
    new_cohort_state, project_receipt, reduce_effect_observation, CloneRef, CohortPhase,
    EffectKind, EffectObservation, EffectRequest, FailureClass, OperationOutcome,
};
use vm_cohort_kvm::{KvmAdapterError, KvmCohortRuntime};

use crate::mapping::receipt_from_bytes;
use crate::{AdapterError, ChaosCohortOutcome, MappedChaosCohort};
use cleanup::{finish_failure, observation_ref};

/// Executes one mapped cohort and applies exact ChaosControl state before activation.
///
/// # Errors
///
/// Returns a bounded error for input drift, runtime construction, observation,
/// cleanup projection, or receipt failure.
// r[impl chaoscontrol.vm_cohort.restore]
pub fn execute_snapshot_cohort(
    mapped: &MappedChaosCohort,
    snapshot: &VmSnapshot,
) -> Result<ChaosCohortExecution, AdapterError> {
    validate_execution_snapshot(mapped, snapshot)?;
    let runtime = new_runtime(mapped)?;
    execute_snapshot_cohort_with_runtime(mapped, snapshot, runtime)
}

/// Executes with a caller-prepared runtime, including a conformance failure schedule.
///
/// # Errors
///
/// Returns a bounded error for input drift, crossed observations, cleanup projection,
/// or receipt failure. Effect and restore failures return typed failed executions.
pub fn execute_snapshot_cohort_with_runtime(
    mapped: &MappedChaosCohort,
    snapshot: &VmSnapshot,
    mut runtime: KvmCohortRuntime,
) -> Result<ChaosCohortExecution, AdapterError> {
    validate_execution_snapshot(mapped, snapshot)?;
    let mut state = new_cohort_state(mapped.plan.clone());
    for effect in &mapped.plan.effects {
        let observation = match runtime.execute_effect(&mapped.plan, effect) {
            Ok(observation) => observation,
            Err(error) => {
                let outcome = classify_kvm_outcome(&error);
                let failed = failure_observation(mapped, effect, outcome)?;
                let failed_state = reduce_effect_observation(&state, &failed)
                    .map_err(|_| AdapterError::Core("failure observation"))?;
                return finish_failure(
                    runtime,
                    failed_state,
                    ChaosExecutionFailureKind::KvmEffect,
                    error.to_string(),
                );
            }
        };
        if effect.kind == EffectKind::RestoreVcpu {
            let restore_result = runtime
                .clone_descriptors(&effect.clone_ref)
                .ok_or(AdapterError::Core("live clone descriptors"))
                .and_then(|(vm, vcpus)| {
                    snapshot
                        .restore_devices_only(vcpus, vm)
                        .map_err(|error| AdapterError::Snapshot(error.to_string()))
                });
            if let Err(error) = restore_result {
                let failed = failure_observation(mapped, effect, OperationOutcome::OutcomeUnknown)?;
                let failed_state = reduce_effect_observation(&state, &failed)
                    .map_err(|_| AdapterError::Core("snapshot failure observation"))?;
                return finish_failure(
                    runtime,
                    failed_state,
                    ChaosExecutionFailureKind::SnapshotRestore,
                    error.to_string(),
                );
            }
        }
        state = reduce_effect_observation(&state, &observation)
            .map_err(|_| AdapterError::Core("success observation"))?;
    }
    if state.phase != CohortPhase::Active {
        return Err(AdapterError::Core("cohort did not activate"));
    }
    let mechanism_receipt =
        project_receipt(&state).map_err(|_| AdapterError::Core("mechanism receipt"))?;
    Ok(ChaosCohortExecution::Active(ActiveChaosCohort {
        outcome: ChaosCohortOutcome {
            state,
            mechanism_receipt_ref: mechanism_receipt.receipt_ref,
            fault_authority_granted: false,
            replay_authority_granted: false,
            release_authority_granted: false,
        },
        runtime,
    }))
}

/// Applies one ChaosControl-owned exact vCPU state to one prepared VM Cohort clone.
///
/// # Errors
///
/// Returns a bounded error for missing descriptors, index drift, or KVM restore failure.
pub fn restore_vcpu_snapshot(
    snapshot: &VcpuSnapshot,
    runtime: &KvmCohortRuntime,
    clone_ref: &CloneRef,
    vcpu_index: u32,
) -> Result<(), AdapterError> {
    let vcpu_index = usize::try_from(vcpu_index)
        .map_err(|_| AdapterError::Admission("vCPU index exceeds usize"))?;
    let (_vm, vcpus) = runtime
        .clone_descriptors(clone_ref)
        .ok_or(AdapterError::Core("live clone descriptors"))?;
    let vcpu = vcpus
        .get(vcpu_index)
        .ok_or(AdapterError::Admission("vCPU index is out of bounds"))?;
    snapshot
        .restore(vcpu)
        .map_err(|error| AdapterError::Snapshot(error.to_string()))
}

fn new_runtime(mapped: &MappedChaosCohort) -> Result<KvmCohortRuntime, AdapterError> {
    let first = mapped
        .plan
        .clones
        .first()
        .ok_or(AdapterError::Core("cohort plan has no clone"))?;
    KvmCohortRuntime::new(
        mapped.kvm_profile.clone(),
        &mapped.memory,
        &first.memory_base_ref,
        mapped.disk.clone(),
        &first.disk_base_ref,
    )
    .map_err(|error| AdapterError::Kvm(error.to_string()))
}

fn validate_execution_snapshot(
    mapped: &MappedChaosCohort,
    snapshot: &VmSnapshot,
) -> Result<(), AdapterError> {
    let snapshot_bytes = serde_json::to_vec(snapshot)?;
    let observed = receipt_from_bytes(&snapshot_bytes)?;
    if observed != mapped.snapshot_ref {
        return Err(AdapterError::Admission(
            "execution snapshot differs from the mapped snapshot",
        ));
    }
    Ok(())
}

fn failure_observation(
    mapped: &MappedChaosCohort,
    effect: &EffectRequest,
    outcome: OperationOutcome,
) -> Result<EffectObservation, AdapterError> {
    Ok(EffectObservation {
        cohort_ref: mapped.plan.cohort_ref.clone(),
        clone_ref: effect.clone_ref.clone(),
        operation_ref: effect.operation_ref.clone(),
        kind: effect.kind,
        outcome,
        observation_ref: observation_ref("effect-failure", effect.operation_ref.as_str())?,
    })
}

fn classify_kvm_outcome(error: &KvmAdapterError) -> OperationOutcome {
    match error {
        KvmAdapterError::Injected(FailureClass::TransientKvm | FailureClass::ResourceExhausted) => {
            OperationOutcome::TransientFailure
        }
        KvmAdapterError::Injected(FailureClass::OutcomeUnknown)
        | KvmAdapterError::Base(_)
        | KvmAdapterError::Memfd(_)
        | KvmAdapterError::Mapping(_)
        | KvmAdapterError::Kvm(_) => OperationOutcome::OutcomeUnknown,
        KvmAdapterError::Admission(_)
        | KvmAdapterError::Missing(_)
        | KvmAdapterError::Injected(FailureClass::Compatibility | FailureClass::Permanent) => {
            OperationOutcome::PermanentFailure
        }
    }
}
