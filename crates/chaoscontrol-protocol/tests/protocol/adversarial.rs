use super::*;

fn pair(profile: &AdmittedProfile) -> Vec<CollectedObservation> {
    vec![
        observation(profile, 0, 0, 0, DrainState::Final, first_projection()),
        observation(profile, 1, 0, 0, DrainState::Final, first_projection()),
    ]
}

#[test]
fn duplicate_journal_replay_is_equivalent_but_changed_metadata_conflicts() {
    let admitted = admit_profile(profile()).unwrap();
    let mut records = pair(&admitted);
    let boundary = records[0].draft.logical_boundary_ref.clone();
    let original =
        assemble_cohort(&admitted, &boundary, &records, ProjectionSupport::Available).unwrap();
    records.push(records[0].clone());
    let repeated =
        assemble_cohort(&admitted, &boundary, &records, ProjectionSupport::Available).unwrap();
    assert_eq!(original, repeated);
    records.last_mut().unwrap().draft.drain_state = DrainState::Open;
    assert_eq!(
        assemble_cohort(&admitted, &boundary, &records, ProjectionSupport::Available)
            .unwrap()
            .classification,
        CohortClassification::Conflicting
    );
}

#[test]
fn post_final_records_and_mixed_executions_cannot_complete() {
    let admitted = admit_profile(profile()).unwrap();
    let mut records = pair(&admitted);
    let boundary = records[0].draft.logical_boundary_ref.clone();
    records.push(observation(
        &admitted,
        0,
        1,
        0,
        DrainState::Final,
        first_projection(),
    ));
    assert_ne!(
        assemble_cohort(&admitted, &boundary, &records, ProjectionSupport::Available)
            .unwrap()
            .classification,
        CohortClassification::Complete
    );
    records.pop();
    records[1].draft.execution_ref = reference("execution", 'e');
    assert_ne!(
        assemble_cohort(&admitted, &boundary, &records, ProjectionSupport::Available)
            .unwrap()
            .classification,
        CohortClassification::Complete
    );
}

#[test]
fn malformed_support_and_forged_complete_flags_do_not_admit_oracles() {
    let admitted = admit_profile(profile()).unwrap();
    let mut records = pair(&admitted);
    let boundary = records[0].draft.logical_boundary_ref.clone();
    records[0].draft.profile_ref = "invalid".into();
    let malformed = assemble_cohort(
        &admitted,
        &boundary,
        &records,
        ProjectionSupport::Unavailable,
    )
    .unwrap();
    assert_ne!(malformed.classification, CohortClassification::Unsupported);
    let mut forged =
        assemble_cohort(&admitted, &boundary, &[], ProjectionSupport::Available).unwrap();
    forged.classification = CohortClassification::Complete;
    let oracle = FixedOracle {
        adapter_ref: admitted.profile.oracle.adapter_ref.clone(),
        authority: OracleAuthority::ConsumerIndependent,
        verdict: ProtocolVerdict::Pass,
    };
    assert!(run_consumer_oracle(&admitted, &forged, &oracle).is_err());
}

#[test]
fn finite_implementation_limits_reject_maximum_integer_profiles() {
    admit_profile(profile()).unwrap();
    let mut oversized = profile();
    oversized.bounds.max_cohort_backlog = u32::MAX;
    assert!(admit_profile(oversized).is_err());
    let mut inconsistent = profile();
    inconsistent.bounds.max_cohort_backlog = 1;
    assert!(admit_profile(inconsistent).is_err());
    let mut overclaim = profile();
    overclaim.non_claims = vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()];
    assert!(admit_profile(overclaim).is_err());
}
