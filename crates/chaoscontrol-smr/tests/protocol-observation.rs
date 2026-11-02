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

fn enable_declared_markers(profile: &mut ProtocolProfile, intake: &mut Intake) {
    profile.marker_policy = MarkerPolicy::Declared;
    let profile_ref = profile.admit().unwrap();
    for record in &mut intake.records {
        record.profile_ref = profile_ref.clone();
    }
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

// r[verify chaoscontrol.protocol_observation.validation]
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

// r[verify chaoscontrol.protocol_observation.validation]
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

// r[verify chaoscontrol.protocol_observation.validation]
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

// r[verify chaoscontrol.protocol_observation.validation]
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
    let mut p = protocol();
    p.oracle_adapter_ref = "adapter".into();
    assert_eq!(p.admit().unwrap_err().class, "reference-digest");
    let mut p = protocol();
    p.oracle_adapter_ref = String::new();
    assert!(p.admit().is_err());
}

// r[verify chaoscontrol.protocol_observation.validation]
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

// r[verify chaoscontrol.protocol_observation.validation]
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

// r[verify chaoscontrol.protocol_observation.snapshot_binding]
// r[verify chaoscontrol.protocol_observation.validation]
#[test]
fn marker_binding_binds_complete_cohort_state_without_widening_claims() {
    let (mut profile, _, mut intake) = fixture();
    enable_declared_markers(&mut profile, &mut intake);
    let complete = cohort(&profile, &intake);
    assert_eq!(complete.completeness, Completeness::Complete);
    let binding = MarkerBinding::bind(
        &profile,
        &reference("declared-marker"),
        &intake.records[0],
        &complete,
        &reference("restorable-parent"),
    )
    .unwrap();
    assert_eq!(binding.cohort_completeness, Completeness::Complete);
    assert!(binding.complete_protocol_state());
    let identity = binding
        .admit(&profile, &intake.records[0], &complete)
        .unwrap();
    assert!(identity.starts_with("blake3:"));

    let (disabled, _, other) = fixture();
    let other_cohort = cohort(&disabled, &other);
    assert_eq!(
        MarkerBinding::bind(
            &disabled,
            &reference("declared-marker"),
            &other.records[0],
            &other_cohort,
            &reference("restorable-parent"),
        )
        .unwrap_err()
        .class,
        "unsupported"
    );
}

// r[verify chaoscontrol.protocol_observation.snapshot_binding]
// r[verify chaoscontrol.protocol_observation.validation]
#[test]
fn marker_binding_never_completes_an_incomplete_cohort() {
    let (mut profile, _, mut intake) = fixture();
    enable_declared_markers(&mut profile, &mut intake);
    intake.records.remove(0);
    let incomplete = cohort(&profile, &intake);
    assert_ne!(incomplete.completeness, Completeness::Complete);
    let binding = MarkerBinding::bind(
        &profile,
        &reference("declared-marker"),
        &intake.records[0],
        &incomplete,
        &reference("restorable-parent"),
    )
    .unwrap();
    assert_eq!(binding.cohort_completeness, incomplete.completeness);
    assert!(!binding.complete_protocol_state());

    let mut upgraded = binding.clone();
    upgraded.cohort_completeness = Completeness::Complete;
    assert_eq!(
        upgraded
            .admit(&profile, &intake.records[0], &incomplete)
            .unwrap_err()
            .class,
        "conflicting"
    );
}

