use super::*;

fn event(sequence: u64, tick: u64) -> EventEnvelope {
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.into(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Recording,
        source_sequence: sequence,
        tick,
        vm_index: Some(0),
        process: None,
        thread: None,
        kind: "serial_output".into(),
        text: format!("retained {tick}\n"),
        payload_digest: *blake3::hash(&tick.to_le_bytes()).as_bytes(),
        details: None,
        derived: BTreeMap::new(),
    }
}

fn input() -> WorkspaceTimelineInput {
    let events = vec![event(0, 10), event(1, 20), event(2, 30)];
    let moment = MomentContext {
        tick: 20,
        start_tick: 10,
        end_tick: 20,
        retained_start_tick: 0,
        retained_end_tick: 30,
        source_ids: source_names(&[[1; 32]]),
        session_id: None,
        checkpoint_id: None,
        branch_id: None,
    };
    let limits = EventQueryLimits {
        max_result_rows: 1,
        ..EventQueryLimits::default()
    };
    let query_result = evaluate_at_moment(&EventQuery::All, &events, &limits, &moment).unwrap();
    let receipt =
        workspace_query_receipt(&EventQuery::All, &events, &query_result, &limits, &moment)
            .unwrap();
    WorkspaceTimelineInput {
        facts: vec![
            TimelineFact {
                id: 1,
                parent: None,
                tick: 0,
                kind: "branch".into(),
                label: "root b3:retained".into(),
            },
            TimelineFact {
                id: 2,
                parent: Some(1),
                tick: 20,
                kind: "checkpoint".into(),
                label: "checkpoint b3:retained".into(),
            },
        ],
        events,
        query_receipt: receipt.query,
        query_result,
        moment,
        missing_artifacts: vec!["branch-2 checkpoint snapshot absent".into()],
        source_truncated: false,
        limits: TimelineLimits::default(),
        claim: TimelineClaim::RetainedFactsOnly,
    }
}

#[test]
fn historical_window_receipt_and_static_graph_preserve_gaps_and_truncation() {
    let input = input();
    assert_eq!(input.query_result.scanned, 3);
    assert_eq!(input.query_result.matched, 2);
    assert_eq!(input.query_result.rows[0].tick, 10);
    assert!(input.query_result.truncated);
    for format in ["markdown", "dot"] {
        let output = render_workspace_timeline(&input, format).unwrap();
        assert!(output.contains("truncated=true"));
        assert!(output.contains("Missing artifact: branch-2 checkpoint snapshot absent"));
        assert!(output.contains("horizon=10..=20"));
        assert!(output.contains("b3:010101"));
        assert!(output.contains("checkpoint"));
        let receipt = workspace_timeline_receipt(&input, format, &output).unwrap();
        assert_eq!(receipt.nodes, 4);
        assert_eq!(receipt.edges, 1);
        assert_eq!(receipt.rows, 1);
        validate_workspace_timeline_receipt(&receipt, &input, format, &output).unwrap();
        assert_eq!(render_workspace_timeline(&input, format).unwrap(), output);
        let mut forged = receipt.clone();
        forged.edges += 1;
        assert_eq!(
            validate_workspace_timeline_receipt(&forged, &input, format, &output).unwrap_err(),
            EventQueryError::TimelineReceiptDrift
        );
        assert!(workspace_timeline_receipt(&input, format, &(output + "forged")).is_err());
    }
    assert!(render_workspace_timeline(&input, "dot")
        .unwrap()
        .contains("n1 -> n2"));
}

