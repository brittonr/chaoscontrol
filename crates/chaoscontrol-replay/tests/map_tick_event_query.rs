use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn event(sequence: u64, tick: u64, kind: &str) -> EventEnvelope {
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick,
        vm_index: Some(0),
        process: None,
        thread: None,
        kind: kind.to_string(),
        text: "not JSON; tick=456".to_string(),
        payload_digest: [9; 32],
        details: Some(json!({"tick":321})),
        derived: BTreeMap::from([
            ("seen".to_string(), "obsolete".to_string()),
            ("tick".to_string(), "999".to_string()),
            ("other".to_string(), "keep".to_string()),
        ]),
    }
}
fn query(input: Value, field: &str) -> EventQuery {
    serde_json::from_value(json!({"MapTick":{"input":input,"field":field}}))
        .expect("MapTick must be supported")
}
fn annotate(mut row: EventEnvelope, field: &str) -> EventEnvelope {
    row.derived.insert(field.to_string(), row.tick.to_string());
    row
}

// r[verify chaoscontrol.map_tick.projection]
#[test]
fn canonical_ticks_remain_exact_despite_payload_shadows_permutations_and_duplicates() {
    let ticks = [0, 0, 9007199254740992, 9007199254740993, u64::MAX];
    let mut rows: Vec<_> = ticks
        .into_iter()
        .enumerate()
        .map(|(i, tick)| event(i as u64, tick, "metric"))
        .collect();
    let expected: Vec<_> = rows
        .iter()
        .cloned()
        .map(|row| annotate(row, "seen"))
        .collect();
    let mut duplicate = rows[0].clone();
    duplicate
        .derived
        .insert("other".to_string(), "later".to_string());
    rows.reverse();
    rows.push(duplicate);
    assert_eq!(
        evaluate(
            &query(json!("All"), "seen"),
            &rows,
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        expected
    );
    let source = event(0, u64::MAX, "metric");
    for field in ["kind", "text", "process", "source_kind", "tick"] {
        assert_eq!(
            evaluate(
                &query(json!("All"), field),
                std::slice::from_ref(&source),
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            vec![annotate(source.clone(), field)]
        );
    }
}

#[test]
fn canonical_tick_projections_drive_past_witness_elapsed_time_selection() {
    let rows = [
        event(0, 1, "error"),
        event(1, 2, "restart"),
        event(2, 8, "error"),
    ];
    let input = json!({"WithLastValue":{"input":{"MapTick":{"input":{"Filter":{"input":"All","predicate":{"Kind":"error"}}},"field":"observed_tick"}},"witness":{"MapTick":{"input":{"Filter":{"input":"All","predicate":{"Kind":"restart"}}},"field":"restart_tick"}},"field":"previous_tick","witness_field":"restart_tick","within_ticks":10}});
    let delta = json!({"MapDifference":{"input":input,"field":"elapsed","minuend":"observed_tick","subtrahend":"previous_tick"}});
    let q:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":delta,"predicate":{"FieldNumber":{"field":"elapsed","comparison":"GreaterThan","value":5}}}},"fields":["elapsed"]}})).unwrap();
    let mut expected = rows[2].clone();
    expected.derived = BTreeMap::from([("elapsed".to_string(), "6.0".to_string())]);
    assert_eq!(
        evaluate(&q, &rows, &EventQueryLimits::default())
            .unwrap()
            .rows,
        vec![expected]
    );
    let mut wide = event(0, u64::MAX, "metric");
    wide.derived
        .insert("previous_tick".to_string(), (u64::MAX - 1).to_string());
    let q:EventQuery=serde_json::from_value(json!({"MapDifference":{"input":query(json!("All"),"seen"),"field":"elapsed","minuend":"seen","subtrahend":"previous_tick"}})).unwrap();
    let mut expected = annotate(wide.clone(), "seen");
    expected
        .derived
        .insert("elapsed".to_string(), "0.0".to_string());
    assert_eq!(
        evaluate(&q, &[wide], &EventQueryLimits::default())
            .unwrap()
            .rows,
        vec![expected]
    );
}

#[test]
fn canonical_lookup_still_wins_and_numeric_filters_can_select_projected_ticks() {
    let source = event(0, 7, "metric");
    let limits = EventQueryLimits::default();
    let q:EventQuery=serde_json::from_value(json!({"Filter":{"input":query(json!("All"),"kind"),"predicate":{"FieldNumber":{"field":"kind","comparison":"Equal","value":7}}}})).unwrap();
    assert_eq!(
        evaluate(&q, std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        Vec::<EventEnvelope>::new()
    );
    let q: EventQuery = serde_json::from_value(
        json!({"Filter":{"input":query(json!("All"),"kind"),"predicate":{"Kind":"metric"}}}),
    )
    .unwrap();
    assert_eq!(
        evaluate(&q, std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![annotate(source.clone(), "kind")]
    );
    let q:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":query(json!("All"),"seen"),"predicate":{"FieldNumber":{"field":"seen","comparison":"GreaterThan","value":6}}}},"fields":["seen"]}})).unwrap();
    let mut expected = source.clone();
    expected.derived = BTreeMap::from([("seen".to_string(), "7".to_string())]);
    assert_eq!(
        evaluate(&q, &[source], &limits).unwrap().rows,
        vec![expected]
    );
}

#[test]
fn names_and_children_are_admitted_even_without_rows() {
    let limits = EventQueryLimits::default();
    for invalid in [
        String::new(),
        "a\nb".to_string(),
        "é".repeat(MAX_FIELD_BYTES / 2) + "x",
    ] {
        assert_eq!(
            evaluate(&query(json!("All"), &invalid), &[], &limits),
            Err(EventQueryError::InvalidField)
        );
    }
    let exact = "é".repeat(MAX_FIELD_BYTES / 2);
    let source = event(0, u64::MAX, "metric");
    assert_eq!(
        evaluate(
            &query(json!("All"), &exact),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(source, &exact)]
    );
    assert_eq!(
        evaluate(
            &query(
                json!({"MapText":{"input":"All","field":"bad","pointer":"not-a-pointer"}}),
                "seen"
            ),
            &[],
            &limits
        ),
        Err(EventQueryError::InvalidPointer)
    );
    assert_eq!(
        evaluate(&query(json!("All"), "seen"), &[], &limits)
            .unwrap()
            .rows,
        Vec::<EventEnvelope>::new()
    );
}

#[test]
fn generated_and_intermediate_capacity_cannot_be_hidden_by_narrowing_or_deduplication() {
    let limits = EventQueryLimits::default();
    let q = query(json!("All"), "projected");
    let narrow = |input| EventQuery::Narrow {
        input: Box::new(input),
        fields: vec!["projected".to_string()],
    };
    let mut source = event(0, u64::MAX, "metric");
    source.derived.insert("pad".to_string(), String::new());
    let used: usize = source.derived.iter().map(|(k, v)| k.len() + v.len()).sum();
    source.derived.get_mut("pad").unwrap().push_str(
        &"x".repeat(MAX_DERIVED_BYTES - "projected".len() - u64::MAX.to_string().len() - used),
    );
    assert_eq!(
        evaluate(&q, std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![annotate(source.clone(), "projected")]
    );
    source.derived.get_mut("pad").unwrap().push('x');
    assert_eq!(
        evaluate(
            &narrow(q.clone()),
            &[event(0, u64::MAX, "metric"), source],
            &limits
        ),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, u64::MAX, "metric");
    while source.derived.len() < MAX_DERIVED_FIELDS {
        source
            .derived
            .insert(format!("f{}", source.derived.len()), String::new());
    }
    assert_eq!(
        evaluate(
            &query(json!("All"), "seen"),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(source.clone(), "seen")]
    );
    assert_eq!(
        evaluate(&narrow(q), std::slice::from_ref(&source), &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let input = json!({"Map":{"input":"All","field":"overflow","pointer":"/tick"}});
    assert_eq!(
        evaluate(&narrow(query(input, "seen")), &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, 0, "metric");
    source.derived.get_mut("seen").unwrap().clear();
    let used: usize = source.derived.iter().map(|(k, v)| k.len() + v.len()).sum();
    source
        .derived
        .get_mut("seen")
        .unwrap()
        .push_str(&"x".repeat(MAX_DERIVED_BYTES - used));
    assert_eq!(
        evaluate(
            &query(json!("All"), "seen"),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(source.clone(), "seen")]
    );
    let input = json!({"Map":{"input":"All","field":"overflow","pointer":"/tick"}});
    assert_eq!(
        evaluate(&narrow(query(input, "seen")), &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
}

#[test]
fn shared_limits_moments_and_receipts_bind_canonical_tick_projection() {
    let rows = [event(0, 0, "metric"), event(1, 1, "metric")];
    let q = query(json!("All"), "seen");
    let exact = EventQueryLimits {
        max_nodes: 2,
        max_depth: 1,
        max_scanned_events: 2,
        max_operations: 4,
        ..EventQueryLimits::default()
    };
    let result = evaluate(&q, &rows, &exact).unwrap();
    let bytes = serde_json::to_vec(&result.rows).unwrap().len();
    assert_eq!(
        evaluate(
            &q,
            &rows,
            &EventQueryLimits {
                max_result_bytes: bytes,
                ..exact
            }
        )
        .unwrap(),
        result
    );
    for (limits, error) in [
        (
            EventQueryLimits {
                max_operations: 3,
                ..exact
            },
            EventQueryError::OperationBound,
        ),
        (
            EventQueryLimits {
                max_nodes: 1,
                ..exact
            },
            EventQueryError::QueryBound,
        ),
        (
            EventQueryLimits {
                max_scanned_events: 1,
                ..exact
            },
            EventQueryError::ScanBound,
        ),
        (
            EventQueryLimits {
                max_result_bytes: bytes - 1,
                ..exact
            },
            EventQueryError::ResultBound,
        ),
    ] {
        assert_eq!(evaluate(&q, &rows, &limits), Err(error));
    }
    let nested = EventQuery::UpTo {
        input: Box::new(q.clone()),
        moment_tick: 1,
    };
    assert_eq!(
        evaluate(
            &nested,
            &rows,
            &EventQueryLimits {
                max_nodes: 3,
                max_operations: 6,
                ..exact
            }
        ),
        Err(EventQueryError::QueryDepth)
    );
    let prefix = evaluate(
        &q,
        &rows,
        &EventQueryLimits {
            max_result_rows: 1,
            ..exact
        },
    )
    .unwrap();
    assert_eq!(prefix.rows, result.rows[..1]);
    assert_eq!(prefix.matched, 2);
    assert!(prefix.truncated);
    let limits = EventQueryLimits::default();
    let input = json!({"UpTo":{"input":"All","moment_tick":0}});
    let q = query(input.clone(), "seen");
    let result = evaluate(&q, &rows, &limits).unwrap();
    let receipt = receipt_for(&q, &rows, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(0));
    validate_receipt(&receipt, &q, &rows, &result, &limits).unwrap();
    for changed in [query(input, "other"), query(json!("All"), "seen")] {
        assert!(validate_receipt(&receipt, &changed, &rows, &result, &limits).is_err());
    }
    let mut drift = rows.clone();
    drift[1].text = "unselected canonical drift".to_string();
    assert!(validate_receipt(&receipt, &q, &drift, &result, &limits).is_err());
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("seen".to_string(), "9".to_string());
    assert!(validate_receipt(&receipt, &q, &rows, &forged, &limits).is_err());
    let mixed: EventQuery = serde_json::from_value(
        json!({"Union":{"inputs":[q,{"UpTo":{"input":"All","moment_tick":1}}]}}),
    )
    .unwrap();
    assert_eq!(
        evaluate(&mixed, &rows, &limits),
        Err(EventQueryError::AmbiguousMoment)
    );
}
