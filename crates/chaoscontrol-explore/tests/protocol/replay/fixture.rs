use super::{guest, machine};
use chaoscontrol_explore::{protocol_observation as session, snapshot_store};
use chaoscontrol_protocol::{self as wire, branch_marker, protocol_observation as protocol};
use chaoscontrol_sdk::protocol_observation as sdk;
use chaoscontrol_vmm::controller;
use snapshot_store::SnapshotStore;

pub const DIRTY_COUNTER: u64 = 17;
const PROJECTION: &[u8] = b"{}";
const COUNTER_BYTES: usize = std::mem::size_of::<u64>();
const COMMAND_OFFSET: usize = std::mem::offset_of!(wire::HypercallPage, command);
const LENGTH_OFFSET: usize = std::mem::offset_of!(wire::HypercallPage, payload_len);
pub const FLAGS_OFFSET: usize = std::mem::offset_of!(wire::HypercallPage, flags);

pub struct Oracle(protocol::AdmittedProfile);
impl protocol::ProtocolOracle for Oracle {
    fn adapter_ref(&self) -> &str {
        &self.0.profile.oracle.adapter_ref
    }
    fn projection_schema_ref(&self) -> &str {
        &self.0.profile.projection_schema_ref
    }
    fn authority(&self) -> protocol::OracleAuthority {
        protocol::OracleAuthority::ConsumerIndependent
    }
    fn evaluate(
        &self,
        cohort: &protocol::CohortResult,
        budget: u32,
    ) -> Result<protocol::OracleDecision, protocol::ProtocolObservationError> {
        let work_items = u32::try_from(cohort.records.len()).unwrap();
        assert!(work_items <= budget);
        let matches = cohort
            .records
            .iter()
            .all(|record| record.collected.draft.projection_bytes.as_deref() == Some(PROJECTION));
        Ok(protocol::OracleDecision {
            verdict: if matches {
                protocol::ProtocolVerdict::Pass
            } else {
                protocol::ProtocolVerdict::Fail
            },
            diagnostic_refs: Vec::new(),
            work_items,
        })
    }
}

struct Capture(Vec<u8>);
impl sdk::ObservationTransport for Capture {
    fn send(&mut self, payload: &[u8]) -> Result<(), sdk::ObservationEmitError> {
        assert!(payload.len() <= wire::PAYLOAD_MAX);
        self.0 = payload.to_vec();
        Ok(())
    }
}