#[test]
fn workspace_rejects_each_graph_bound_without_silently_dropping_input() {
    let input = input();
    let mut limited = input.clone();
    limited.limits.max_nodes = 3;
    assert_eq!(
        render_workspace_timeline(&limited, "dot").unwrap_err(),
        EventQueryError::TimelineBound
    );
    limited = input.clone();
    limited.facts.push(TimelineFact {
        id: 3,
        parent: Some(2),
        tick: 20,
        kind: "branch".into(),
        label: "child".into(),
    });
    limited.limits.max_edges = 1;
    assert_eq!(
        render_workspace_timeline(&limited, "dot").unwrap_err(),
        EventQueryError::TimelineBound
    );
    limited = input.clone();
    limited.limits.max_rows = 3;
    assert_eq!(
        render_workspace_timeline(&limited, "markdown").unwrap_err(),
        EventQueryError::RowBound
    );
    limited = input.clone();
    limited.limits.max_bytes = 1;
    assert_eq!(
        render_workspace_timeline(&limited, "dot").unwrap_err(),
        EventQueryError::TimelineByteBound
    );
    limited = input.clone();
    limited.facts[1].parent = Some(99);
    assert_eq!(
        render_workspace_timeline(&limited, "dot").unwrap_err(),
        EventQueryError::MissingTimelineParent
    );
    limited = input.clone();
    limited.facts[1].tick = 31;
    assert_eq!(
        render_workspace_timeline(&limited, "dot").unwrap_err(),
        EventQueryError::InvalidWindow
    );
    limited = input.clone();
    limited.facts[0].tick = 21;
    assert_eq!(
        render_workspace_timeline(&limited, "dot").unwrap_err(),
        EventQueryError::InvalidWindow
    );
}

#[test]
fn workspace_receipts_reject_moment_input_output_accounting_and_claim_drift() {
    let input = input();
    let receipt = workspace_query_receipt(
        &EventQuery::All,
        &input.events,
        &input.query_result,
        &input.query_receipt.limits,
        &input.moment,
    )
    .unwrap();
    validate_workspace_query_receipt(
        &receipt,
        &EventQuery::All,
        &input.events,
        &input.query_result,
        &input.query_receipt.limits,
        &input.moment,
    )
    .unwrap();
    let mut changed = input.moment.clone();
    changed.start_tick = 0;
    assert!(validate_workspace_query_receipt(
        &receipt,
        &EventQuery::All,
        &input.events,
        &input.query_result,
        &input.query_receipt.limits,
        &changed
    )
    .is_err());
    changed = input.moment.clone();
    changed.source_ids = source_names(&[[2; 32]]);
    assert_eq!(
        evaluate_at_moment(
            &EventQuery::All,
            &input.events,
            &input.query_receipt.limits,
            &changed
        )
        .unwrap_err(),
        EventQueryError::SourceDrift
    );
    let mut forged = receipt.clone();
    forged.output_digest = [9; 32];
    assert_eq!(
        validate_workspace_query_receipt(
            &forged,
            &EventQuery::All,
            &input.events,
            &input.query_result,
            &input.query_receipt.limits,
            &input.moment
        )
        .unwrap_err(),
        EventQueryError::ReceiptDrift
    );
    let mut forged_rows = input.query_result.clone();
    forged_rows.rows[0].text = "unretained output".into();
    assert_eq!(
        workspace_query_receipt(
            &EventQuery::All,
            &input.events,
            &forged_rows,
            &input.query_receipt.limits,
            &input.moment
        )
        .unwrap_err(),
        EventQueryError::ResultDrift
    );
    let mut missing = input.clone();
    missing.events.pop();
    assert!(render_workspace_timeline(&missing, "dot").is_err());
    let mut changed = input.clone();
    changed.events[2]
        .derived
        .insert("unused".into(), "drift".into());
    assert_eq!(
        render_workspace_timeline(&changed, "dot").unwrap_err(),
        EventQueryError::SourceDrift
    );
    assert!(serde_json::from_str::<TimelineClaim>("\"complete_history\"").is_err());
}

