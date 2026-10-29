use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const RELATIONS: [&str; 4] = ["PrecededBy", "NotPrecededBy", "FollowedBy", "NotFollowedBy"];

fn event(sequence: u64, tick: u64, kind: &str) -> EventEnvelope {
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick,
        vm_index: Some(0),
        process: Some("monitor".to_string()),
        thread: None,
        kind: kind.to_string(),
        text: json!({"label":kind}).to_string(),
        payload_digest: [2; 32],
        details: Some(json!({"phase":kind})),
        derived: BTreeMap::from([("kept".to_string(), "original".to_string())]),
    }
}

fn events() -> Vec<EventEnvelope> {
    let mut events: Vec<_> = [9, 0, 10, u64::MAX, 2, u64::MAX - 1, 5]
        .into_iter()
        .enumerate()
        .map(|(i, tick)| event(i as u64, tick, "candidate"))
        .collect();
    for tick in [0, 5, 10, u64::MAX, 1] {
        events.push(event(events.len() as u64, tick, "witness"));
    }
    events
}

fn filter(kind: &str) -> Value {
    json!({"Filter":{"input":"All","predicate":{"Kind":kind}}})
}

fn encoded(relation: &str, within: u64, input: Value, witness: Value) -> Value {
    json!({"Temporal":{"input":input,"witness":witness,"relation":relation,"within_ticks":within}})
}

fn query(relation: &str, within: u64) -> EventQuery {
    serde_json::from_value(encoded(
        relation,
        within,
        filter("candidate"),
        filter("witness"),
    ))
    .expect("temporal witness queries must be supported")
}

fn oracle(
    events: &[EventEnvelope],
    relation: &str,
    within: u64,
    candidate_kind: &str,
    witness_kind: &str,
) -> Vec<EventEnvelope> {
    let mut expected: Vec<_> = events
        .iter()
        .filter(|candidate| {
            if candidate.kind != candidate_kind {
                return false;
            }
            let exists = events
                .iter()
                .filter(|witness| witness.kind == witness_kind)
                .any(|witness| {
                    let distance = match relation {
                        "PrecededBy" | "NotPrecededBy" => candidate.tick.checked_sub(witness.tick),
                        _ => witness.tick.checked_sub(candidate.tick),
                    };
                    distance.is_some_and(|distance| distance > 0 && distance <= within)
                });
            if relation.starts_with("Not") {
                !exists
            } else {
                exists
            }
        })
        .cloned()
        .collect();
    expected.sort_by_key(|event| (event.tick, event.source_id, event.source_sequence));
    expected
}

// r[verify chaoscontrol.temporal_event_query.correlation]
#[test]
fn all_temporal_relations_match_independent_pairwise_oracle_at_tick_boundaries() {
    let mut events = events();
    let limits = EventQueryLimits::default();
    for _ in 0..2 {
        for relation in RELATIONS {
            for within in [0, 1, 3, 4, u64::MAX] {
                for (candidate_kind, witness_kind) in [
                    ("candidate", "witness"),
                    ("candidate", "absent"),
                    ("absent", "witness"),
                ] {
                    let query: EventQuery = serde_json::from_value(encoded(
                        relation,
                        within,
                        filter(candidate_kind),
                        filter(witness_kind),
                    ))
                    .unwrap();
                    assert_eq!(
                        evaluate(&query, &events, &limits).unwrap().rows,
                        oracle(&events, relation, within, candidate_kind, witness_kind),
                        "{relation} {within} {candidate_kind} {witness_kind}"
                    );
                }
            }
        }
        events.reverse();
    }
}