pub struct Fixture {
    pub profile: protocol::AdmittedProfile,
    pub marker: branch_marker::BranchMarker,
    pub boundary: String,
    pub session: session::Session<Oracle>,
    pub controller: controller::SimulationController,
    pub store: snapshot_store::FileSnapshotStore,
    pub parent: snapshot_store::ReplayParentSnapshotRef,
    pub expected: protocol::CohortResult,
    pub binding: protocol::MarkerSnapshotBinding,
    pub directory: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let kernel = directory.path().join("protocol-guest.elf");
        std::fs::write(&kernel, guest::image()).unwrap();
        let mut profile = protocol::decode_profile(include_bytes!(
            "../../../../../contracts/protocol-observation/fixtures/valid.json"
        ))
        .unwrap()
        .profile;
        for (index, producer) in profile.producers.iter_mut().enumerate() {
            producer.vm_id = u32::try_from(index).unwrap();
        }
        let profile = protocol::admit_profile(profile).unwrap();
        let boundary = format!(
            "logical-boundary:{}",
            "a".repeat(protocol::BLAKE3_HEX_BYTES)
        );
        let projection_ref = protocol::projection_identity(PROJECTION);
        let marker = branch_marker::BranchMarker::new(
            "protocol-replay",
            "final",
            "fixture",
            serde_json::json!({"projection_ref": projection_ref}),
            None,
            Some(boundary.clone()),
        )
        .unwrap();
        let session = session::Session::new(profile.clone(), Oracle(profile.clone())).unwrap();
        let mut controller =
            controller::SimulationController::new(machine::config(&kernel)).unwrap();
        session.configure(&mut controller).unwrap();
        for index in 0..machine::VM_COUNT {
            let frame = frame(&profile, index, &boundary, &marker);
            controller
                .vm(index)
                .write_guest_memory(guest::FRAME_ADDRESS, &frame)
                .unwrap();
        }
        let store = snapshot_store::FileSnapshotStore::new(directory.path());
        let parent = store
            .put_snapshot(&controller.snapshot_all().unwrap(), 1)
            .unwrap();
        eprintln!("protocol fixture: parent stored, begin guest continuation");
        controller.run(machine::TICKS).unwrap();
        eprintln!("protocol fixture: guest continuation returned");
        let expected = session.collect(&controller, &boundary).unwrap();
        assert_eq!(
            expected.classification,
            protocol::CohortClassification::Complete,
            "cohort={expected:#?}, machine={:?}",
            (0..machine::VM_COUNT)
                .map(|index| {
                    let vm = controller.vm(index);
                    (
                        vm.exit_count(),
                        vm.io_exit_count(),
                        vm.read_guest_memory(guest::COUNTER_ADDRESS, COUNTER_BYTES)
                            .unwrap(),
                        vm.read_guest_memory(wire::HYPERCALL_PAGE_ADDR, wire::PAYLOAD_OFFSET)
                            .unwrap(),
                    )
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(expected.records.len(), machine::VM_COUNT);
        let binding = protocol::bind_marker_snapshot(
            &profile,
            &expected,
            &marker,
            &projection_ref,
            &session::snapshot_binding_reference(&parent).unwrap(),
        )
        .unwrap();
        let fixture = Self {
            profile,
            marker,
            boundary,
            session,
            controller,
            store,
            parent,
            expected,
            binding,
            directory,
        };
        fixture.assert_counter(1);
        fixture
    }

    pub fn replay(&mut self, ticks: u64) -> Result<protocol::CohortResult, session::SessionError> {
        eprintln!("protocol fixture: begin stored-parent replay");
        self.session.replay(
            &mut self.controller,
            &self.store,
            session::ReplayRequest {
                parent: &self.parent,
                expected: &self.expected,
                binding: &self.binding,
                ticks,
            },
        )
    }

    pub fn dirty_counter(&self) {
        for index in 0..machine::VM_COUNT {
            self.controller
                .vm(index)
                .write_guest_memory(guest::COUNTER_ADDRESS, &DIRTY_COUNTER.to_le_bytes())
                .unwrap();
        }
        self.assert_counter(DIRTY_COUNTER);
    }

    pub fn assert_counter(&self, expected: u64) {
        for index in 0..machine::VM_COUNT {
            let bytes = self
                .controller
                .vm(index)
                .read_guest_memory(guest::COUNTER_ADDRESS, COUNTER_BYTES)
                .unwrap();
            assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), expected);
            let registers = self.controller.vm(index).read_vcpu_registers(0).unwrap();
            assert!(guest::stop_range().contains(&registers.rip));
        }
    }
}

fn frame(
    profile: &protocol::AdmittedProfile,
    index: usize,
    boundary: &str,
    marker: &branch_marker::BranchMarker,
) -> Vec<u8> {
    let mut emitter = sdk::ProtocolObservationEmitter::new(
        profile.clone(),
        &profile.profile.producers[index].producer_ref,
        &profile.profile.execution_ref,
    )
    .unwrap();
    let mut capture = Capture(Vec::new());
    emitter
        .emit_with(
            sdk::ObservationEmissionInput {
                transition_class: "final".into(),
                logical_boundary_ref: boundary.into(),
                projection: sdk::ProjectionPayload::CanonicalJson(PROJECTION.to_vec()),
                drain_state: protocol::DrainState::Final,
                marker: Some(sdk::MarkerContext {
                    marker_identity: marker.identity.clone(),
                    parent_snapshot_ref: None,
                }),
            },
            &mut capture,
        )
        .unwrap();
    let mut page = vec![0_u8; wire::HYPERCALL_PAGE_SIZE];
    page[COMMAND_OFFSET] = wire::CMD_PROTOCOL_OBSERVATION;
    page[LENGTH_OFFSET..LENGTH_OFFSET + std::mem::size_of::<u16>()]
        .copy_from_slice(&u16::try_from(capture.0.len()).unwrap().to_le_bytes());
    page[wire::PAYLOAD_OFFSET..wire::PAYLOAD_OFFSET + capture.0.len()].copy_from_slice(&capture.0);
    page
}