#[test]
fn source_admission_rejects_unknown_owner_and_unsupported_dlog_tag() {
    let mut unknown = event(0, 0);
    unknown.vm_index = None;
    assert_eq!(
        admit_event(&unknown).unwrap_err(),
        EventQueryError::UnknownOwner
    );
    use chaoscontrol_vmm::dlog::{DlogRecord, DlogTag};
    let mut record = DlogRecord::new(0, 999, 0, 0, DlogTag::IoOut, 0);
    record.tag = 0;
    let context = DlogContext {
        vm_index: 0,
        ticks: vec![DlogTickAnchor {
            source_sequence: 0,
            tick: 0,
        }],
    };
    assert_eq!(
        envelopes_from_dlog_bytes(&record.to_bytes(), 1, &context).unwrap_err(),
        EventQueryError::UnsupportedRecord
    );
}

#[test]
fn fault_controls_preserve_admission_and_suppression_without_promoting_stages() {
    use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
    use chaoscontrol_fault::faults::Fault;
    use chaoscontrol_fault::mechanisms::PauseControlRequest;
    use chaoscontrol_fault::outcomes::NANOSECONDS_PER_SIMULATION_TICK;
    use chaoscontrol_fault::schedule::FaultScheduleBuilder;
    let mut engine = FaultEngine::new(EngineConfig {
        seed: 42,
        schedule: Some(
            FaultScheduleBuilder::new()
                .at_ns(
                    NANOSECONDS_PER_SIMULATION_TICK,
                    Fault::ProcessKill { target: 0 },
                )
                .build(),
        ),
        ..EngineConfig::default()
    });
    engine.begin_run();
    engine.force_setup_complete();
    engine
        .request_pause_control(
            PauseControlRequest {
                requester: "fixture".into(),
                start_tick: 0,
                end_tick: 5,
            },
            0,
        )
        .unwrap();
    assert!(engine
        .poll_fault_attempts(NANOSECONDS_PER_SIMULATION_TICK)
        .unwrap()
        .is_empty());
    let ledger = engine.fault_outcomes();
    let events = envelopes_from_fault_controls([8; 32], ledger).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].tick, 0);
    assert_eq!(events[1].tick, 1);
    assert_eq!(events[1].source_sequence, 1);
    assert_eq!(events[1].source_kind, EventSourceKind::FaultControl);
    assert_eq!(events[1].process.as_deref(), Some("fault_engine"));
    assert_eq!(events[1].kind, "fault_control_selection_suppressed");
    assert!(ledger.events.is_empty());
    let mut corrupt = ledger.clone();
    corrupt.control_events[1].stage_event_count = 1;
    assert_eq!(
        envelopes_from_fault_controls([8; 32], &corrupt).unwrap_err(),
        EventQueryError::InvalidEvent
    );
}

#[test]
fn admitted_empty_sources_remain_bound_without_inventing_events() {
    let input = input();
    let mut moment = input.moment;
    moment.source_ids = source_names(&[[1; 32], [2; 32]]);
    let result =
        evaluate_at_moment(&EventQuery::All, &[], &EventQueryLimits::default(), &moment).unwrap();
    let receipt = workspace_query_receipt(
        &EventQuery::All,
        &[],
        &result,
        &EventQueryLimits::default(),
        &moment,
    )
    .unwrap();
    assert!(result.rows.is_empty());
    assert_eq!(receipt.moment.source_ids.len(), 2);
    validate_workspace_query_receipt(
        &receipt,
        &EventQuery::All,
        &[],
        &result,
        &EventQueryLimits::default(),
        &moment,
    )
    .unwrap();
    let mut omitted = moment.clone();
    omitted.source_ids.remove(0);
    assert_eq!(
        evaluate_at_moment(
            &EventQuery::All,
            &[event(0, 10)],
            &EventQueryLimits::default(),
            &omitted
        )
        .unwrap_err(),
        EventQueryError::SourceDrift
    );
    assert!(validate_workspace_query_receipt(
        &receipt,
        &EventQuery::All,
        &[],
        &result,
        &EventQueryLimits::default(),
        &omitted
    )
    .is_err());
}