// r[verify chaoscontrol.protocol_observation.snapshot_binding]
// r[verify chaoscontrol.protocol_observation.validation]
#[test]
fn marker_binding_rejects_boundary_projection_snapshot_and_identity_drift() {
    let (mut profile, _, mut intake) = fixture();
    enable_declared_markers(&mut profile, &mut intake);
    let complete = cohort(&profile, &intake);
    let binding = MarkerBinding::bind(
        &profile,
        &reference("declared-marker"),
        &intake.records[0],
        &complete,
        &reference("restorable-parent"),
    )
    .unwrap();
    for mutate in [
        |b: &mut MarkerBinding| b.boundary_ref = reference("other-boundary"),
        |b: &mut MarkerBinding| b.projection_ref = reference("other-projection"),
        |b: &mut MarkerBinding| b.cohort_ref = reference("other-cohort"),
    ] {
        let mut drifted = binding.clone();
        mutate(&mut drifted);
        assert_eq!(
            drifted
                .admit(&profile, &intake.records[0], &complete)
                .unwrap_err()
                .class,
            "conflicting"
        );
    }
    // Marker and snapshot refs are opaque consumer identities: a different
    // well-formed digest is a different binding, not a cohort conflict.
    let identity = binding
        .admit(&profile, &intake.records[0], &complete)
        .unwrap();
    let mut rebound = binding.clone();
    rebound.marker_ref = reference("other-marker");
    rebound.parent_snapshot_ref = reference("other-snapshot");
    assert_ne!(
        rebound
            .admit(&profile, &intake.records[0], &complete)
            .unwrap(),
        identity
    );
    for mutate in [
        |b: &mut MarkerBinding| b.marker_ref = "not-a-digest".into(),
        |b: &mut MarkerBinding| b.parent_snapshot_ref = "not-a-digest".into(),
    ] {
        let mut malformed = binding.clone();
        mutate(&mut malformed);
        assert_eq!(
            malformed
                .admit(&profile, &intake.records[0], &complete)
                .unwrap_err()
                .class,
            "reference-digest"
        );
    }
    assert!(MarkerBinding::bind(
        &profile,
        "not-a-digest",
        &intake.records[0],
        &complete,
        &reference("restorable-parent"),
    )
    .is_err());
    assert!(MarkerBinding::bind(
        &profile,
        &reference("declared-marker"),
        &intake.records[0],
        &complete,
        "not-a-digest",
    )
    .is_err());
    let mut elsewhere = intake.records[0].clone();
    elsewhere.boundary_ref = reference("elsewhere");
    assert_eq!(
        MarkerBinding::bind(
            &profile,
            &reference("declared-marker"),
            &elsewhere,
            &complete,
            &reference("restorable-parent"),
        )
        .unwrap_err()
        .class,
        "conflicting"
    );
}

struct ConsumerOracle {
    identity: String,
    verdict: OracleVerdict,
    calls: std::cell::Cell<usize>,
}

impl ConsumerOracle {
    fn new(profile: &ProtocolProfile, verdict: OracleVerdict) -> Self {
        Self {
            identity: profile.independent_oracle_ref.clone(),
            verdict,
            calls: std::cell::Cell::new(0),
        }
    }
}

impl ProtocolOracle for ConsumerOracle {
    fn identity(&self) -> &str {
        &self.identity
    }

    fn evaluate(&self, cohort: &CohortResult) -> SmrResult<OracleResult> {
        self.calls.set(self.calls.get() + 1);
        Ok(OracleResult {
            schema: ORACLE_RESULT_SCHEMA.into(),
            oracle_ref: self.identity.clone(),
            cohort_ref: cohort.cohort_ref.clone(),
            verdict: self.verdict,
            diagnostics: Vec::new(),
        })
    }
}

