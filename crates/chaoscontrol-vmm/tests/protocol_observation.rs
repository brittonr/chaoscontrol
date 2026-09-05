use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_protocol::protocol_observation::*;
use chaoscontrol_protocol::{HypercallPage, CMD_PROTOCOL_OBSERVATION, STATUS_ERROR, STATUS_OK};
use chaoscontrol_sdk::protocol_observation::*;

#[path = "protocol/compatibility.rs"]
mod compatibility;
#[path = "protocol/raft.rs"]
mod raft;
use raft::RaftOracle;
const TERM: u64 = 4;
const OVERFLOW_PAYLOAD_BYTES: u16 = u16::MAX;

struct HostTransport<'a> {
    engine: &'a mut FaultEngine,
    position: SchedulerPosition,
}
impl ObservationTransport for HostTransport<'_> {
    fn send(&mut self, payload: &[u8]) -> Result<(), ObservationEmitError> {
        let mut page = HypercallPage::zeroed();
        page.command = CMD_PROTOCOL_OBSERVATION;
        page.payload_len = u16::try_from(payload.len()).unwrap();
        page.payload[..payload.len()].copy_from_slice(payload);
        let (_, status) = self
            .engine
            .handle_hypercall_at(&page, Some(self.position.clone()));
        if status != STATUS_OK {
            return Err(ObservationEmitError::Transport(status));
        }
        Ok(())
    }
}

fn input(leader: &str, drain_state: DrainState) -> ObservationEmissionInput {
    let mut value = serde_json::json!({"leader": leader, "term": TERM, "runtime_pass": true});
    value.sort_all_objects();
    ObservationEmissionInput {
        transition_class: "term-complete".into(),
        logical_boundary_ref: format!("logical-boundary:{}", "a".repeat(BLAKE3_HEX_BYTES)),
        projection: ProjectionPayload::CanonicalJson(serde_json::to_vec(&value).unwrap()),
        drain_state,
        marker: None,
    }
}

fn emit(
    engine: &mut FaultEngine,
    profile: &AdmittedProfile,
    index: usize,
    leader: &str,
) -> ProtocolObservationEmitter {
    let producer = &profile.profile.producers[index];
    let mut emitter = ProtocolObservationEmitter::new(
        profile.clone(),
        &producer.producer_ref,
        &profile.profile.execution_ref,
    )
    .unwrap();
    let position = SchedulerPosition {
        vm_id: producer.vm_id,
        active_vcpu: 0,
        guest_exit_sequence: 0,
        schedule_state_ref: format!("schedule-state:{}", "b".repeat(BLAKE3_HEX_BYTES)),
    };
    emitter
        .emit_with(
            input(leader, DrainState::Final),
            &mut HostTransport { engine, position },
        )
        .unwrap();
    emitter
}

#[test]
fn guest_host_round_trip_and_independent_raft_oracle() {
    for (other_leader, expected) in [
        ("node-a", ProtocolVerdict::Pass),
        ("node-b", ProtocolVerdict::Fail),
    ] {
        let profile = RaftOracle::profile();
        let oracle = RaftOracle::new(&profile);
        let mut engine = FaultEngine::new(EngineConfig::default());
        engine.configure_protocol(profile.clone(), &oracle).unwrap();
        emit(&mut engine, &profile, 0, "node-a");
        emit(&mut engine, &profile, 1, other_leader);
        let boundary = input("node-a", DrainState::Final).logical_boundary_ref;
        let cohort = engine
            .protocol_collection()
            .cohort(&boundary, ProjectionSupport::Available)
            .unwrap();
        let result = run_consumer_oracle(&profile, &cohort, &oracle).unwrap();
        assert_eq!(result.decision.verdict, expected);
        assert!(engine.oracle().report().events.is_empty());
        let before = engine.protocol_collection().clone();
        let snapshot = engine.snapshot();
        engine.begin_run();
        engine.restore(&snapshot).unwrap();
        assert_eq!(*engine.protocol_collection(), before);
        assert_eq!(
            engine
                .protocol_collection()
                .cohort(&boundary, ProjectionSupport::Available)
                .unwrap(),
            cohort
        );
    }
}

#[test]
fn rejected_wire_suffix_and_unbound_host_frames_are_not_success() {
    let profile = RaftOracle::profile();
    let oracle = RaftOracle::new(&profile);
    let mut engine = FaultEngine::new(EngineConfig::default());
    engine.configure_protocol(profile.clone(), &oracle).unwrap();
    emit(&mut engine, &profile, 0, "node-a");
    emit(&mut engine, &profile, 1, "node-a");
    let mut page = HypercallPage::zeroed();
    page.command = CMD_PROTOCOL_OBSERVATION;
    page.payload_len = OVERFLOW_PAYLOAD_BYTES;
    assert_eq!(engine.handle_hypercall(&page), (0, STATUS_ERROR));
    assert_eq!(engine.protocol_collection().rejected(), 1);
    let cohort = engine
        .protocol_collection()
        .cohort(
            &input("node-a", DrainState::Final).logical_boundary_ref,
            ProjectionSupport::Available,
        )
        .unwrap();
    assert_eq!(cohort.classification, CohortClassification::Incomplete);
    assert!(run_consumer_oracle(&profile, &cohort, &oracle).is_err());
}

