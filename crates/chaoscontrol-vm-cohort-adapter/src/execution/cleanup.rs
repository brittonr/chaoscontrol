use vm_cohort_core::{
    reduce_cleanup_observation, CleanupObservation, CleanupOutcome, CohortPhase, CohortState,
    ReceiptRef, ReferenceError,
};
use vm_cohort_kvm::KvmCohortRuntime;

use super::{ChaosCohortExecution, ChaosExecutionFailureKind, FailedChaosCohort};
use crate::AdapterError;

const MAX_FAILURE_DETAIL_CHARS: usize = 512;

pub(super) fn finish_failure(
    mut runtime: KvmCohortRuntime,
    required_state: CohortState,
    kind: ChaosExecutionFailureKind,
    detail: String,
) -> Result<ChaosCohortExecution, AdapterError> {
    let retained_state = required_state.clone();
    let (state, runtime) = match runtime.cleanup_obligations(required_state) {
        Ok(cleaned) => (cleaned, None),
        Err(cleanup_error) => {
            let uncertain = retain_cleanup_uncertainty(&retained_state)?;
            let detail = bounded_detail(&format!(
                "{detail}; cleanup outcome unknown: {cleanup_error}"
            ));
            return Ok(failed_execution(
                uncertain,
                kind,
                detail,
                true,
                Some(runtime),
            ));
        }
    };
    Ok(failed_execution(
        state,
        kind,
        bounded_detail(&detail),
        false,
        runtime,
    ))
}

fn failed_execution(
    state: CohortState,
    kind: ChaosExecutionFailureKind,
    detail: String,
    cleanup_uncertain: bool,
    runtime: Option<KvmCohortRuntime>,
) -> ChaosCohortExecution {
    ChaosCohortExecution::Failed(FailedChaosCohort {
        state,
        kind,
        detail,
        cleanup_uncertain,
        runtime,
        fault_authority_granted: false,
        replay_authority_granted: false,
        release_authority_granted: false,
    })
}

fn retain_cleanup_uncertainty(state: &CohortState) -> Result<CohortState, AdapterError> {
    if state.phase == CohortPhase::CleanupUncertain {
        return Ok(state.clone());
    }
    let obligation = state
        .cleanup_obligations
        .first()
        .ok_or(AdapterError::Core("cleanup failure has no obligation"))?;
    let observation = CleanupObservation {
        cohort_ref: state.plan.cohort_ref.clone(),
        clone_ref: obligation.clone_ref.clone(),
        cleanup_operation_ref: obligation.cleanup_operation_ref.clone(),
        resource_ref: obligation.resource_ref.clone(),
        outcome: CleanupOutcome::OutcomeUnknown,
        observation_ref: observation_ref(
            "cleanup-unknown",
            obligation.cleanup_operation_ref.as_str(),
        )?,
    };
    reduce_cleanup_observation(state, &observation)
        .map_err(|_| AdapterError::Core("cleanup uncertainty observation"))
}

fn bounded_detail(value: &str) -> String {
    value.chars().take(MAX_FAILURE_DETAIL_CHARS).collect()
}

pub(super) fn observation_ref(
    domain: &str,
    operation_ref: &str,
) -> Result<ReceiptRef, AdapterError> {
    let mut hasher =
        blake3::Hasher::new_derive_key("onixresearch.chaoscontrol.vm-cohort-observation.v1");
    hasher.update(domain.as_bytes());
    hasher.update(operation_ref.as_bytes());
    ReceiptRef::new(format!("blake3:{}", hasher.finalize().to_hex()))
        .map_err(|ReferenceError| AdapterError::Admission("observation identity"))
}