// r[verify chaoscontrol.protocol_observation.oracle_boundary]
// r[verify chaoscontrol.protocol_observation.validation]
#[test]
fn oracle_boundary_rejects_self_reports_and_never_invokes_partial_cohorts() {
    let (profile, _, intake) = fixture();
    let complete = cohort(&profile, &intake);
    let oracle = ConsumerOracle::new(&profile, OracleVerdict::Pass);
    let result = evaluate_oracle(&profile, &complete, &oracle).unwrap();
    assert_eq!(result.verdict, OracleVerdict::Pass);
    assert_eq!(oracle.calls.get(), 1);
    assert!(result.admit(&profile, &complete).is_ok());

    let mut self_oracle = ConsumerOracle::new(&profile, OracleVerdict::Pass);
    self_oracle.identity = profile.oracle_adapter_ref.clone();
    assert_eq!(
        evaluate_oracle(&profile, &complete, &self_oracle)
            .unwrap_err()
            .class,
        "unsupported"
    );
    assert_eq!(self_oracle.calls.get(), 0);

    let (profile, _, mut intake) = fixture();
    intake.records.remove(0);
    let incomplete = cohort(&profile, &intake);
    let oracle = ConsumerOracle::new(&profile, OracleVerdict::Pass);
    let result = evaluate_oracle(&profile, &incomplete, &oracle).unwrap();
    assert_eq!(result.verdict, OracleVerdict::Incomplete);
    assert_eq!(
        oracle.calls.get(),
        0,
        "partial facts must not reach the oracle"
    );
    assert!(
        !result.admit(&profile, &complete).is_ok()
            || incomplete.completeness != Completeness::Complete
    );
}

