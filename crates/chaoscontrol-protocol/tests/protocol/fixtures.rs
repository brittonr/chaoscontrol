use chaoscontrol_protocol::protocol_observation::*;

pub const FIRST_VM_ID: u32 = 7;
pub const SECOND_VM_ID: u32 = 9;
pub const RECORD_LIMIT: u32 = 8;
pub const PARTICIPANT_LIMIT: u32 = 2;
pub const PROJECTION_LIMIT_BYTES: u32 = 512;
pub const TOTAL_PROJECTION_LIMIT_BYTES: u64 = 4_096;
pub const BOUNDARY_LIMIT: u32 = 4;
pub const BACKLOG_LIMIT: u32 = 16;
pub const ORACLE_WORK_LIMIT: u32 = 32;
pub const DIAGNOSTIC_LIMIT: u32 = 4;
pub const TERM_VALUE: u64 = 4;
pub const COVERAGE_REGION_START: usize = 8_192;
pub const COVERAGE_REGION_SIZE: usize = 1_024;

pub fn reference(prefix: &str, digit: char) -> String {
    format!("{prefix}:{}", digit.to_string().repeat(BLAKE3_HEX_BYTES))
}

pub fn profile() -> ProtocolObservationProfile {
    let participants = vec![reference("participant", '3'), reference("participant", '4')];
    ProtocolObservationProfile {
        schema: PROFILE_SCHEMA.into(),
        execution_ref: reference("execution", '7'),
        protocol_ref: reference("protocol", 'a'),
        projection_schema_ref: reference("projection-schema", 'b'),
        logical_boundary_schema_ref: reference("logical-boundary-schema", 'c'),
        cohort_ref: reference("cohort", 'd'),
        producers: vec![
            ProducerProfile {
                producer_ref: reference("producer", '1'),
                participant_ref: participants[0].clone(),
                process_ref: reference("process", '5'),
                vm_id: FIRST_VM_ID,
                admitted_generation: 0,
            },
            ProducerProfile {
                producer_ref: reference("producer", '2'),
                participant_ref: participants[1].clone(),
                process_ref: reference("process", '6'),
                vm_id: SECOND_VM_ID,
                admitted_generation: 0,
            },
        ],
        required_participants: participants,
        completion_rule: CompletionRule::AllRequiredParticipants,
        bounds: ProtocolObservationBounds {
            max_records_per_producer: RECORD_LIMIT,
            max_projection_bytes: PROJECTION_LIMIT_BYTES,
            max_total_projection_bytes: TOTAL_PROJECTION_LIMIT_BYTES,
            max_participants: PARTICIPANT_LIMIT,
            max_logical_boundaries: BOUNDARY_LIMIT,
            max_cohort_backlog: BACKLOG_LIMIT,
            max_oracle_work_items: ORACLE_WORK_LIMIT,
            max_diagnostic_refs: DIAGNOSTIC_LIMIT,
        },
        oracle: OracleAdapterProfile {
            adapter_ref: reference("oracle-adapter", 'e'),
            authority: OracleAuthority::ConsumerIndependent,
            requires_complete_cohort: true,
        },
        novelty_selectors: vec![
            NoveltySelector::ProjectionRef,
            NoveltySelector::LogicalBoundaryRef,
            NoveltySelector::TransitionClass,
        ],
        marker_policy: MarkerPolicy::OptionalDeclared,
        non_claims: REQUIRED_NON_CLAIMS
            .iter()
            .map(|value| value.to_string())
            .collect(),
    }
}

pub fn observation(
    profile: &AdmittedProfile,
    producer_index: usize,
    source_sequence: u64,
    source_loss_count: u64,
    drain_state: DrainState,
    projection_bytes: Vec<u8>,
) -> CollectedObservation {
    let producer = &profile.profile.producers[producer_index];
    let logical_boundary_ref = reference("logical-boundary", 'f');
    let projection_ref = projection_identity(&projection_bytes);
    let novelty_identity = novelty_identity(
        profile,
        &projection_ref,
        &logical_boundary_ref,
        "raft-term-transition",
    );
    bind_scheduler_position(
        ObservationDraft {
            schema: DRAFT_SCHEMA.into(),
            profile_ref: profile.profile_ref.clone(),
            protocol_ref: profile.profile.protocol_ref.clone(),
            cohort_ref: profile.profile.cohort_ref.clone(),
            producer_ref: producer.producer_ref.clone(),
            participant_ref: producer.participant_ref.clone(),
            process_ref: producer.process_ref.clone(),
            execution_ref: profile.profile.execution_ref.clone(),
            generation: producer.admitted_generation,
            source_sequence,
            source_loss_count,
            drain_state,
            transition_class: "raft-term-transition".into(),
            logical_boundary_ref,
            projection_schema_ref: profile.profile.projection_schema_ref.clone(),
            projection_ref,
            projection_bytes: Some(projection_bytes),
            novelty_identity,
            marker_identity: None,
            parent_snapshot_ref: None,
        },
        SchedulerPosition {
            schedule_state_ref: reference("schedule-state", '8'),
            vm_id: producer.vm_id,
            active_vcpu: 0,
            guest_exit_sequence: source_sequence,
        },
    )
    .expect("fixture binds scheduler position")
}

pub fn first_projection() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"leader": "node-a", "term": TERM_VALUE})).unwrap()
}

pub fn second_projection() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"leader": "node-b", "term": TERM_VALUE})).unwrap()
}

pub struct FixedOracle {
    pub adapter_ref: String,
    pub authority: OracleAuthority,
    pub verdict: ProtocolVerdict,
}
impl ProtocolOracle for FixedOracle {
    fn adapter_ref(&self) -> &str {
        &self.adapter_ref
    }
    fn projection_schema_ref(&self) -> &str {
        "projection-schema:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    }
    fn authority(&self) -> OracleAuthority {
        self.authority
    }
    fn evaluate(
        &self,
        _cohort: &CohortResult,
        _work_limit: u32,
    ) -> Result<OracleDecision, ProtocolObservationError> {
        Ok(OracleDecision {
            verdict: self.verdict,
            diagnostic_refs: Vec::new(),
            work_items: 1,
        })
    }
}