#[test]
fn stage_adapter_retains_selected_applied_observed_ticks_and_full_evidence() {
    use chaoscontrol_fault::faults::Fault;
    use chaoscontrol_fault::outcomes::{
        fault_run_id, fault_schedule_id, transition_fault_outcome, FaultAttempt, FaultObservation,
        FaultObservationEffect, FaultObservationSubsystem, FaultOutcomeLedger, FaultPlanEffect,
        FaultStageKind, NANOSECONDS_PER_SIMULATION_TICK,
    };
    let fault = Fault::ProcessKill { target: 0 };
    let selected_at = 7 * NANOSECONDS_PER_SIMULATION_TICK;
    let schedule = fault_schedule_id([(selected_at, None, &fault)]);
    let attempt = FaultAttempt::new(
        fault_run_id(42, 1, schedule),
        1,
        schedule,
        0,
        selected_at,
        fault,
    );
    let mut ledger = transition_fault_outcome(
        &FaultOutcomeLedger::default(),
        Some(&attempt),
        attempt.id,
        FaultStageKind::Selected,
    )
    .unwrap();
    for stage in [
        FaultStageKind::Applicable {
            effect: FaultPlanEffect::ProcessKill { target: 0 },
        },
        FaultStageKind::Applied {
            effect: FaultPlanEffect::ProcessKill { target: 0 },
        },
        FaultStageKind::Observed {
            observation: FaultObservation::new(
                attempt.id,
                FaultObservationSubsystem::Process,
                0,
                FaultObservationEffect::ProcessSkipped,
            ),
        },
    ] {
        ledger = transition_fault_outcome(&ledger, None, attempt.id, stage).unwrap();
    }
    let deltas = vec![
        FaultRoundTraceDelta {
            tick: 7,
            event_start: 0,
            event_end: 2,
        },
        FaultRoundTraceDelta {
            tick: 8,
            event_start: 2,
            event_end: 3,
        },
        FaultRoundTraceDelta {
            tick: 9,
            event_start: 3,
            event_end: 4,
        },
    ];
    let envelopes = envelopes_from_fault_stages([7; 32], &ledger.events, &deltas, &ledger).unwrap();
    assert_eq!(
        envelopes.iter().map(|e| e.tick).collect::<Vec<_>>(),
        [7, 7, 8, 9]
    );
    assert_eq!(envelopes[2].kind, "fault_stage_applied");
    assert_eq!(envelopes[3].kind, "fault_stage_observed");
    assert_eq!(
        envelopes[3].details.as_ref().unwrap()["event"],
        serde_json::to_value(&ledger.events[3]).unwrap()
    );
    assert_eq!(
        envelopes_from_fault_stages([7; 32], &ledger.events, &[], &ledger).unwrap_err(),
        EventQueryError::AmbiguousTime
    );
    let mut overlap = deltas.clone();
    overlap[1].event_start = 1;
    assert_eq!(
        envelopes_from_fault_stages([7; 32], &ledger.events, &overlap, &ledger).unwrap_err(),
        EventQueryError::InvalidEvent
    );
    let mut forged_tick = deltas;
    forged_tick[0].tick = 6;
    assert_eq!(
        envelopes_from_fault_stages([7; 32], &ledger.events, &forged_tick, &ledger).unwrap_err(),
        EventQueryError::AmbiguousTime
    );
}

