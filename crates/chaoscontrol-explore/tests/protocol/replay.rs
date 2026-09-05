//! Executed port-I/O continuation through the production controller and file store.
//! This fixture does not establish a Linux boot or protocol-semantic correctness.

#[path = "replay/fixture.rs"]
mod fixture;
#[path = "replay/guest.rs"]
mod guest;
#[path = "replay/machine.rs"]
mod machine;

use chaoscontrol_explore::{protocol_observation as session, snapshot_store};
use chaoscontrol_protocol::protocol_observation as protocol;
use snapshot_store::SnapshotStore;

const REPLAY_ATTEMPTS: usize = 2;

// r[verify chaoscontrol.protocol_observation.snapshot_binding]
#[test]
#[ignore = "requires /dev/kvm for an executed guest continuation"]
fn stored_parent_replays_executed_guest_observations() {
    let mut fixture = fixture::Fixture::new();
    assert!(fixture.store.has_snapshot(&fixture.parent));
    for _ in 0..REPLAY_ATTEMPTS {
        fixture.dirty_counter();
        let replayed = fixture.replay(machine::TICKS).unwrap();
        assert_eq!(replayed, fixture.expected);
        fixture.assert_counter(1);
        assert_eq!(fixture.controller.tick(), machine::TICKS);
        for index in 0..machine::VM_COUNT {
            assert!(fixture.controller.vm(index).io_exit_count() > 0);
            assert_eq!(
                fixture
                    .controller
                    .vm(index)
                    .fault_engine()
                    .protocol_collection()
                    .rejected(),
                0
            );
        }
    }
}

// r[verify chaoscontrol.protocol_observation.validation]
#[test]
#[ignore = "requires /dev/kvm for replay preflight mutation checks"]
fn invalid_replay_inputs_leave_the_running_state_unchanged() {
    let mut fixture = fixture::Fixture::new();
    fixture.dirty_counter();
    assert!(matches!(
        fixture.replay(0),
        Err(session::SessionError::Protocol(
            protocol::ProtocolObservationError::BoundExceeded("replay-ticks")
        ))
    ));
    fixture.assert_counter(fixture::DIRTY_COUNTER);

    let binding = fixture.binding.clone();
    fixture.binding.marker_identity.push('0');
    assert_marker_rejection(&mut fixture);
    fixture.binding = binding.clone();
    fixture.binding.cohort_identity.push('0');
    assert_marker_rejection(&mut fixture);
    fixture.binding = binding;

    let parent = fixture.parent.clone();
    fixture.parent.digest = snapshot_store::digest_bytes(b"stale parent");
    assert_marker_rejection(&mut fixture);
    fixture.parent = parent;

    let expected = fixture.expected.clone();
    fixture.expected = protocol::assemble_cohort(
        &fixture.profile,
        &fixture.boundary,
        &[expected.records[0].collected.clone()],
        protocol::ProjectionSupport::Available,
    )
    .unwrap();
    assert_eq!(
        fixture.expected.classification,
        protocol::CohortClassification::Incomplete
    );
    assert_marker_rejection(&mut fixture);
    fixture.expected = expected;

    let artifact_path = fixture.directory.path().join(&fixture.parent.path);
    let artifact_bytes = std::fs::read(&artifact_path).unwrap();
    std::fs::remove_file(&artifact_path).unwrap();
    assert!(matches!(
        fixture.replay(machine::TICKS),
        Err(session::SessionError::Snapshot(
            snapshot_store::SnapshotStoreError::Missing { .. }
        ))
    ));
    fixture.assert_counter(fixture::DIRTY_COUNTER);
    std::fs::write(&artifact_path, b"tampered snapshot").unwrap();
    assert!(matches!(
        fixture.replay(machine::TICKS),
        Err(session::SessionError::Snapshot(
            snapshot_store::SnapshotStoreError::DigestMismatch { .. }
        ))
    ));
    fixture.assert_counter(fixture::DIRTY_COUNTER);
    assert_eq!(fixture.controller.tick(), machine::TICKS);
    std::fs::write(&artifact_path, artifact_bytes).unwrap();
    assert_eq!(fixture.replay(machine::TICKS).unwrap(), fixture.expected);
    fixture.assert_counter(1);
}

// r[verify chaoscontrol.protocol_observation.snapshot_binding]
// r[verify chaoscontrol.protocol_observation.validation]
#[test]
#[ignore = "requires /dev/kvm for observed replay drift"]
fn a_restorable_parent_cannot_hide_an_incomplete_guest_cohort() {
    let mut fixture = fixture::Fixture::new();
    let parent = fixture.store.get_snapshot(&fixture.parent).unwrap();
    fixture.controller.restore_all(&parent).unwrap();
    fixture
        .controller
        .vm(0)
        .write_guest_memory(guest::FRAME_ADDRESS + fixture::FLAGS_OFFSET as u64, &[1])
        .unwrap();
    fixture.parent = fixture
        .store
        .put_snapshot(&fixture.controller.snapshot_all().unwrap(), 1)
        .unwrap();
    fixture.binding = protocol::bind_marker_snapshot(
        &fixture.profile,
        &fixture.expected,
        &fixture.marker,
        &fixture.binding.projection_ref,
        &session::snapshot_binding_reference(&fixture.parent).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        fixture.replay(machine::TICKS),
        Err(session::SessionError::Protocol(
            protocol::ProtocolObservationError::IdentityMismatch("protocol-replay")
        ))
    ));
    fixture.assert_counter(1);
    let actual = fixture
        .session
        .collect(&fixture.controller, &fixture.boundary)
        .unwrap();
    assert_eq!(
        actual.classification,
        protocol::CohortClassification::Incomplete
    );
    assert_eq!(
        fixture
            .controller
            .vm(0)
            .fault_engine()
            .protocol_collection()
            .rejected(),
        1
    );
}

#[test]
#[ignore = "requires /dev/kvm for the production kernel-loader rejection path"]
fn malformed_guest_image_is_not_a_replay_fixture() {
    let directory = tempfile::tempdir().unwrap();
    let kernel = directory.path().join("malformed.elf");
    std::fs::write(&kernel, b"not an ELF image").unwrap();
    assert!(matches!(
        chaoscontrol_vmm::controller::SimulationController::new(machine::config(&kernel)),
        Err(chaoscontrol_vmm::vm::VmError::KernelLoad { .. })
    ));
}

fn assert_marker_rejection(fixture: &mut fixture::Fixture) {
    assert!(matches!(
        fixture.replay(machine::TICKS),
        Err(session::SessionError::Protocol(
            protocol::ProtocolObservationError::MarkerMismatch
        ))
    ));
    fixture.assert_counter(fixture::DIRTY_COUNTER);
    assert_eq!(fixture.controller.tick(), machine::TICKS);
}
