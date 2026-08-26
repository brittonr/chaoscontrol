mod support;

use chaoscontrol_snapshot_descriptor::{
    preflight, validate_consumer_reference, validate_descriptor, validate_restore_receipt,
    DisallowedConsumerClaim, PhaseStatus, PreflightStatus,
};
use serde_json::json;
use support::{
    chunked_descriptor, consumer_reference, destination, monolithic_descriptor, successful_receipt,
};

const FAILED_PHASE_COUNT: usize = 3;

// r[verify chaoscontrol.snapshot_descriptor.verification]
#[test]
fn incomplete_or_duplicated_cohort_is_rejected() {
    let mut missing_owner = monolithic_descriptor();
    missing_owner.state_owners.pop();
    assert_eq!(
        validate_descriptor(&missing_owner)
            .expect_err("missing state owner must fail")
            .code(),
        "state-owners"
    );

    let mut duplicate_device = monolithic_descriptor();
    let first = duplicate_device.topology.devices[0].clone();
    duplicate_device.topology.devices.push(first);
    assert_eq!(
        validate_descriptor(&duplicate_device)
            .expect_err("duplicate device must fail")
            .code(),
        "device-inventory"
    );
}

#[test]
fn stale_schema_and_unknown_profile_are_rejected() {
    let mut stale = monolithic_descriptor();
    stale.state_schema_version = stale.state_schema_version.saturating_add(1);
    assert_eq!(
        validate_descriptor(&stale)
            .expect_err("stale schema must fail")
            .code(),
        "state-schema-version"
    );

    let mut unknown = monolithic_descriptor();
    unknown.completeness_profile = "portable-anywhere".to_string();
    assert_eq!(
        validate_descriptor(&unknown)
            .expect_err("unknown profile must fail")
            .code(),
        "completeness-profile"
    );
}

#[test]
fn destination_drift_denies_before_restore() {
    let descriptor = monolithic_descriptor();
    let mut destination = destination();
    destination.topology.msr_indices.pop();
    let decision = preflight(&descriptor, &destination).expect("preflight produces denial");
    assert_eq!(decision.status, PreflightStatus::Denied);
    assert!(decision.plan.is_none());
    assert!(decision
        .blockers
        .iter()
        .any(|blocker| blocker.code == "msr-inventory-mismatch"));
}

#[test]
fn architecture_topology_device_and_runtime_drift_deny_preflight() {
    let descriptor = monolithic_descriptor();

    let mut architecture = destination();
    architecture.architecture = "aarch64".to_string();
    assert_denied(&descriptor, architecture, "architecture-mismatch");

    let mut topology = destination();
    topology.topology.vcpu_count = topology.topology.vcpu_count.saturating_add(1);
    assert_denied(&descriptor, topology, "vcpu-topology-mismatch");

    let mut device = destination();
    device.topology.devices[0].identity.base_address = device.topology.devices[0]
        .identity
        .base_address
        .saturating_add(1);
    assert_denied(&descriptor, device, "device-cohort-mismatch");

    let mut runtime = destination();
    runtime.runtime.runtime_build.hex.replace_range(..1, "f");
    assert_denied(&descriptor, runtime, "runtime-build-mismatch");
}

#[test]
fn reordered_and_tampered_chunks_are_rejected() {
    let mut reordered = chunked_descriptor();
    reordered.payload.members.swap(0, 1);
    assert_eq!(
        validate_descriptor(&reordered)
            .expect_err("reordered chunks must fail")
            .code(),
        "chunk-order"
    );

    let mut truncated = chunked_descriptor();
    truncated.payload.members[0].content.length_bytes = truncated.payload.members[0]
        .content
        .length_bytes
        .saturating_sub(1);
    assert_eq!(
        validate_descriptor(&truncated)
            .expect_err("truncated closure must fail")
            .code(),
        "chunk-length"
    );
}

#[test]
fn unknown_digest_algorithm_and_locator_in_descriptor_fail_decode() {
    let mut value = serde_json::to_value(monolithic_descriptor()).expect("descriptor JSON value");
    value["payload"]["logical_payload"]["digest"]["algorithm"] = json!("md5");
    assert!(
        serde_json::from_value::<chaoscontrol_snapshot_descriptor::SnapshotDescriptor>(value)
            .is_err()
    );

    let mut value = serde_json::to_value(monolithic_descriptor()).expect("descriptor JSON value");
    value["path"] = json!("/host/snapshot.bin");
    assert!(
        serde_json::from_value::<chaoscontrol_snapshot_descriptor::SnapshotDescriptor>(value)
            .is_err()
    );
}

#[test]
fn post_mutation_failure_requires_poison_and_cannot_claim_success() {
    let descriptor = monolithic_descriptor();
    let mut receipt = successful_receipt(&descriptor);
    receipt.completed = false;
    receipt.continuation = None;
    receipt.phases.truncate(FAILED_PHASE_COUNT);
    let failed = receipt.phases.last_mut().expect("retained failure phase");
    failed.status = PhaseStatus::Failed;
    failed.diagnostic = Some("injected restore failure".to_string());
    receipt.poisoned = false;
    assert_eq!(
        validate_restore_receipt(&receipt)
            .expect_err("post-mutation failure without poison must fail")
            .code(),
        "restore-poison"
    );

    receipt.poisoned = true;
    validate_restore_receipt(&receipt).expect("poisoned failure receipt is valid");
}

#[test]
fn consumer_authority_overreach_is_rejected() {
    let descriptor = monolithic_descriptor();
    let mut reference = consumer_reference(&descriptor);
    reference.disallowed_claims.extend([
        DisallowedConsumerClaim::RestoreAuthority,
        DisallowedConsumerClaim::WorldMerge,
    ]);
    assert_eq!(
        validate_consumer_reference(&reference)
            .expect_err("restore authority claim must fail")
            .code(),
        "consumer-authority-overreach"
    );
}

fn assert_denied(
    descriptor: &chaoscontrol_snapshot_descriptor::SnapshotDescriptor,
    destination: chaoscontrol_snapshot_descriptor::DestinationObservation,
    blocker_code: &str,
) {
    let decision = preflight(descriptor, &destination).expect("preflight must classify drift");
    assert_eq!(decision.status, PreflightStatus::Denied);
    assert!(decision
        .blockers
        .iter()
        .any(|blocker| blocker.code == blocker_code));
}