#[test]
fn temporal_queries_preserve_candidate_projections_and_share_witness_regex_admission() {
    let events = events();
    let input = json!({"Map":{"input":filter("candidate"),"field":"phase","pointer":"/phase"}});
    let witness = json!({"MapText":{"input":{"Filter":{"input":"All","predicate":{"Regex":{"field":"kind","pattern":"^witness$"}}}},"field":"witness_label","pointer":"/label"}});
    let original: EventQuery =
        serde_json::from_value(encoded("PrecededBy", 4, input, witness)).unwrap();
    let mut expected = oracle(&events, "PrecededBy", 4, "candidate", "witness");
    for event in &mut expected {
        event
            .derived
            .insert("phase".to_string(), "candidate".to_string());
    }
    assert_eq!(
        evaluate(&original, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        expected
    );
    let invalid = encoded(
        "NotPrecededBy",
        0,
        filter("absent"),
        json!({"Filter":{"input":"All","predicate":{"Regex":{"field":"text","pattern":"["}}}}),
    );
    let invalid = serde_json::from_value(invalid).unwrap();
    assert_eq!(
        evaluate(&invalid, &[], &EventQueryLimits::default()),
        Err(EventQueryError::InvalidRegex)
    );
}

#[test]
fn witness_evaluation_and_envelope_failures_cannot_hide_behind_empty_candidates_or_zero_windows() {
    let mut events = vec![event(0, 1, "witness")];
    events[0].text = "plain log".to_string();
    let witness = json!({"MapText":{"input":"All","field":"label","pointer":"/label"}});
    let invalid: EventQuery =
        serde_json::from_value(encoded("NotFollowedBy", 0, filter("absent"), witness)).unwrap();
    assert_eq!(
        evaluate(&invalid, &events, &EventQueryLimits::default()),
        Err(EventQueryError::InvalidJsonText)
    );
    events[0].details = Some(json!({"big":"x".repeat(MAX_DERIVED_BYTES/2)}));
    let first = json!({"Map":{"input":"All","field":"first","pointer":"/big"}});
    let second = json!({"Map":{"input":first,"field":"second","pointer":"/big"}});
    let invalid: EventQuery =
        serde_json::from_value(encoded("NotFollowedBy", 0, filter("absent"), second)).unwrap();
    assert_eq!(
        evaluate(&invalid, &events, &EventQueryLimits::default()),
        Err(EventQueryError::InvalidEvent)
    );
}

#[test]
fn temporal_queries_share_exact_work_structural_and_final_result_bounds() {
    let events = events();
    let original = query("PrecededBy", u64::MAX);
    let exact = EventQueryLimits {
        max_operations: 5 * events.len(),
        ..EventQueryLimits::default()
    };
    let expected = oracle(&events, "PrecededBy", u64::MAX, "candidate", "witness");
    assert_eq!(evaluate(&original, &events, &exact).unwrap().rows, expected);
    assert_eq!(
        evaluate(
            &original,
            &events,
            &EventQueryLimits {
                max_operations: exact.max_operations - 1,
                ..exact
            }
        ),
        Err(EventQueryError::OperationBound)
    );
    assert_eq!(
        evaluate(
            &original,
            &events,
            &EventQueryLimits {
                max_nodes: 4,
                ..exact
            }
        ),
        Err(EventQueryError::QueryBound)
    );
    assert_eq!(
        evaluate(
            &original,
            &events,
            &EventQueryLimits {
                max_depth: 1,
                ..exact
            }
        ),
        Err(EventQueryError::QueryDepth)
    );
    let limited = evaluate(
        &original,
        &events,
        &EventQueryLimits {
            max_result_rows: 1,
            ..exact
        },
    )
    .unwrap();
    assert_eq!(limited.rows, expected[..1]);
    assert!(limited.truncated);
    assert_eq!(limited.matched, expected.len() as u64);
    let bytes = serde_json::to_vec(&expected).unwrap().len();
    assert_eq!(
        evaluate(
            &original,
            &events,
            &EventQueryLimits {
                max_result_bytes: bytes,
                ..exact
            }
        )
        .unwrap()
        .rows,
        expected
    );
    assert_eq!(
        evaluate(
            &original,
            &events,
            &EventQueryLimits {
                max_result_bytes: bytes - 1,
                ..exact
            }
        ),
        Err(EventQueryError::ResultBound)
    );
    for (relation, within) in [
        (json!("During"), json!(5)),
        (json!("PrecededBy"), json!(-1)),
        (json!("FollowedBy"), json!("5")),
    ] {
        let mut bad = encoded("PrecededBy", 5, filter("candidate"), filter("witness"));
        bad["Temporal"]["relation"] = relation;
        bad["Temporal"]["within_ticks"] = within;
        assert!(serde_json::from_value::<EventQuery>(bad).is_err());
    }
}

#[test]
fn temporal_receipts_bind_both_inputs_windows_and_witnesses_with_explicit_moment_scoping() {
    let events = events();
    let at = |input| json!({"UpTo":{"input":input,"moment_tick":10}});
    let encoded_original = encoded(
        "PrecededBy",
        u64::MAX,
        at(filter("candidate")),
        at(filter("witness")),
    );
    let original: EventQuery = serde_json::from_value(encoded_original.clone()).unwrap();
    let limits = EventQueryLimits::default();
    let result = evaluate(&original, &events, &limits).unwrap();
    let expected: Vec<_> = oracle(&events, "PrecededBy", u64::MAX, "candidate", "witness")
        .into_iter()
        .filter(|event| event.tick <= 10)
        .collect();
    assert_eq!(result.rows, expected);
    let receipt = receipt_for(&original, &events, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(10));
    validate_receipt(&receipt, &original, &events, &result, &limits).unwrap();
    let mut changed = encoded_original.clone();
    changed["Temporal"]["within_ticks"] = json!(u64::MAX - 1);
    let changed: EventQuery = serde_json::from_value(changed).unwrap();
    assert_eq!(
        evaluate(&changed, &events, &limits).unwrap().rows,
        result.rows
    );
    assert_eq!(
        validate_receipt(&receipt, &changed, &events, &result, &limits),
        Err(EventQueryError::ReceiptDrift)
    );
    let mut changed = encoded_original.clone();
    changed["Temporal"]["witness"]["UpTo"]["input"] = json!({"Filter":{"input":"All","predicate":{"Regex":{"field":"kind","pattern":"^witness$"}}}});
    let changed: EventQuery = serde_json::from_value(changed).unwrap();
    assert_eq!(
        evaluate(&changed, &events, &limits).unwrap().rows,
        result.rows
    );
    assert_eq!(
        validate_receipt(&receipt, &changed, &events, &result, &limits),
        Err(EventQueryError::ReceiptDrift)
    );
    let mut changed = events.clone();
    changed[10].text = "changed excluded witness".to_string();
    assert_eq!(
        validate_receipt(&receipt, &original, &changed, &result, &limits),
        Err(EventQueryError::SourceDrift)
    );
    let mut forged = result.clone();
    forged.rows[0].text = "forged candidate".to_string();
    assert_eq!(
        validate_receipt(&receipt, &original, &events, &forged, &limits),
        Err(EventQueryError::ResultDrift)
    );
    let mut ambiguous = encoded_original;
    ambiguous["Temporal"]["witness"]["UpTo"]["moment_tick"] = json!(11);
    let ambiguous = serde_json::from_value(ambiguous).unwrap();
    assert_eq!(
        evaluate(&ambiguous, &events, &limits),
        Err(EventQueryError::AmbiguousMoment)
    );
    let pair = [event(0, 10, "candidate"), event(1, 12, "witness")];
    let outer: EventQuery = serde_json::from_value(at(encoded(
        "FollowedBy",
        2,
        filter("candidate"),
        filter("witness"),
    )))
    .unwrap();
    assert_eq!(
        evaluate(&outer, &pair, &limits).unwrap().rows,
        vec![pair[0].clone()]
    );
    let capped: EventQuery = serde_json::from_value(encoded(
        "FollowedBy",
        2,
        at(filter("candidate")),
        at(filter("witness")),
    ))
    .unwrap();
    assert_eq!(
        evaluate(&capped, &pair, &limits).unwrap().rows,
        Vec::<EventEnvelope>::new()
    );
}

#[test]
fn negative_empty_witness_preserves_first_duplicate_projection() {
    let mut events = Vec::new();
    let mut first = BTreeMap::new();
    for index in 0..96 {
        let position = (index * 17) % 11;
        let mut source = event(position, position % 3, "candidate");
        source
            .derived
            .insert("projection".to_string(), index.to_string());
        first.entry(position).or_insert_with(|| source.clone());
        events.push(source);
    }
    let mut expected: Vec<_> = first.into_values().collect();
    expected.sort_by_key(|event| (event.tick, event.source_id, event.source_sequence));
    let original = query("NotPrecededBy", 5);
    assert_eq!(
        evaluate(&original, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        expected
    );
}
