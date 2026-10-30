use chaoscontrol_smr::observation::guest::*;
use chaoscontrol_smr::observation::*;
use chaoscontrol_smr::*;

fn reference(value: &str) -> String {
    projection_ref(value.as_bytes())
}

fn protocol() -> ProtocolProfile {
    ProtocolProfile {
        schema: observation::PROFILE_SCHEMA.into(),
        protocol_ref: reference("protocol"),
        projection_schema_ref: reference("chain-json"),
        boundary_schema_ref: reference("index"),
        oracle_adapter_ref: reference("adapter"),
        independent_oracle_ref: reference("independent"),
        novelty_selection_ref: reference("all-projection-refs"),
        marker_policy: MarkerPolicy::Disabled,
        participants: (0..2)
            .map(|guest_id| Participant {
                participant_ref: reference(&format!("participant-{guest_id}")),
                producer_ref: reference(&format!("producer-{guest_id}")),
                guest_id,
                generation: 0,
            })
            .collect(),
        bounds: Bounds {
            records: 16,
            record_bytes: 4096,
            total_bytes: 65536,
            boundaries: 4,
            cohort_backlog: 4,
            oracle_work: 16,
        },
        non_claims: NON_CLAIMS.iter().map(|s| s.to_string()).collect(),
    }
}

fn workload() -> SmrWorkloadPlan {
    admit_profile(&SmrWorkloadProfile {
        schema: chaoscontrol_smr::PROFILE_SCHEMA.into(),
        profile_id: "cohort-fixture".into(),
        initial_state_ref: reference("initial"),
        observation_mode: ObservationMode::Lossless,
        bounds: WorkloadBounds {
            maximum_commands: 4,
            maximum_command_bytes: 128,
            maximum_clients: 2,
            maximum_concurrency: 2,
            maximum_virtual_progress: 100,
            maximum_trace_events: 16,
            maximum_fault_actions: 4,
            maximum_replay_events: 32,
            maximum_evidence_bytes: 65536,
            maximum_reduction_attempts: 8,
        },
        liveness: LivenessProfile {
            profile_id: "stable".into(),
            required_progress: 1,
            virtual_progress_horizon: 20,
        },
    })
    .unwrap()
}

struct Intake {
    sources: Vec<GuestSource>,
    records: Vec<ProtocolObservation>,
    drains: Vec<Drain>,
    execution: String,
}
impl MultiNodeGuestIntake for Intake {
    fn sources(&self) -> &[GuestSource] {
        &self.sources
    }
    fn execution_ref(&self) -> &str {
        &self.execution
    }
    fn observations(&self) -> &[ProtocolObservation] {
        &self.records
    }
    fn drains(&self) -> &[Drain] {
        &self.drains
    }
}

fn fixture() -> (ProtocolProfile, SmrWorkloadPlan, Intake) {
    let profile = protocol();
    let plan = workload();
    let records = profile
        .participants
        .iter()
        .map(|p| {
            let observation = ChainObservation {
                event_sequence: 1,
                profile_ref: plan.profile_ref.clone(),
                replica_id: p.participant_ref[7..].into(),
                command_index: 1,
                prior_digest: plan.genesis_digest.clone(),
                digest: chain_transition(&plan.profile_ref, 1, &plan.genesis_digest, b"command")
                    .unwrap(),
                application_state_ref: reference("state"),
            };
            let bytes = serde_json::to_vec(&observation).unwrap();
            ProtocolObservation {
                schema: SCHEMA.into(),
                profile_ref: profile.admit().unwrap(),
                protocol_ref: profile.protocol_ref.clone(),
                projection_schema_ref: profile.projection_schema_ref.clone(),
                participant_ref: p.participant_ref.clone(),
                producer_ref: p.producer_ref.clone(),
                guest_id: p.guest_id,
                generation: p.generation,
                source_sequence: 1,
                transition_class_ref: reference("committed"),
                boundary_ref: reference("boundary"),
                projection_ref: projection_ref(&bytes),
                projection: Some(bytes),
                scheduler_ref: reference("scheduler"),
                dropped: 0,
            }
        })
        .collect();
    let drains = profile
        .participants
        .iter()
        .map(|p| Drain {
            participant_ref: p.participant_ref.clone(),
            producer_ref: p.producer_ref.clone(),
            generation: p.generation,
            last_sequence: 1,
            dropped: 0,
            final_drain: true,
        })
        .collect();
    let sources = profile
        .participants
        .iter()
        .map(|p| GuestSource {
            guest_id: p.guest_id,
            executable_ref: reference("binary"),
            source_revision_ref: reference("source"),
            adapter_ref: reference("adapter"),
            launch_contract_ref: reference("launch"),
        })
        .collect();
    (
        profile,
        plan,
        Intake {
            sources,
            records,
            drains,
            execution: reference("execution-artifacts"),
        },
    )
}

