use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const OPERATORS: [&str; 2] = ["Intersect", "Difference"];

fn events() -> Vec<EventEnvelope> {
    [
        (1, 0, 30, "request", 0),
        (1, 1, 10, "error", 1),
        (2, 0, 20, "error", 0),
        (2, 1, 10, "request", 1),
        (1, 2, 40, "error", 0),
        (2, 2, 50, "request", 0),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (source, sequence, tick, kind, vm))| EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [source; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick,
        vm_index: Some(vm),
        process: Some("monitor".to_string()),
        thread: None,
        kind: kind.to_string(),
        text: json!({"left": format!("L{i}"), "right": format!("R{i}")}).to_string(),
        payload_digest: [3; 32],
        details: Some(json!({"left": format!("L{i}"), "right": format!("R{i}")})),
        derived: BTreeMap::from([("kept".to_string(), "original".to_string())]),
    })
    .collect()
}

fn filter(predicate: Value) -> Value {
    json!({"Filter": {"input": "All", "predicate": predicate}})
}

fn query(operator: &str, input: Value, other: Value) -> EventQuery {
    serde_json::from_value(json!({operator: {"input": input, "other": other}}))
        .expect("retained set operations must be supported")
}

fn at(input: Value, tick: u64) -> Value {
    json!({"UpTo": {"input": input, "moment_tick": tick}})
}

// r[verify chaoscontrol.event_set_algebra.combination]
#[test]
fn set_operations_match_independent_position_membership_oracle() {
    let mut events = events();
    let limits = EventQueryLimits::default();
    let operands = [
        json!("All"),
        filter(json!({"Vm": 0})),
        filter(json!({"Kind": "error"})),
        filter(json!({"Kind": "absent"})),
    ];
    for _ in 0..2 {
        for input in &operands {
            for other in &operands {
                let left = evaluate(
                    &serde_json::from_value(input.clone()).unwrap(),
                    &events,
                    &limits,
                )
                .unwrap()
                .rows;
                let right = evaluate(
                    &serde_json::from_value(other.clone()).unwrap(),
                    &events,
                    &limits,
                )
                .unwrap()
                .rows;
                for operator in OPERATORS {
                    let expected: Vec<_> = left
                        .iter()
                        .filter(|row| {
                            let present = right.iter().any(|other| {
                                other.source_id == row.source_id
                                    && other.source_sequence == row.source_sequence
                            });
                            present == (operator == "Intersect")
                        })
                        .cloned()
                        .collect();
                    assert_eq!(
                        evaluate(
                            &query(operator, input.clone(), other.clone()),
                            &events,
                            &limits
                        )
                        .unwrap()
                        .rows,
                        expected,
                        "{operator}"
                    );
                }
            }
        }
        events.reverse();
    }
}

