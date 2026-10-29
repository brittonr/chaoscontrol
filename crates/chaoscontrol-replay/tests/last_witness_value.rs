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
        process: Some("monitor".to_string()),
        thread: None,
        kind: kind.to_string(),
        text: format!("{kind}-{sequence}"),
        payload_digest: [2; 32],
        details: Some(json!({"first":"first","second":"second","value":20})),
        derived: BTreeMap::from([("last".to_string(), "stale".to_string())]),
    }
}
fn filter(kind: &str) -> Value {
    json!({"Filter":{"input":"All","predicate":{"Kind":kind}}})
}
fn encoded(input: Value, witness: Value, field: &str, witness_field: &str, within: u64) -> Value {
    json!({"WithLastValue":{"input":input,"witness":witness,"field":field,"witness_field":witness_field,"within_ticks":within}})
}
fn query(
    input: Value,
    witness: Value,
    field: &str,
    witness_field: &str,
    within: u64,
) -> EventQuery {
    serde_json::from_value(encoded(input, witness, field, witness_field, within))
        .expect("WithLastValue must be supported")
}
fn last(within: u64) -> EventQuery {
    query(
        filter("candidate"),
        filter("witness"),
        "last",
        "text",
        within,
    )
}
fn fill_to(row: &mut EventEnvelope, bytes: usize) {
    row.derived.insert("pad".to_string(), String::new());
    let used: usize = row.derived.iter().map(|(k, v)| k.len() + v.len()).sum();
    row.derived
        .get_mut("pad")
        .unwrap()
        .push_str(&"x".repeat(bytes - used));
}