fn cohort(p: &ProtocolProfile, i: &Intake) -> CohortResult {
    assemble_cohort(p, &reference("boundary"), &i.records, &i.drains).unwrap()
}
fn mutate_projection(record: &mut ProtocolObservation, f: impl FnOnce(&mut ChainObservation)) {
    let mut observation = serde_json::from_slice(record.projection.as_ref().unwrap()).unwrap();
    f(&mut observation);
    let bytes = serde_json::to_vec(&observation).unwrap();
    record.projection_ref = projection_ref(&bytes);
    record.projection = Some(bytes);
}

#[test]
fn complete_cohort_is_order_independent_but_not_semantic_proof() {
    let (p, plan, mut i) = fixture();
    let first = cohort(&p, &i);
    assert_eq!(first.completeness, Completeness::Complete);
    i.records.reverse();
    assert_eq!(first.cohort_ref, cohort(&p, &i).cohort_ref);
    assert_eq!(first.novelty_ref, cohort(&p, &i).novelty_ref);
    assert_eq!(
        validate_guest_history(&p, &reference("boundary"), &plan, &i)
            .unwrap()
            .history
            .verdict,
        SafetyVerdict::Pass
    );
    for claim in [
        "snapshot_backed_reproduced",
        "universal-protocol-correctness",
        "deterministic-replay",
    ] {
        assert!(validate_claim(EvidenceClass::GuestExecution, claim).is_err());
    }
    assert!(validate_claim(EvidenceClass::Instrumentation, "bounded-guest-observations").is_err());
}

#[test]
fn divergence_is_detected_even_when_runtime_claims_success() {
    let (p, plan, mut i) = fixture();
    mutate_projection(&mut i.records[1], |o| {
        o.application_state_ref = reference("wrong-state")
    });
    let report = validate_guest_history(&p, &reference("boundary"), &plan, &i).unwrap();
    assert_eq!(report.history.verdict, SafetyVerdict::Fail);
    assert!(report
        .history
        .violations
        .iter()
        .any(|v| v.class == SafetyViolationClass::Divergence));
}

#[test]
fn duplicate_gap_loss_and_drain_cannot_become_complete() {
    let (p, _, mut i) = fixture();
    i.records.push(i.records[0].clone());
    assert_eq!(cohort(&p, &i).completeness, Completeness::Conflicting);
    let (p, _, mut i) = fixture();
    i.records[0].source_sequence = 2;
    i.drains[0].last_sequence = 2;
    assert_eq!(cohort(&p, &i).completeness, Completeness::Incomplete);
    let (p, _, mut i) = fixture();
    i.drains[0].dropped = 1;
    assert_eq!(cohort(&p, &i).completeness, Completeness::Incomplete);
    i.drains[0].dropped = 0;
    i.drains[0].final_drain = false;
    assert_eq!(cohort(&p, &i).completeness, Completeness::Incomplete);
    i.records.remove(0);
    assert_eq!(cohort(&p, &i).completeness, Completeness::Incomplete);
}

