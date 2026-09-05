use super::*;

#[test]
fn nickel_projection_matches_rust_fixture_and_rejects_envelope_drift() {
    let exported = include_bytes!("../../../../contracts/protocol-observation/fixtures/valid.json");
    let admitted = decode_profile(exported).unwrap();
    assert_eq!(admitted.profile, profile());
    let valid = observation(&admitted, 0, 0, 0, DrainState::Final, first_projection());
    admit_observation(&admitted, valid.clone()).unwrap();
    let value = serde_json::to_value(valid).unwrap();
    for field in [
        "schema",
        "profile_ref",
        "protocol_ref",
        "cohort_ref",
        "producer_ref",
        "participant_ref",
        "process_ref",
        "execution_ref",
        "logical_boundary_ref",
        "projection_schema_ref",
        "projection_ref",
        "novelty_identity",
    ] {
        let mut bad = value.clone();
        bad["draft"][field] = "invalid".into();
        let bad: CollectedObservation = serde_json::from_value(bad).unwrap();
        assert!(admit_observation(&admitted, bad).is_err(), "{field}");
    }
    for field in ["generation", "source_sequence", "source_loss_count"] {
        let mut bad = value.clone();
        bad["draft"][field] = serde_json::json!(u64::MAX);
        let bad: CollectedObservation = serde_json::from_value(bad).unwrap();
        assert!(admit_observation(&admitted, bad).is_err(), "{field}");
    }
    for field in ["vm_id", "active_vcpu"] {
        let mut bad = value.clone();
        bad["scheduler_position"][field] = serde_json::json!(u32::MAX);
        let bad: CollectedObservation = serde_json::from_value(bad).unwrap();
        assert!(admit_observation(&admitted, bad).is_err(), "{field}");
    }
    let mut unknown = value;
    unknown["scheduler_position"]["unknown"] = true.into();
    assert!(serde_json::from_value::<CollectedObservation>(unknown).is_err());
}

#[test]
fn omitted_optional_fields_preserve_compatibility_but_required_fields_fail() {
    let admitted = admit_profile(profile()).unwrap();
    let mut record = observation(&admitted, 0, 0, 0, DrainState::Final, first_projection());
    record.draft.projection_bytes = None;
    let value = serde_json::to_value(&record).unwrap();
    for field in ["projection_bytes", "marker_identity", "parent_snapshot_ref"] {
        assert!(value["draft"].get(field).is_none(), "{field}");
    }
    let decoded: CollectedObservation = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded, record);
    admit_observation(&admitted, decoded).unwrap();
    for field in ["projection_bytes", "marker_identity", "parent_snapshot_ref"] {
        let mut bad = value.clone();
        bad["draft"][field] = true.into();
        assert!(serde_json::from_value::<CollectedObservation>(bad).is_err());
    }
    let mut missing = value;
    missing["draft"]
        .as_object_mut()
        .unwrap()
        .remove("source_sequence");
    assert!(serde_json::from_value::<CollectedObservation>(missing).is_err());
}

#[test]
fn canonical_projection_and_reference_boundaries_fail_closed() {
    let admitted = admit_profile(profile()).unwrap();
    let valid = observation(&admitted, 0, 0, 0, DrainState::Final, first_projection());
    for bytes in [
        b" { } ".to_vec(),
        b"{\"x\":0,\"x\":1}".to_vec(),
        b"[".to_vec(),
        vec![0; MAX_INLINE_PROJECTION_BYTES + 1],
    ] {
        let mut bad = valid.clone();
        bad.draft.projection_ref = projection_identity(&bytes);
        bad.draft.projection_bytes = Some(bytes);
        bad.draft.novelty_identity = novelty_identity(
            &admitted,
            &bad.draft.projection_ref,
            &bad.draft.logical_boundary_ref,
            &bad.draft.transition_class,
        );
        assert!(admit_observation(&admitted, bad).is_err());
    }
    let mut external = valid.clone();
    external.draft.projection_bytes = None;
    admit_observation(&admitted, external).unwrap();
    assert!(novelty_coverage_slot(&valid.draft.novelty_identity, 0, 0).is_err());
    assert!(novelty_coverage_slot("protocol-novelty:invalid", 0, 1).is_err());
    let highest = reference("protocol-novelty", 'f');
    const OVERFLOW_REGION_SIZE: usize = 4;
    assert!(novelty_coverage_slot(&highest, usize::MAX, OVERFLOW_REGION_SIZE).is_err());
}

#[test]
fn forged_profile_and_wrong_boundary_cannot_reuse_complete_evidence() {
    let admitted = admit_profile(profile()).unwrap();
    let records = vec![
        observation(&admitted, 0, 0, 0, DrainState::Final, first_projection()),
        observation(&admitted, 1, 0, 0, DrainState::Final, first_projection()),
    ];
    let boundary = &records[0].draft.logical_boundary_ref;
    let valid =
        assemble_cohort(&admitted, boundary, &records, ProjectionSupport::Available).unwrap();
    validate_cohort(&admitted, &valid).unwrap();
    let wrong = assemble_cohort(
        &admitted,
        &reference("logical-boundary", 'e'),
        &records,
        ProjectionSupport::Available,
    )
    .unwrap();
    assert_eq!(wrong.classification, CohortClassification::Incomplete);
    assert!(wrong.records.is_empty());
    let mut forged = admitted.clone();
    forged.profile.producers.clear();
    assert!(assemble_cohort(&forged, boundary, &records, ProjectionSupport::Available).is_err());
    let mut forged = valid;
    forged.records.clear();
    assert!(validate_cohort(&admitted, &forged).is_err());
}