// r[verify chaoscontrol.protocol_observation.evidence]
// r[verify chaoscontrol.protocol_observation.validation]
#[test]
fn receipts_bind_admitted_facts_and_refuse_incomplete_passes() {
    let (profile, _, intake) = fixture();
    let complete = cohort(&profile, &intake);
    let oracle = ConsumerOracle::new(&profile, OracleVerdict::Pass);
    let result = evaluate_oracle(&profile, &complete, &oracle).unwrap();
    let receipt = ProtocolReceipt::build(
        &profile,
        &complete,
        &result,
        None,
        vec![reference("scheduler")],
        vec![reference("fault")],
        vec![reference("replay")],
    )
    .unwrap();
    assert!(receipt.passed());
    let identity = receipt.admit(&profile, &complete).unwrap();
    assert!(identity.starts_with("blake3:"));

    let mut upgraded = receipt.clone();
    upgraded.completeness = Completeness::Incomplete;
    assert_eq!(
        upgraded.admit(&profile, &complete).unwrap_err().class,
        "conflicting"
    );

    let (profile, _, mut intake) = fixture();
    intake.records.remove(0);
    let incomplete = cohort(&profile, &intake);
    let oracle = ConsumerOracle::new(&profile, OracleVerdict::Pass);
    let result = evaluate_oracle(&profile, &incomplete, &oracle).unwrap();
    let receipt = ProtocolReceipt::build(
        &profile,
        &incomplete,
        &result,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    assert!(!receipt.passed());
    let mut forged = receipt.clone();
    forged.oracle_verdict = OracleVerdict::Pass;
    assert_eq!(
        forged.admit(&profile, &incomplete).unwrap_err().class,
        "incomplete"
    );

    let mut missing = receipt.clone();
    missing.non_claims.pop();
    assert_eq!(
        missing.admit(&profile, &incomplete).unwrap_err().class,
        "claim"
    );
}

// r[verify chaoscontrol.protocol_observation.snapshot_binding]
// r[verify chaoscontrol.protocol_observation.evidence]
#[test]
fn marker_bindings_flow_into_receipts_without_upgrading_completeness() {
    let (mut profile, _, mut intake) = fixture();
    enable_declared_markers(&mut profile, &mut intake);
    let complete = cohort(&profile, &intake);
    let binding = MarkerBinding::bind(
        &profile,
        &reference("declared-marker"),
        &intake.records[0],
        &complete,
        &reference("restorable-parent"),
    )
    .unwrap();
    let oracle = ConsumerOracle::new(&profile, OracleVerdict::Fail);
    let result = evaluate_oracle(&profile, &complete, &oracle).unwrap();
    let receipt = ProtocolReceipt::build(
        &profile,
        &complete,
        &result,
        Some(&binding),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(
        receipt.marker_ref.as_deref(),
        Some(binding.marker_ref.as_str())
    );
    assert_eq!(
        receipt.parent_snapshot_ref.as_deref(),
        Some(binding.parent_snapshot_ref.as_str())
    );
    assert!(!receipt.passed(), "a failing oracle result is never a pass");
}

// r[verify chaoscontrol.protocol_observation.cohort]
// r[verify chaoscontrol.protocol_observation.novelty]
// r[verify chaoscontrol.protocol_observation.validation]
#[test]
fn unknown_participants_and_unstable_novelty_selection_are_not_pass() {
    let (profile, _, mut intake) = fixture();
    intake.records[0].participant_ref = reference("unknown-participant");
    assert_eq!(
        cohort(&profile, &intake).completeness,
        Completeness::Unsupported
    );

    let (profile, _, intake) = fixture();
    let baseline = cohort(&profile, &intake);
    let mut drifted = intake.records[0].clone();
    let bytes = b"different projection bytes".to_vec();
    drifted.projection_ref = projection_ref(&bytes);
    drifted.projection = Some(bytes);
    let mut records = intake.records.clone();
    records[0] = drifted;
    let drifted_cohort =
        assemble_cohort(&profile, &reference("boundary"), &records, &intake.drains).unwrap();
    assert_eq!(drifted_cohort.completeness, Completeness::Complete);
    assert_ne!(baseline.novelty_ref, drifted_cohort.novelty_ref);

    let mut reselect = profile.clone();
    reselect.novelty_selection_ref = reference("different-novelty-selection");
    let reselect_ref = reselect.admit().unwrap();
    let mut reselected = intake.records.clone();
    for record in &mut reselected {
        record.profile_ref = reselect_ref.clone();
    }
    let reselected_cohort = assemble_cohort(
        &reselect,
        &reference("boundary"),
        &reselected,
        &intake.drains,
    )
    .unwrap();
    assert_eq!(reselected_cohort.completeness, Completeness::Complete);
    assert_ne!(baseline.novelty_ref, reselected_cohort.novelty_ref);
}

// r[verify chaoscontrol.protocol_observation.evidence]
// r[verify chaoscontrol.protocol_observation.validation]
#[test]
fn operator_status_reports_coverage_gaps_results_and_markers() {
    let (profile, _, intake) = fixture();
    let complete = cohort(&profile, &intake);
    let oracle = ConsumerOracle::new(&profile, OracleVerdict::Pass);
    let result = evaluate_oracle(&profile, &complete, &oracle).unwrap();
    let status =
        protocol_status(&profile, &complete, &intake.records, Some(&result), None).unwrap();
    assert_eq!(status.completeness, Completeness::Complete);
    assert_eq!(status.participants_required, 2);
    assert_eq!(status.participants_present, 2);
    assert!(status.missing_participants.is_empty());
    assert!(status.sequence_gaps.is_empty());
    assert_eq!(status.oracle_verdict, Some(OracleVerdict::Pass));
    assert_eq!(status.marker_reachable, None);
    assert_eq!(status.novelty_ref, complete.novelty_ref);

    let (profile, _, mut intake) = fixture();
    intake.records.remove(0);
    let incomplete = cohort(&profile, &intake);
    let status = protocol_status(&profile, &incomplete, &intake.records, None, None).unwrap();
    assert_eq!(status.completeness, Completeness::Incomplete);
    assert_eq!(status.participants_present, 1);
    assert_eq!(status.missing_participants.len(), 1);
    assert!(!status.blockers.is_empty());
}

// r[verify chaoscontrol.protocol_observation.cohort]
// r[verify chaoscontrol.protocol_observation.validation]
#[test]
fn operator_status_lists_sequence_gaps_as_blockers() {
    let (profile, _, mut intake) = fixture();
    let mut gap = intake.records[0].clone();
    gap.source_sequence = 3;
    intake.records.push(gap);
    let result = cohort(&profile, &intake);
    assert_eq!(result.completeness, Completeness::Incomplete);
    let status = protocol_status(&profile, &result, &intake.records, None, None).unwrap();
    assert_eq!(status.sequence_gaps.len(), 1);
    assert!(status.sequence_gaps[0].ends_with("#2"));
    assert!(!status.gaps_truncated);
}
