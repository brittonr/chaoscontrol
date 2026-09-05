//! KVM-backed shared-page dispatch and snapshot retention fixtures.

use super::*;
use chaoscontrol_protocol::protocol_observation::*;
use chaoscontrol_protocol::{CMD_PROTOCOL_OBSERVATION, PAYLOAD_OFFSET, STATUS_ERROR, STATUS_OK};
use chaoscontrol_sdk::protocol_observation::*;

struct FixtureOracle(AdmittedProfile);
impl ProtocolOracle for FixtureOracle {
    fn adapter_ref(&self) -> &str {
        &self.0.profile.oracle.adapter_ref
    }
    fn projection_schema_ref(&self) -> &str {
        &self.0.profile.projection_schema_ref
    }
    fn authority(&self) -> OracleAuthority {
        OracleAuthority::ConsumerIndependent
    }
    fn evaluate(
        &self,
        _: &CohortResult,
        _: u32,
    ) -> Result<OracleDecision, ProtocolObservationError> {
        Err(ProtocolObservationError::OracleMismatch)
    }
}
struct Capture(Vec<u8>);
impl ObservationTransport for Capture {
    fn send(&mut self, bytes: &[u8]) -> Result<(), ObservationEmitError> {
        self.0 = bytes.to_vec();
        Ok(())
    }
}

#[test]
#[ignore = "requires /dev/kvm for VM-backed dispatch and snapshot restore"]
fn shared_page_snapshot_preserves_protocol_observations() {
    const COMMAND_OFFSET: usize = std::mem::offset_of!(HypercallPage, command);
    const FLAGS_OFFSET: usize = std::mem::offset_of!(HypercallPage, flags);
    const LENGTH_OFFSET: usize = std::mem::offset_of!(HypercallPage, payload_len);
    const STATUS_OFFSET: usize = std::mem::offset_of!(HypercallPage, status);
    let profile = decode_profile(include_bytes!(
        "../../../../contracts/protocol-observation/fixtures/valid.json"
    ))
    .unwrap();
    let producer = profile.profile.producers[0].clone();
    let mut vm = DeterministicVm::new(VmConfig {
        vm_id: producer.vm_id as usize,
        ..VmConfig::default()
    })
    .unwrap();
    vm.fault_engine
        .configure_protocol(profile.clone(), &FixtureOracle(profile.clone()))
        .unwrap();
    let mut emitter = ProtocolObservationEmitter::new(
        profile.clone(),
        &producer.producer_ref,
        &profile.profile.execution_ref,
    )
    .unwrap();
    let mut capture = Capture(Vec::new());
    emitter
        .emit_with(
            ObservationEmissionInput {
                transition_class: "final".into(),
                logical_boundary_ref: format!("logical-boundary:{}", "a".repeat(BLAKE3_HEX_BYTES)),
                projection: ProjectionPayload::CanonicalJson(b"{}".to_vec()),
                drain_state: DrainState::Final,
                marker: None,
            },
            &mut capture,
        )
        .unwrap();
    let mut page = [0_u8; HYPERCALL_PAGE_SIZE];
    page[COMMAND_OFFSET] = CMD_PROTOCOL_OBSERVATION;
    page[LENGTH_OFFSET..LENGTH_OFFSET + std::mem::size_of::<u16>()]
        .copy_from_slice(&u16::try_from(capture.0.len()).unwrap().to_le_bytes());
    page[PAYLOAD_OFFSET..PAYLOAD_OFFSET + capture.0.len()].copy_from_slice(&capture.0);
    vm.write_guest_memory(HYPERCALL_PAGE_ADDR, &page).unwrap();
    vm.handle_sdk_hypercall();
    assert_eq!(
        vm.read_guest_memory(HYPERCALL_PAGE_ADDR + STATUS_OFFSET as u64, 1)
            .unwrap(),
        [STATUS_OK]
    );
    let expected = vm.fault_engine.protocol_collection().clone();
    assert_eq!(
        expected.records()[0].scheduler_position.vm_id,
        producer.vm_id
    );
    assert_eq!(
        expected.records()[0].scheduler_position.schedule_state_ref,
        vm.scheduler.state_id().to_reference()
    );
    let snapshot = vm.snapshot().unwrap();
    page[FLAGS_OFFSET] = 1;
    vm.write_guest_memory(HYPERCALL_PAGE_ADDR, &page).unwrap();
    vm.handle_sdk_hypercall();
    assert_eq!(
        vm.read_guest_memory(HYPERCALL_PAGE_ADDR + STATUS_OFFSET as u64, 1)
            .unwrap(),
        [STATUS_ERROR]
    );
    assert_eq!(vm.fault_engine.protocol_collection().rejected(), 1);
    vm.restore(&snapshot).unwrap();
    assert_eq!(*vm.fault_engine.protocol_collection(), expected);
}