// r[verify chaoscontrol.last_witness_value.annotation]
#[test]
fn nearest_strict_witness_matches_pairwise_oracle_at_tick_boundaries_and_ties() {
    let mut events: Vec<_> = [0, 1, 2, 5, 6, 10, u64::MAX - 1, u64::MAX]
        .into_iter()
        .enumerate()
        .map(|(i, t)| event(i as u64, t, "candidate"))
        .collect();
    for tick in [0, 1, 5, u64::MAX - 1, u64::MAX] {
        events.push(event(events.len() as u64, tick, "witness"));
    }
    let mut tied = event(0, 5, "witness");
    tied.source_id = [2; 32];
    events.push(tied);
    for _ in 0..2 {
        for within in [0, 1, 4, 5, u64::MAX] {
            let mut expected: Vec<_> = events
                .iter()
                .filter(|row| row.kind == "candidate")
                .cloned()
                .collect();
            for row in &mut expected {
                let witness = events
                    .iter()
                    .filter(|w| {
                        w.kind == "witness"
                            && row
                                .tick
                                .checked_sub(w.tick)
                                .is_some_and(|d| d > 0 && d <= within)
                    })
                    .max_by_key(|w| (w.tick, w.source_id, w.source_sequence));
                if let Some(witness) = witness {
                    row.derived.insert("last".to_string(), witness.text.clone());
                } else {
                    row.derived.remove("last");
                }
            }
            expected.sort_by_key(|row| (row.tick, row.source_id, row.source_sequence));
            assert_eq!(
                evaluate(&last(within), &events, &EventQueryLimits::default())
                    .unwrap()
                    .rows,
                expected
            );
        }
        events.reverse();
    }
    let mut expected = event(0, 2, "candidate");
    expected.derived.remove("last");
    assert_eq!(
        evaluate(
            &last(5),
            &[event(0, 2, "candidate")],
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        vec![expected]
    );
    assert_eq!(
        evaluate(
            &last(5),
            &[event(1, 1, "witness")],
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        vec![]
    );
}

#[test]
fn duplicate_witness_projections_keep_first_and_missing_nearest_never_uses_older_value() {
    let events = [
        event(0, 3, "candidate"),
        event(1, 1, "witness"),
        event(2, 2, "witness"),
    ];
    let projected = |pointer: &str| json!({"Map":{"input":filter("witness"),"field":"label","pointer":pointer}});
    let witness = json!({"Union":{"inputs":[projected("/first"),projected("/second")]}});
    let mut expected = events[0].clone();
    expected
        .derived
        .insert("last".to_string(), "first".to_string());
    assert_eq!(
        evaluate(
            &query(filter("candidate"), witness, "last", "label", 5),
            &events,
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        vec![expected]
    );
    let mut events = events;
    events[1]
        .derived
        .insert("label".to_string(), "older".to_string());
    let mut expected = events[0].clone();
    expected.derived.remove("last");
    assert_eq!(
        evaluate(
            &query(filter("candidate"), filter("witness"), "last", "label", 5),
            &events,
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        vec![expected]
    );
    events[2].derived.insert("label".to_string(), String::new());
    let mut expected = events[0].clone();
    expected.derived.insert("last".to_string(), String::new());
    assert_eq!(
        evaluate(
            &query(filter("candidate"), filter("witness"), "last", "label", 5),
            &events,
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        vec![expected]
    );
}

#[test]
fn canonical_lookup_and_projected_witness_values_compose_without_mutating_source_fields() {
    let mut events = [event(0, 2, "candidate"), event(1, 1, "witness")];
    events[1].process = None;
    for (field, value) in [
        ("kind", Some("witness")),
        ("text", Some("witness-1")),
        ("source_kind", Some("serial")),
        ("process", None),
    ] {
        events[1]
            .derived
            .insert(field.to_string(), "shadow".to_string());
        let mut expected = events[0].clone();
        if let Some(value) = value {
            expected
                .derived
                .insert("last".to_string(), value.to_string());
        } else {
            expected.derived.remove("last");
        }
        assert_eq!(
            evaluate(
                &query(filter("candidate"), filter("witness"), "last", field, 5),
                &events,
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            vec![expected]
        );
    }
    for operator in ["Map", "MapText"] {
        events[1].text = json!({"value":"20"}).to_string();
        let witness =
            json!({operator:{"input":filter("witness"),"field":"amount","pointer":"/value"}});
        let q:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":encoded(filter("candidate"),witness,"last","amount",5),"predicate":{"FieldNumber":{"field":"last","comparison":"GreaterThan","value":10}}}},"fields":["last"]}})).unwrap();
        let mut expected = events[0].clone();
        expected.derived = BTreeMap::from([("last".to_string(), "20".to_string())]);
        assert_eq!(
            evaluate(&q, &events, &EventQueryLimits::default())
                .unwrap()
                .rows,
            vec![expected]
        );
    }
    let mut expected = events[0].clone();
    expected
        .derived
        .insert("kind".to_string(), "witness".to_string());
    assert_eq!(
        evaluate(
            &query(filter("candidate"), filter("witness"), "kind", "kind", 5),
            &events,
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        vec![expected]
    );
}

#[test]
fn names_and_hidden_invalid_witnesses_are_admitted_even_for_empty_or_zero_windows() {
    let limits = EventQueryLimits::default();
    for invalid in [
        String::new(),
        "a\nb".to_string(),
        "é".repeat(MAX_FIELD_BYTES / 2) + "x",
    ] {
        for (field, witness_field) in [(&invalid[..], "text"), ("last", &invalid[..])] {
            assert_eq!(
                evaluate(
                    &query(filter("absent"), filter("witness"), field, witness_field, 0),
                    &[],
                    &limits
                ),
                Err(EventQueryError::InvalidField)
            );
        }
    }
    let exact = "é".repeat(MAX_FIELD_BYTES / 2);
    let mut events = [event(0, 2, "candidate"), event(1, 1, "witness")];
    events[1].derived.insert(exact.clone(), "value".to_string());
    let mut expected = events[0].clone();
    expected.derived.insert(exact.clone(), "value".to_string());
    assert_eq!(
        evaluate(
            &query(filter("candidate"), filter("witness"), &exact, &exact, 5),
            &events,
            &limits
        )
        .unwrap()
        .rows,
        vec![expected]
    );
    events[1].text = "invalid JSON".to_string();
    let malformed = json!({"MapText":{"input":filter("witness"),"field":"x","pointer":"/value"}});
    assert_eq!(
        evaluate(
            &query(filter("absent"), malformed, "last", "x", 0),
            &events,
            &limits
        ),
        Err(EventQueryError::InvalidJsonText)
    );
    fill_to(&mut events[1], MAX_DERIVED_BYTES);
    let invalid = json!({"Map":{"input":filter("witness"),"field":"x","pointer":"/first"}});
    assert_eq!(
        evaluate(
            &query(filter("absent"), invalid, "last", "x", 0),
            &events,
            &limits
        ),
        Err(EventQueryError::InvalidEvent)
    );
}

#[test]
fn generated_and_intermediate_capacity_cannot_be_hidden_by_downstream_narrowing() {
    let limits = EventQueryLimits::default();
    let mut events = [event(0, 2, "candidate"), event(1, 1, "witness")];
    events[0].derived.remove("last");
    events[1]
        .derived
        .insert("label".to_string(), "v".to_string());
    fill_to(&mut events[0], MAX_DERIVED_BYTES - "last".len() - 1);
    let q = query(filter("candidate"), filter("witness"), "last", "label", 5);
    let mut expected = events[0].clone();
    expected.derived.insert("last".to_string(), "v".to_string());
    assert_eq!(evaluate(&q, &events, &limits).unwrap().rows, vec![expected]);
    let narrow = |q| EventQuery::Narrow {
        input: Box::new(q),
        fields: vec!["last".to_string()],
    };
    events[0].derived.get_mut("pad").unwrap().push('x');
    assert_eq!(
        evaluate(&narrow(q.clone()), &events, &limits),
        Err(EventQueryError::InvalidEvent)
    );
    events[0] = event(0, 2, "candidate");
    while events[0].derived.len() < MAX_DERIVED_FIELDS {
        events[0]
            .derived
            .insert(format!("f{}", events[0].derived.len()), String::new());
    }
    let mut expected = events[0].clone();
    expected.derived.insert("last".to_string(), "v".to_string());
    assert_eq!(evaluate(&q, &events, &limits).unwrap().rows, vec![expected]);
    events[0].derived.remove("last");
    events[0].derived.insert("extra".to_string(), String::new());
    assert_eq!(
        evaluate(&narrow(q), &events, &limits),
        Err(EventQueryError::InvalidEvent)
    );
    events[0] = event(0, 2, "candidate");
    events[0].derived.remove("last");
    fill_to(&mut events[0], MAX_DERIVED_BYTES);
    let input = json!({"Map":{"input":filter("candidate"),"field":"last","pointer":"/first"}});
    assert_eq!(
        evaluate(
            &narrow(query(input, filter("absent"), "last", "label", 0)),
            &events,
            &limits
        ),
        Err(EventQueryError::InvalidEvent)
    );
}

#[test]
fn shared_bounds_moments_and_receipts_cover_both_expressions_and_annotations() {
    let events = [event(0, 2, "candidate"), event(1, 1, "witness")];
    let q = last(1);
    let exact = EventQueryLimits {
        max_nodes: 5,
        max_depth: 2,
        max_scanned_events: 2,
        max_operations: 10,
        ..EventQueryLimits::default()
    };
    let result = evaluate(&q, &events, &exact).unwrap();
    let mut expected = events[0].clone();
    expected
        .derived
        .insert("last".to_string(), "witness-1".to_string());
    assert_eq!(result.rows, vec![expected]);
    let bytes = serde_json::to_vec(&result.rows).unwrap().len();
    assert_eq!(
        evaluate(
            &q,
            &events,
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
                max_operations: 9,
                ..exact
            },
            EventQueryError::OperationBound,
        ),
        (
            EventQueryLimits {
                max_nodes: 4,
                ..exact
            },
            EventQueryError::QueryBound,
        ),
        (
            EventQueryLimits {
                max_depth: 1,
                ..exact
            },
            EventQueryError::QueryDepth,
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
        assert_eq!(evaluate(&q, &events, &limits), Err(error));
    }
    let limits = EventQueryLimits::default();
    let input = json!({"UpTo":{"input":filter("candidate"),"moment_tick":2}});
    let witness = json!({"UpTo":{"input":filter("witness"),"moment_tick":2}});
    let q = query(input.clone(), witness.clone(), "last", "text", 1);
    let result = evaluate(&q, &events, &limits).unwrap();
    let receipt = receipt_for(&q, &events, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(2));
    validate_receipt(&receipt, &q, &events, &result, &limits).unwrap();
    for changed in [
        query(input.clone(), witness.clone(), "last", "text", 2),
        query(input.clone(), witness.clone(), "other", "text", 1),
        query(input.clone(), witness.clone(), "last", "kind", 1),
        query(input.clone(), filter("witness"), "last", "text", 1),
        query(filter("candidate"), witness.clone(), "last", "text", 1),
    ] {
        assert!(validate_receipt(&receipt, &changed, &events, &result, &limits).is_err());
    }
    let mut changed = events.clone();
    changed[1].text = "changed witness".to_string();
    assert!(validate_receipt(&receipt, &q, &changed, &result, &limits).is_err());
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("last".to_string(), "forged".to_string());
    assert!(validate_receipt(&receipt, &q, &events, &forged, &limits).is_err());
    let conflict = query(
        input,
        json!({"UpTo":{"input":filter("witness"),"moment_tick":1}}),
        "last",
        "text",
        1,
    );
    assert_eq!(
        evaluate(&conflict, &events, &limits),
        Err(EventQueryError::AmbiguousMoment)
    );
}
