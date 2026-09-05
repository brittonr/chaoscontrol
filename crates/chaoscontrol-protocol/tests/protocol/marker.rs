use super::*;

#[test]
fn marker_linkage_requires_declared_record_and_exact_snapshot_context() {
    let admitted = admit_profile(profile()).unwrap();
    let mut records = vec![
        observation(&admitted, 0, 0, 0, DrainState::Final, first_projection()),
        observation(&admitted, 1, 0, 0, DrainState::Final, first_projection()),
    ];
    let boundary = records[0].draft.logical_boundary_ref.clone();
    let projection = records[0].draft.projection_ref.clone();
    let snapshot = reference("snapshot", 'e');
    let marker = BranchMarker::new(
        "raft",
        "declared",
        "fixture",
        serde_json::json!({"projection_ref": projection}),
        None,
        Some(boundary.clone()),
    )
    .unwrap();
    let undeclared =
        assemble_cohort(&admitted, &boundary, &records, ProjectionSupport::Available).unwrap();
    assert!(bind_marker_snapshot(&admitted, &undeclared, &marker, &projection, &snapshot).is_err());
    records[0].draft.marker_identity = Some(marker.identity.clone());
    records[0].draft.parent_snapshot_ref = Some(snapshot.clone());
    let cohort =
        assemble_cohort(&admitted, &boundary, &records, ProjectionSupport::Available).unwrap();
    let binding =
        bind_marker_snapshot(&admitted, &cohort, &marker, &projection, &snapshot).unwrap();
    validate_marker_binding(&admitted, &cohort, &binding).unwrap();
    let status = build_status(&admitted, &cohort, None, Some(&binding)).unwrap();
    assert_eq!(
        serde_json::to_value(status.marker_reachability).unwrap(),
        "identity-linked",
        "a structural link does not establish snapshot reachability"
    );
    assert_eq!(
        build_status(&admitted, &cohort, None, None)
            .unwrap()
            .marker_reachability,
        MarkerReachability::NotBound
    );
    for (field, value) in [
        ("marker_identity", reference("b3", 'f')),
        ("projection_ref", reference("projection", 'f')),
        ("record_ref", reference("protocol-record", 'f')),
        ("cohort_identity", reference("protocol-cohort", 'f')),
        ("logical_boundary_ref", reference("logical-boundary", 'e')),
        ("parent_snapshot_ref", reference("snapshot", 'f')),
        ("scheduler_state_ref", reference("schedule-state", 'f')),
        ("parent_snapshot_ref", String::new()),
    ] {
        let mut altered = serde_json::to_value(&binding).unwrap();
        altered[field] = value.into();
        let altered: MarkerSnapshotBinding = serde_json::from_value(altered).unwrap();
        assert!(
            validate_marker_binding(&admitted, &cohort, &altered).is_err(),
            "{field}"
        );
    }
    let incomplete = assemble_cohort(
        &admitted,
        &boundary,
        &records[..1],
        ProjectionSupport::Available,
    )
    .unwrap();
    assert!(validate_marker_binding(&admitted, &incomplete, &binding).is_err());
}

#[test]
fn oracle_results_cannot_change_verdict_work_or_cohort_identity() {
    let admitted = admit_profile(profile()).unwrap();
    let records = vec![
        observation(&admitted, 0, 0, 0, DrainState::Final, first_projection()),
        observation(&admitted, 1, 0, 0, DrainState::Final, first_projection()),
    ];
    let cohort = assemble_cohort(
        &admitted,
        &records[0].draft.logical_boundary_ref,
        &records,
        ProjectionSupport::Available,
    )
    .unwrap();
    let oracle = FixedOracle {
        adapter_ref: admitted.profile.oracle.adapter_ref.clone(),
        authority: OracleAuthority::ConsumerIndependent,
        verdict: ProtocolVerdict::Pass,
    };
    let result = run_consumer_oracle(&admitted, &cohort, &oracle).unwrap();
    validate_oracle_result(&admitted, &cohort, &result).unwrap();
    let mut altered = result.clone();
    altered.decision.verdict = ProtocolVerdict::Fail;
    assert!(validate_oracle_result(&admitted, &cohort, &altered).is_err());
    let mut altered = result.clone();
    altered.decision.work_items = u32::MAX;
    assert_eq!(
        validate_oracle_result(&admitted, &cohort, &altered),
        Err(ProtocolObservationError::OracleWorkExceeded)
    );
    let mut altered = result;
    altered.cohort_identity = reference("protocol-cohort", 'f');
    assert_eq!(
        validate_oracle_result(&admitted, &cohort, &altered),
        Err(ProtocolObservationError::OracleMismatch)
    );
}
