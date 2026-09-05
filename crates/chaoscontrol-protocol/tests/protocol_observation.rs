#![cfg(feature = "std")]

use chaoscontrol_protocol::branch_marker::BranchMarker;
use chaoscontrol_protocol::protocol_observation::*;
#[path = "protocol/fixtures.rs"]
mod fixtures;
use fixtures::*;
#[path = "protocol/admission.rs"]
mod admission;
#[path = "protocol/adversarial.rs"]
mod adversarial;
#[path = "protocol/marker.rs"]
mod marker;

#[test]
fn complete_cohort_builds_replay_stable_evidence() {
    let admitted = admit_profile(profile()).unwrap();
    assert_eq!(admitted, admit_profile(profile()).unwrap());
    let mut observations = vec![
        observation(&admitted, 0, 0, 0, DrainState::Final, first_projection()),
        observation(&admitted, 1, 0, 0, DrainState::Final, first_projection()),
    ];
    let boundary = reference("logical-boundary", 'f');
    let marker = BranchMarker::new(
        "raft",
        "leader-elected",
        "raft-consumer",
        serde_json::json!({"projection_ref": observations[0].draft.projection_ref}),
        None,
        Some(boundary.clone()),
    )
    .unwrap();
    observations[0].draft.marker_identity = Some(marker.identity.clone());
    let cohort = assemble_cohort(
        &admitted,
        &boundary,
        &observations,
        ProjectionSupport::Available,
    )
    .unwrap();
    assert_eq!(cohort.classification, CohortClassification::Complete);
    assert!(cohort.issues.is_empty());
    let oracle = FixedOracle {
        adapter_ref: admitted.profile.oracle.adapter_ref.clone(),
        authority: OracleAuthority::ConsumerIndependent,
        verdict: ProtocolVerdict::Pass,
    };
    let oracle_result = run_consumer_oracle(&admitted, &cohort, &oracle).unwrap();
    let binding = bind_marker_snapshot(
        &admitted,
        &cohort,
        &marker,
        &observations[0].draft.projection_ref,
        &reference("snapshot", 'a'),
    )
    .unwrap();
    let context = ProtocolEvidenceContext {
        marker_binding: Some(binding),
        fault_refs: vec![reference("fault", 'b')],
        replay_refs: vec![reference("replay", 'c')],
    };
    let receipt = build_receipt(&admitted, &cohort, Some(oracle_result.clone()), context).unwrap();
    validate_receipt(&admitted, &cohort, &receipt).unwrap();
    let status = build_status(
        &admitted,
        &cohort,
        Some(&oracle_result),
        receipt.marker_binding.as_ref(),
    )
    .unwrap();
    assert_eq!(status.oracle_verdict, Some(ProtocolVerdict::Pass));
    assert!(status.missing_participants.is_empty());
    let slot = novelty_coverage_slot(
        &cohort.novelty_identities[0],
        COVERAGE_REGION_START,
        COVERAGE_REGION_SIZE,
    )
    .unwrap();
    assert_eq!(
        slot,
        novelty_coverage_slot(
            &cohort.novelty_identities[0],
            COVERAGE_REGION_START,
            COVERAGE_REGION_SIZE
        )
        .unwrap()
    );
    let mut forged = receipt;
    forged.participant_refs.clear();
    assert!(validate_receipt(&admitted, &cohort, &forged).is_err());
}

#[test]
fn incomplete_and_conflicting_cohorts_fail_closed() {
    let admitted = admit_profile(profile()).unwrap();
    let incomplete = vec![observation(
        &admitted,
        0,
        1,
        1,
        DrainState::Open,
        first_projection(),
    )];
    let boundary = reference("logical-boundary", 'f');
    let cohort = assemble_cohort(
        &admitted,
        &boundary,
        &incomplete,
        ProjectionSupport::Available,
    )
    .unwrap();
    assert_eq!(cohort.classification, CohortClassification::Incomplete);
    assert!(cohort
        .issues
        .iter()
        .any(|issue| issue.kind == CohortIssueKind::FailedFinalDrain));
    let oracle = FixedOracle {
        adapter_ref: admitted.profile.oracle.adapter_ref.clone(),
        authority: OracleAuthority::ConsumerIndependent,
        verdict: ProtocolVerdict::Pass,
    };
    assert_eq!(
        run_consumer_oracle(&admitted, &cohort, &oracle),
        Err(ProtocolObservationError::CohortNotComplete)
    );
    let conflicting = vec![
        observation(&admitted, 0, 0, 0, DrainState::Final, first_projection()),
        observation(&admitted, 0, 0, 0, DrainState::Final, second_projection()),
        observation(&admitted, 1, 0, 0, DrainState::Final, first_projection()),
    ];
    assert_eq!(
        assemble_cohort(
            &admitted,
            &boundary,
            &conflicting,
            ProjectionSupport::Available
        )
        .unwrap()
        .classification,
        CohortClassification::Conflicting
    );
}

#[test]
fn invalid_profiles_or_authority_cannot_create_evidence() {
    let mut self_report = profile();
    self_report.oracle.authority = OracleAuthority::RuntimeSelfReport;
    assert_eq!(
        admit_profile(self_report),
        Err(ProtocolObservationError::RuntimeSelfOracle)
    );
    let mut reversed = profile();
    reversed.required_participants.reverse();
    assert_eq!(
        admit_profile(reversed),
        Err(ProtocolObservationError::NonCanonicalOrder("participants"))
    );
    let mut value = serde_json::to_value(profile()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("unknown".into(), serde_json::Value::Null);
    assert!(serde_json::from_value::<ProtocolObservationProfile>(value).is_err());
    assert_eq!(
        validate_claim(ProtocolObservationClaim::ProtocolSemantics),
        Err(ProtocolObservationError::ClaimOverreach)
    );
    validate_claim(ProtocolObservationClaim::BoundedObservation).unwrap();
}