#[test]
fn application_suppression_is_a_control_fact_not_an_applied_stage() {
    use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
    use chaoscontrol_fault::faults::Fault;
    use chaoscontrol_fault::mechanisms::PauseControlRequest;
    use chaoscontrol_fault::schedule::FaultScheduleBuilder;
    let mut engine = FaultEngine::new(EngineConfig {
        seed: 42,
        schedule: Some(
            FaultScheduleBuilder::new()
                .at_ns(0, Fault::ProcessKill { target: 0 })
                .build(),
        ),
        ..EngineConfig::default()
    });
    engine.begin_run();
    engine.force_setup_complete();
    let attempt = engine.poll_fault_attempts(0).unwrap().remove(0);
    engine
        .request_pause_control(
            PauseControlRequest {
                requester: "fixture".into(),
                start_tick: 0,
                end_tick: 5,
            },
            0,
        )
        .unwrap();
    assert!(engine.suppress_fault_application(attempt.id, 0).unwrap());
    let ledger = engine.fault_outcomes();
    let controls = envelopes_from_fault_controls([9; 32], ledger).unwrap();
    assert_eq!(controls[1].kind, "fault_control_application_suppressed");
    assert_eq!(controls[1].tick, 0);
    assert_eq!(ledger.counters.selected, 1);
    assert_eq!(ledger.counters.applied, 0);
    assert_eq!(ledger.counters.observed, 0);
    let stages = envelopes_from_fault_stages(
        [8; 32],
        &ledger.events,
        &[FaultRoundTraceDelta {
            tick: 0,
            event_start: 0,
            event_end: ledger.events.len() as u64,
        }],
        ledger,
    )
    .unwrap();
    assert_eq!(stages[0].kind, "fault_stage_selected");
    assert_eq!(stages[1].kind, "fault_stage_rejected");
}

#[test]
fn markdown_untrusted_literals_are_inert_before_bounds_and_receipt_digest() {
    let mut input = input();
    let hostile =
        "<script>bad()</script> ![pixel](https://evil.test/p) www.evil.test user@evil.test | `x`";
    input.events[0].text = hostile.into();
    input.facts[1].label = hostile.into();
    input.missing_artifacts[0] = hostile.into();
    input.query_result = evaluate(
        &input.query_receipt.query,
        &input.events,
        &input.query_receipt.limits,
    )
    .unwrap();
    input.query_receipt = receipt_for(
        &input.query_receipt.query,
        &input.events,
        &input.query_result,
        &input.query_receipt.limits,
    )
    .unwrap();
    let markdown = render_workspace_timeline(&input, "markdown").unwrap();
    assert!(!markdown.contains("<script>"));
    assert!(!markdown.contains("![pixel]("));
    assert!(!markdown.contains("https://"));
    assert!(!markdown.contains("www.evil"));
    assert!(!markdown.contains("user@evil"));
    assert!(markdown.contains("&lt;script&gt;"));
    assert!(markdown.contains("\\|"));
    let receipt = workspace_timeline_receipt(&input, "markdown", &markdown).unwrap();
    assert_eq!(
        receipt.output_digest,
        *blake3::hash(markdown.as_bytes()).as_bytes()
    );
    validate_workspace_timeline_receipt(&receipt, &input, "markdown", &markdown).unwrap();
    input.limits.max_bytes = markdown.len();
    assert_eq!(
        render_workspace_timeline(&input, "markdown").unwrap(),
        markdown
    );
    input.limits.max_bytes -= 1;
    assert_eq!(
        render_workspace_timeline(&input, "markdown").unwrap_err(),
        EventQueryError::TimelineByteBound
    );
}

#[test]
fn source_truncation_is_visible_without_forging_query_accounting() {
    let mut input = input();
    let limits = EventQueryLimits::default();
    input.query_result = evaluate(&input.query_receipt.query, &input.events, &limits).unwrap();
    input.query_receipt = receipt_for(
        &input.query_receipt.query,
        &input.events,
        &input.query_result,
        &limits,
    )
    .unwrap();
    assert!(!input.query_result.truncated);
    input.source_truncated = true;
    input
        .missing_artifacts
        .push("frontier trace stop missing".into());
    let output = render_workspace_timeline(&input, "markdown").unwrap();
    assert!(output.contains("truncated=true query_truncated=false source_truncated=true"));
    let receipt = workspace_timeline_receipt(&input, "markdown", &output).unwrap();
    assert!(receipt.truncated && receipt.source_truncated);
    assert!(!receipt.query_receipt.truncated);
    validate_workspace_timeline_receipt(&receipt, &input, "markdown", &output).unwrap();
    input.source_truncated = false;
    assert!(validate_workspace_timeline_receipt(&receipt, &input, "markdown", &output).is_err());
}
