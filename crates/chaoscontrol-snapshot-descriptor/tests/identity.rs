mod support;

use chaoscontrol_snapshot_descriptor::{
    descriptor_identity, preflight, validate_consumer_reference, validate_descriptor,
    validate_locator_sidecar, validate_restore_receipt, verify_content, DigestAlgorithm,
    LocatorHint, LocatorKind, LocatorSidecar, PreflightStatus,
};
use support::{
    chunked_descriptor, consumer_reference, destination, monolithic_descriptor, successful_receipt,
    PAYLOAD_BYTES,
};

#[test]
fn equivalent_json_projections_keep_one_descriptor_identity() {
    let descriptor = monolithic_descriptor();
    validate_descriptor(&descriptor).expect("fixture descriptor is valid");
    let compact = serde_json::to_string(&descriptor).expect("compact descriptor JSON");
    let pretty = serde_json::to_string_pretty(&descriptor).expect("pretty descriptor JSON");
    let compact_descriptor = serde_json::from_str(&compact).expect("decode compact descriptor");
    let pretty_descriptor = serde_json::from_str(&pretty).expect("decode pretty descriptor");
    assert_eq!(
        descriptor_identity(&compact_descriptor).expect("compact identity"),
        descriptor_identity(&pretty_descriptor).expect("pretty identity")
    );
}

#[test]
fn every_behavior_field_is_inside_descriptor_identity() {
    let descriptor = monolithic_descriptor();
    let baseline = descriptor_identity(&descriptor).expect("baseline identity");

    let mut changed = descriptor.clone();
    changed.runtime.scheduler_profile = "different-scheduler-v1".to_string();
    assert_ne!(
        descriptor_identity(&changed).expect("changed identity"),
        baseline
    );

    let mut changed = descriptor;
    changed.topology.memory_bytes = changed.topology.memory_bytes.saturating_add(1);
    assert_ne!(
        descriptor_identity(&changed).expect("changed memory identity"),
        baseline
    );
}

#[test]
fn monolithic_and_chunked_closures_are_content_bound() {
    let monolithic = monolithic_descriptor();
    assert!(verify_content(
        &monolithic.payload.logical_payload,
        PAYLOAD_BYTES
    ));
    assert_eq!(
        monolithic.payload.logical_payload.digest.algorithm,
        DigestAlgorithm::Sha256
    );

    let chunked = chunked_descriptor();
    validate_descriptor(&chunked).expect("chunked descriptor is valid");
    assert_eq!(
        chunked
            .payload
            .members
            .iter()
            .map(|member| member.content.length_bytes)
            .sum::<u64>(),
        chunked.payload.logical_payload.length_bytes
    );
}

#[test]
fn matching_destination_produces_an_ordered_restore_plan() {
    let descriptor = monolithic_descriptor();
    let decision = preflight(&descriptor, &destination()).expect("preflight runs");
    assert_eq!(decision.status, PreflightStatus::Admitted);
    assert!(decision.blockers.is_empty());
    assert!(decision.plan.is_some());
}

#[test]
fn detached_locators_do_not_change_descriptor_identity() {
    let descriptor = monolithic_descriptor();
    let descriptor_id = descriptor_identity(&descriptor).expect("descriptor identity");
    let file_sidecar = LocatorSidecar {
        descriptor_id: descriptor_id.clone(),
        hints: vec![LocatorHint {
            kind: LocatorKind::File,
            locator: "snapshots/fixture.snapshot.bin".to_string(),
        }],
    };
    let mirror_sidecar = LocatorSidecar {
        descriptor_id: descriptor_id.clone(),
        hints: vec![LocatorHint {
            kind: LocatorKind::Mirror,
            locator: "mirror:fixture-snapshot".to_string(),
        }],
    };
    validate_locator_sidecar(&file_sidecar).expect("file locator is bounded");
    validate_locator_sidecar(&mirror_sidecar).expect("mirror locator is bounded");
    assert_eq!(
        descriptor_identity(&descriptor).expect("identity after locator move"),
        descriptor_id
    );
}

#[test]
fn successful_restore_and_refs_only_consumer_are_admitted() {
    let descriptor = monolithic_descriptor();
    validate_restore_receipt(&successful_receipt(&descriptor)).expect("complete restore receipt");
    validate_consumer_reference(&consumer_reference(&descriptor)).expect("refs-only consumer");
}