#[test]
fn malformed_stale_unknown_and_overflow_are_not_pass() {
    let (p, _, mut i) = fixture();
    i.records[0].projection.as_mut().unwrap().push(0);
    assert_eq!(cohort(&p, &i).completeness, Completeness::Conflicting);
    let (p, _, mut i) = fixture();
    i.records[0].generation += 1;
    assert_eq!(cohort(&p, &i).completeness, Completeness::Unsupported);
    let (mut p, _, i) = fixture();
    p.bounds.record_bytes = 1;
    assert_eq!(cohort(&p, &i).completeness, Completeness::Unsupported);
    let (mut p, _, _) = fixture();
    p.participants.push(p.participants[0].clone());
    assert!(p.admit().is_err());
    p = protocol();
    p.independent_oracle_ref = p.oracle_adapter_ref.clone();
    assert!(p.admit().is_err());
    let mut json = serde_json::to_value(protocol()).unwrap();
    json["unknown"] = true.into();
    assert!(serde_json::from_value::<ProtocolProfile>(json).is_err());
}

#[test]
fn rollback_and_duplicate_application_remain_chain_failures() {
    let (p, plan, mut i) = fixture();
    let mut second = i.records[0].clone();
    second.source_sequence = 2;
    second.boundary_ref = reference("later-boundary");
    mutate_projection(&mut second, |o| {
        o.event_sequence = 2;
        o.command_index = 2;
        o.prior_digest = o.digest.clone();
        o.digest = chain_transition(&plan.profile_ref, 2, &o.prior_digest, b"second").unwrap();
        o.application_state_ref = reference("second-state");
    });
    let mut rollback = i.records[0].clone();
    rollback.source_sequence = 3;
    rollback.boundary_ref = reference("rollback-boundary");
    mutate_projection(&mut rollback, |o| o.event_sequence = 3);
    i.records.extend([second, rollback]);
    i.drains[0].last_sequence = 3;
    let result = validate_guest_history(&p, &reference("boundary"), &plan, &i).unwrap();
    assert_eq!(result.history.verdict, SafetyVerdict::Fail);
    assert!(result
        .history
        .violations
        .iter()
        .any(|v| v.class == SafetyViolationClass::Rollback));

    let (p, plan, mut i) = fixture();
    let mut duplicate = i.records[0].clone();
    duplicate.source_sequence = 2;
    duplicate.boundary_ref = reference("duplicate-application");
    mutate_projection(&mut duplicate, |o| {
        o.event_sequence = 2;
        o.application_state_ref = reference("applied-twice");
    });
    i.records.push(duplicate);
    i.drains[0].last_sequence = 2;
    assert_eq!(
        validate_guest_history(&p, &reference("boundary"), &plan, &i)
            .unwrap()
            .history
            .verdict,
        SafetyVerdict::Fail
    );
}

#[test]
fn stable_quorum_stall_is_liveness_not_safety() {
    let plan = workload();
    let facts = StabilizationFacts {
        profile_id: "stable".into(),
        quorum_available: true,
        lifecycle_ready: true,
        disruptive_faults_active: false,
        virtual_progress_start: 0,
        virtual_progress_end: 20,
        committed_before: 1,
        committed_after: 1,
    };
    assert_eq!(
        evaluate_liveness(&plan, Some(&facts)).unwrap().verdict,
        LivenessVerdict::Fail
    );
    let mut disrupted = facts;
    disrupted.disruptive_faults_active = true;
    assert_eq!(
        evaluate_liveness(&plan, Some(&disrupted)).unwrap().verdict,
        LivenessVerdict::NotEvaluated
    );
}

#[test]
fn malformed_chain_projection_and_forged_source_fail_before_oracle() {
    let (p, plan, mut i) = fixture();
    let bytes = b"not chain JSON".to_vec();
    i.records[0].projection_ref = projection_ref(&bytes);
    i.records[0].projection = Some(bytes);
    assert_eq!(cohort(&p, &i).completeness, Completeness::Complete);
    assert_eq!(
        validate_guest_history(&p, &reference("boundary"), &plan, &i)
            .unwrap_err()
            .class,
        "malformed"
    );
    let (p, plan, mut i) = fixture();
    i.sources[1].guest_id = i.sources[0].guest_id;
    assert_eq!(
        validate_guest_history(&p, &reference("boundary"), &plan, &i)
            .unwrap_err()
            .class,
        "guest-intake"
    );
    let (mut p, _, _) = fixture();
    p.schema = "stale-schema".into();
    assert_eq!(p.admit().unwrap_err().class, "unsupported");
    p = protocol();
    p.bounds.total_bytes = 0;
    assert_eq!(p.admit().unwrap_err().class, "bound");
}