#[test]
fn emitter_accounts_transport_loss_and_denies_post_final_or_oversized_effects() {
    struct Reject;
    impl ObservationTransport for Reject {
        fn send(&mut self, _: &[u8]) -> Result<(), ObservationEmitError> {
            Err(ObservationEmitError::Transport(STATUS_ERROR))
        }
    }
    let profile = RaftOracle::profile();
    let producer = &profile.profile.producers[0];
    let mut emitter = ProtocolObservationEmitter::new(
        profile.clone(),
        &producer.producer_ref,
        &profile.profile.execution_ref,
    )
    .unwrap();
    let mut huge = input("node-a", DrainState::Open);
    huge.projection = ProjectionPayload::CanonicalJson(vec![0; MAX_INLINE_PROJECTION_BYTES + 1]);
    assert_eq!(
        emitter.emit_with(huge, &mut Reject),
        Err(ObservationEmitError::PayloadTooLarge)
    );
    assert_eq!(emitter.next_sequence(), 0);
    assert!(emitter
        .emit_with(input("node-a", DrainState::Open), &mut Reject)
        .is_err());
    assert_eq!(emitter.next_sequence(), 1);
    assert_eq!(emitter.loss_count(), 1);
    assert!(emitter
        .emit_with(input("node-a", DrainState::Final), &mut Reject)
        .is_err());
    assert_eq!(
        emitter.emit_with(input("node-a", DrainState::Open), &mut Reject),
        Err(ObservationEmitError::Closed)
    );
    assert_eq!(
        emitter.record_source_loss(),
        Err(ObservationEmitError::Closed)
    );
}

#[test]
fn fault_run_reset_cannot_erase_protocol_source_history_or_host_loss() {
    let profile = RaftOracle::profile();
    let oracle = RaftOracle::new(&profile);
    let mut engine = FaultEngine::new(EngineConfig::default());
    engine.configure_protocol(profile.clone(), &oracle).unwrap();
    emit(&mut engine, &profile, 0, "node-a");
    emit(&mut engine, &profile, 1, "node-a");
    let before = engine.protocol_observations().to_vec();
    let mut bad = HypercallPage::zeroed();
    bad.command = CMD_PROTOCOL_OBSERVATION;
    assert_eq!(engine.handle_hypercall(&bad), (0, STATUS_ERROR));
    engine.begin_run();
    assert_eq!(engine.protocol_collection().rejected(), 1);
    assert_eq!(engine.protocol_observations(), before);
    engine.start_fresh_run_at(chaoscontrol_fault::schedule::FaultSchedule::new(), 0);
    assert_eq!(engine.protocol_collection().rejected(), 1);
    assert_eq!(engine.protocol_observations(), before);
    engine
        .begin_counterfactual_run(chaoscontrol_fault::schedule::FaultSchedule::new())
        .unwrap();
    assert_eq!(engine.protocol_collection().rejected(), 1);
    assert_eq!(engine.protocol_observations(), before);
    let cohort = engine
        .protocol_collection()
        .cohort(
            &input("node-a", DrainState::Final).logical_boundary_ref,
            ProjectionSupport::Available,
        )
        .unwrap();
    assert_eq!(cohort.classification, CohortClassification::Incomplete);
    assert!(run_consumer_oracle(&profile, &cohort, &oracle).is_err());
}

#[test]
fn framed_suffix_and_reserved_event_spoof_never_enter_the_journal() {
    let profile = RaftOracle::profile();
    let oracle = RaftOracle::new(&profile);
    let mut engine = FaultEngine::new(EngineConfig::default());
    engine.configure_protocol(profile.clone(), &oracle).unwrap();
    emit(&mut engine, &profile, 0, "node-a");
    let before = engine.protocol_observations().to_vec();
    let accepted = before[0].clone();
    let json = serde_json::to_vec(&accepted.draft).unwrap();
    let mut page = HypercallPage::zeroed();
    page.command = CMD_PROTOCOL_OBSERVATION;
    let length =
        chaoscontrol_protocol::encode_payload(&mut page.payload, PROTOCOL_OBSERVATION_EVENT, &json)
            .unwrap();
    page.payload_len = u16::try_from(length + 1).unwrap();
    assert_eq!(
        engine.handle_hypercall_at(&page, Some(accepted.scheduler_position.clone())),
        (0, STATUS_ERROR)
    );
    assert_eq!(engine.protocol_observations(), before);
    assert_eq!(engine.protocol_collection().rejected(), 1);
    page.payload_len = u16::try_from(length).unwrap();
    page.command = chaoscontrol_protocol::CMD_LIFECYCLE_SEND_EVENT;
    assert_eq!(
        engine.handle_hypercall_at(&page, Some(accepted.scheduler_position)),
        (0, STATUS_ERROR)
    );
    assert_eq!(engine.protocol_observations(), before);
    assert!(engine.oracle().report().events.is_empty());
}

#[test]
fn self_reported_or_wrong_adapter_is_rejected_before_retention() {
    let profile = RaftOracle::profile();
    let mut oracle = RaftOracle::new(&profile);
    oracle.authority = OracleAuthority::RuntimeSelfReport;
    let mut engine = FaultEngine::new(EngineConfig::default());
    assert_eq!(
        engine.configure_protocol(profile.clone(), &oracle),
        Err(ProtocolObservationError::RuntimeSelfOracle)
    );
    oracle.authority = OracleAuthority::ConsumerIndependent;
    oracle.schema.clear();
    assert_eq!(
        engine.configure_protocol(profile.clone(), &oracle),
        Err(ProtocolObservationError::OracleMismatch)
    );
    assert!(engine.protocol_observations().is_empty());
}
