use std::sync::Arc;

use vm_cohort_core::{
    new_cohort_state, reduce_effect_observation, validate_clone_isolation, CohortPhase,
    EffectObservation, OperationOutcome, ReceiptRef,
};

use crate::{
    execute_snapshot_cohort, map_snapshot_cohort, AdapterError, SnapshotCohortMappingRequest,
};

use super::support::{
    compatibility_facts, mapped_fixture, profile, profile_ref, resource_ref, synthetic_block,
    synthetic_snapshot, WORKER_COUNT,
};

// r[verify chaoscontrol.vm_cohort.adapter]
#[test]
fn exact_snapshot_maps_complete_bases_and_private_clone_surfaces() {
    let (snapshot, block, mapped) = mapped_fixture(WORKER_COUNT);
    let expected_memory =
        vm_cohort_kvm::identify_bytes(&snapshot.memory.materialize()).expect("memory identity");
    let expected_disk = vm_cohort_kvm::identify_bytes(&block.materialize()).expect("disk identity");

    assert_eq!(mapped.plan.reservation.workers, WORKER_COUNT);
    assert_eq!(
        mapped.plan.clones.len(),
        usize::try_from(WORKER_COUNT).expect("worker count fits usize")
    );
    assert!(mapped
        .plan
        .clones
        .iter()
        .all(|clone| clone.memory_base_ref == expected_memory));
    assert!(mapped
        .plan
        .clones
        .iter()
        .all(|clone| clone.disk_base_ref == expected_disk));
    assert!(validate_clone_isolation(&mapped.plan).is_empty());
    assert!(!mapped.snapshot_ref.as_str().is_empty());
}

// r[verify chaoscontrol.vm_cohort.verification]
#[test]
fn incomplete_snapshot_and_profile_drift_fail_before_planning() {
    let mut snapshot = synthetic_snapshot();
    snapshot.metadata = None;
    let block = synthetic_block();
    let kvm_profile = profile();
    let facts = compatibility_facts(&kvm_profile);
    let missing_metadata = map_snapshot_cohort(SnapshotCohortMappingRequest {
        snapshot: &snapshot,
        block: &block,
        facts: &facts,
        kvm_profile: kvm_profile.clone(),
        workers: WORKER_COUNT,
        context_ref: resource_ref("missing-metadata"),
    });
    assert!(matches!(missing_metadata, Err(AdapterError::Admission(_))));

    let snapshot = synthetic_snapshot();
    let mut drifted_facts = compatibility_facts(&kvm_profile);
    drifted_facts.profile_ref = profile_ref("drifted-profile");
    let profile_drift = map_snapshot_cohort(SnapshotCohortMappingRequest {
        snapshot: &snapshot,
        block: &block,
        facts: &drifted_facts,
        kvm_profile,
        workers: WORKER_COUNT,
        context_ref: resource_ref("profile-drift"),
    });
    assert!(matches!(profile_drift, Err(AdapterError::Admission(_))));
}

// r[verify chaoscontrol.vm_cohort.verification]
#[test]
fn execution_rejects_snapshot_and_immutable_base_drift_before_activation() {
    let (mut snapshot, _block, mapped) = mapped_fixture(WORKER_COUNT);
    snapshot.virtual_tsc = snapshot.virtual_tsc.saturating_add(1);
    let snapshot_drift = execute_snapshot_cohort(&mapped, &snapshot);
    assert!(matches!(snapshot_drift, Err(AdapterError::Admission(_))));

    let (snapshot, _block, mut mapped) = mapped_fixture(WORKER_COUNT);
    mapped.disk = Arc::from(vec![0_u8; mapped.disk.len()]);
    let base_drift = execute_snapshot_cohort(&mapped, &snapshot);
    assert!(matches!(base_drift, Err(AdapterError::Kvm(_))));
}

// r[verify chaoscontrol.vm_cohort.verification]
#[test]
fn shared_mutable_surface_tampering_is_detected() {
    let (_snapshot, _block, mut mapped) = mapped_fixture(WORKER_COUNT);
    let shared_ref = mapped.plan.clones[0].surfaces[0].resource_ref.clone();
    mapped.plan.clones[1].surfaces[0].resource_ref = shared_ref;
    assert!(!validate_clone_isolation(&mapped.plan).is_empty());
}

// r[verify chaoscontrol.vm_cohort.verification]
#[test]
fn partial_creation_and_unknown_effects_retain_cleanup_obligations() {
    let (_snapshot, _block, mapped) = mapped_fixture(WORKER_COUNT);
    let first = &mapped.plan.effects[0];
    let second = &mapped.plan.effects[1];

    let planned = new_cohort_state(mapped.plan.clone());
    let preparing = reduce_effect_observation(
        &planned,
        &observation(
            &mapped.plan.cohort_ref,
            first,
            OperationOutcome::Success,
            "first-success",
        ),
    )
    .expect("first success observation");
    let failed = reduce_effect_observation(
        &preparing,
        &observation(
            &mapped.plan.cohort_ref,
            second,
            OperationOutcome::PermanentFailure,
            "partial-failure",
        ),
    )
    .expect("partial failure observation");
    assert_eq!(failed.phase, CohortPhase::CleanupRequired);
    assert!(!failed.cleanup_obligations.is_empty());
    assert!(failed.activated_clones.is_empty());

    let uncertain = reduce_effect_observation(
        &preparing,
        &observation(
            &mapped.plan.cohort_ref,
            second,
            OperationOutcome::OutcomeUnknown,
            "unknown-effect",
        ),
    )
    .expect("unknown effect observation");
    assert_eq!(uncertain.phase, CohortPhase::CleanupUncertain);
    assert_eq!(uncertain.cleanup_obligations, failed.cleanup_obligations);
    assert!(uncertain.activated_clones.is_empty());
}

fn observation(
    cohort_ref: &vm_cohort_core::CohortRef,
    effect: &vm_cohort_core::EffectRequest,
    outcome: OperationOutcome,
    label: &str,
) -> EffectObservation {
    EffectObservation {
        cohort_ref: cohort_ref.clone(),
        clone_ref: effect.clone_ref.clone(),
        operation_ref: effect.operation_ref.clone(),
        kind: effect.kind,
        outcome,
        observation_ref: receipt_ref(label),
    }
}

fn receipt_ref(label: &str) -> ReceiptRef {
    ReceiptRef::new(format!(
        "blake3:{}",
        blake3::hash(label.as_bytes()).to_hex()
    ))
    .expect("receipt reference")
}