#[test]
fn membership_ignores_right_projections_but_preserves_left_rows() {
    let events = events();
    let input =
        json!({"Map": {"input": filter(json!({"Vm": 0})), "field": "label", "pointer": "/left"}});
    let other = json!({"MapText": {"input": filter(json!({"Kind": "error"})), "field": "label", "pointer": "/right"}});
    for (operator, indexes) in [("Intersect", vec![2, 4]), ("Difference", vec![0, 5])] {
        let expected: Vec<_> = indexes
            .into_iter()
            .map(|i| {
                let mut row = events[i].clone();
                row.derived.insert("label".to_string(), format!("L{i}"));
                row
            })
            .collect();
        assert_eq!(
            evaluate(
                &query(operator, input.clone(), other.clone()),
                &events,
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            expected
        );
    }
    // Repeated source positions can carry distinct query-local projections.
    let mut repeated = events.clone();
    for index in 0..96 {
        let mut row = events[index % events.len()].clone();
        row.derived
            .insert("kept".to_string(), format!("later-{index}"));
        repeated.push(row);
    }
    let mut expected = events;
    expected.sort_by_key(|event| (event.tick, event.source_id, event.source_sequence));
    for (operator, other) in [
        ("Intersect", json!("All")),
        ("Difference", filter(json!({"Kind":"absent"}))),
    ] {
        assert_eq!(
            evaluate(
                &query(operator, json!("All"), other),
                &repeated,
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            expected
        );
    }
}

#[test]
fn hidden_right_branches_remain_admitted_even_when_left_is_empty() {
    let limits = EventQueryLimits::default();
    let empty = filter(json!({"Kind": "absent"}));
    for operator in OPERATORS {
        let invalid_regex = filter(json!({"Regex": {"field": "kind", "pattern": "["}}));
        assert_eq!(
            evaluate(&query(operator, empty.clone(), invalid_regex), &[], &limits),
            Err(EventQueryError::InvalidRegex)
        );
        let text_map = json!({"MapText": {"input": "All", "field": "label", "pointer": "/left"}});
        let mut malformed = events();
        malformed[5].text = "plain log".to_string();
        assert_eq!(
            evaluate(
                &query(operator, empty.clone(), text_map),
                &malformed,
                &limits
            ),
            Err(EventQueryError::InvalidJsonText)
        );
        let mut large = events();
        large[0].details = Some(json!({"big": "x".repeat(MAX_DERIVED_BYTES / 2)}));
        let first = json!({"Map": {"input": "All", "field": "first", "pointer": "/big"}});
        let second = json!({"Map": {"input": first, "field": "second", "pointer": "/big"}});
        assert_eq!(
            evaluate(&query(operator, empty.clone(), second), &large, &limits),
            Err(EventQueryError::InvalidEvent)
        );
    }
}

#[test]
fn both_operands_share_structural_work_and_result_bounds() {
    let events = events();
    for operator in OPERATORS {
        let original = query(operator, json!("All"), json!("All"));
        let exact = EventQueryLimits {
            max_nodes: 3,
            max_depth: 1,
            max_operations: events.len() * 4,
            ..EventQueryLimits::default()
        };
        let mut expected = if operator == "Intersect" {
            events.clone()
        } else {
            Vec::new()
        };
        expected.sort_by_key(|event| (event.tick, event.source_id, event.source_sequence));
        assert_eq!(evaluate(&original, &events, &exact).unwrap().rows, expected);
        for (limits, error) in [
            (
                EventQueryLimits {
                    max_nodes: 2,
                    ..exact
                },
                EventQueryError::QueryBound,
            ),
            (
                EventQueryLimits {
                    max_operations: exact.max_operations - 1,
                    ..exact
                },
                EventQueryError::OperationBound,
            ),
        ] {
            assert_eq!(evaluate(&original, &events, &limits), Err(error));
        }
        let nested_right = query(operator, json!("All"), at(json!("All"), u64::MAX));
        assert_eq!(
            evaluate(&nested_right, &events, &exact),
            Err(EventQueryError::QueryDepth)
        );
    }
    let original = query("Intersect", json!("All"), json!("All"));
    let capped = evaluate(
        &original,
        &events,
        &EventQueryLimits {
            max_result_rows: 1,
            ..EventQueryLimits::default()
        },
    )
    .unwrap();
    assert_eq!(capped.rows, vec![events[1].clone()]);
    assert_eq!(capped.matched, 6);
    assert!(capped.truncated);
    assert_eq!(
        evaluate(
            &original,
            &events,
            &EventQueryLimits {
                max_result_bytes: 1,
                ..EventQueryLimits::default()
            }
        ),
        Err(EventQueryError::ResultBound)
    );
}

#[test]
fn set_receipts_bind_both_operands_and_moments() {
    let events = events();
    let limits = EventQueryLimits::default();
    for operator in OPERATORS {
        let original = query(
            operator,
            at(json!("All"), 25),
            at(filter(json!({"Kind":"error"})), 25),
        );
        let result = evaluate(&original, &events, &limits).unwrap();
        let expected = if operator == "Intersect" {
            vec![events[1].clone(), events[2].clone()]
        } else {
            vec![events[3].clone()]
        };
        assert_eq!(result.rows, expected);
        let receipt = receipt_for(&original, &events, &result, &limits).unwrap();
        assert_eq!(receipt.moment_tick, Some(25));
        validate_receipt(&receipt, &original, &events, &result, &limits).unwrap();
        let equivalent = query(
            operator,
            at(json!("All"), 25),
            at(
                filter(json!({"Matches":{"field":"kind","text":"error"}})),
                25,
            ),
        );
        assert_eq!(
            validate_receipt(&receipt, &equivalent, &events, &result, &limits),
            Err(EventQueryError::ReceiptDrift)
        );
        let mut changed = events.clone();
        changed[5].text = "changed non-selected source".to_string();
        assert_eq!(
            validate_receipt(&receipt, &original, &changed, &result, &limits),
            Err(EventQueryError::SourceDrift)
        );
        let mut forged = result.clone();
        forged.rows[0]
            .derived
            .insert("kept".to_string(), "forged".to_string());
        assert_eq!(
            validate_receipt(&receipt, &original, &events, &forged, &limits),
            Err(EventQueryError::ResultDrift)
        );
        let mixed = query(operator, at(json!("All"), 25), at(json!("All"), 40));
        assert_eq!(
            evaluate(&mixed, &events, &limits),
            Err(EventQueryError::AmbiguousMoment)
        );
        let outer: EventQuery =
            serde_json::from_value(at(serde_json::to_value(mixed).unwrap(), 10)).unwrap();
        let result = evaluate(&outer, &events, &limits).unwrap();
        assert_eq!(
            receipt_for(&outer, &events, &result, &limits)
                .unwrap()
                .moment_tick,
            Some(10)
        );
    }
}
