use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_EVENT_TEXT_BYTES,
    MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn event(sequence: u64, text: String) -> EventEnvelope {
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick: sequence,
        vm_index: Some(0),
        process: Some("memory-monitor".to_string()),
        thread: None,
        kind: "serial".to_string(),
        text,
        payload_digest: [2; 32],
        details: Some(json!({"used": 999})),
        derived: BTreeMap::from([("used".to_string(), "999".to_string())]),
    }
}

fn leaf(pointer: &str, comparison: &str, value: Value) -> Value {
    json!({"JsonTextNumber": {"pointer": pointer, "comparison": comparison, "value": value}})
}

fn query(predicate: Value) -> EventQuery {
    serde_json::from_value(json!({"Filter": {"input": "All", "predicate": predicate}}))
        .expect("numeric JSON text predicates must be supported")
}

// r[verify chaoscontrol.json_text_number.comparison]
#[test]
fn numeric_stdout_selection_preserves_rows_and_does_not_coerce_values() {
    let values = [
        json!(-3),
        json!(-2.5),
        json!(-0.0),
        json!(50),
        json!(50.25),
        json!(100),
        json!("100"),
        json!(true),
        Value::Null,
        json!([]),
        json!({}),
    ];
    let mut events: Vec<_> = values
        .into_iter()
        .enumerate()
        .map(|(i, value)| event(i as u64, json!({"used": value}).to_string()))
        .collect();
    events.push(event(events.len() as u64, "{}".to_string()));
    for (comparison, indexes) in [
        ("LessThan", vec![0]),
        ("LessThanOrEqual", vec![0, 1]),
        ("Equal", vec![1]),
        ("GreaterThanOrEqual", vec![1, 2, 3, 4, 5]),
        ("GreaterThan", vec![2, 3, 4, 5]),
    ] {
        let result = evaluate(
            &query(leaf("/used", comparison, json!(-2.5))),
            &events,
            &EventQueryLimits::default(),
        )
        .unwrap();
        let expected: Vec<_> = indexes.into_iter().map(|i| events[i].clone()).collect();
        assert_eq!(result.rows, expected, "{comparison}");
    }
    let negated = query(json!({"Not": leaf("/used", "GreaterThan", json!(50))}));
    let expected: Vec<_> = events
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 4 && *i != 5)
        .map(|(_, event)| event.clone())
        .collect();
    assert_eq!(
        evaluate(&negated, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        expected
    );
}

#[test]
fn escaped_unicode_array_pointers_and_binary64_semantics_match_details_comparisons() {
    let source = event(
        0,
        json!({"指标/量": {"~used": [9_007_199_254_740_993_u64]}, "zero": -0.0}).to_string(),
    );
    for (pointer, value) in [
        ("/指标~1量/~0used/0", json!(9_007_199_254_740_992_u64)),
        ("/zero", json!(0)),
    ] {
        assert_eq!(
            evaluate(
                &query(leaf(pointer, "Equal", value)),
                std::slice::from_ref(&source),
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            vec![source.clone()]
        );
    }
}

#[test]
fn visited_bad_json_fails_and_text_byte_admission_precedes_predicates() {
    let predicate = query(leaf("/used", "GreaterThan", json!(0)));
    for text in [
        "not-json".to_string(),
        "{\"used\":1} trailing".to_string(),
        format!("{}0{}", "[".repeat(200), "]".repeat(200)),
        "{\"used\":1e400}".to_string(),
    ] {
        let events = [event(0, "{\"used\":1}".to_string()), event(1, text)];
        assert_eq!(
            evaluate(&predicate, &events, &EventQueryLimits::default()),
            Err(EventQueryError::InvalidJsonText)
        );
    }
    let base = "{\"used\":1,\"pad\":\"\"}";
    let text = format!(
        "{{\"used\":1,\"pad\":\"{}\"}}",
        "x".repeat(MAX_EVENT_TEXT_BYTES - base.len())
    );
    let mut source = event(0, text);
    assert_eq!(
        evaluate(
            &predicate,
            std::slice::from_ref(&source),
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        vec![source.clone()]
    );
    source.text.push(' ');
    assert_eq!(
        evaluate(&predicate, &[source], &EventQueryLimits::default()),
        Err(EventQueryError::InvalidText)
    );
}

#[test]
fn short_circuiting_skips_parsing_but_not_invalid_descendant_admission() {
    let events = [event(0, "plain log".to_string())];
    let numeric = leaf("/used", "GreaterThan", json!(0));
    let limits = EventQueryLimits {
        max_operations: 3,
        ..EventQueryLimits::default()
    };
    let skipped = query(json!({"All": [{"Process": "other"}, numeric]}));
    assert_eq!(
        evaluate(&skipped, &events, &limits).unwrap().rows,
        Vec::<EventEnvelope>::new()
    );
    let skipped = query(json!({"Any": [{"Process": "memory-monitor"}, numeric]}));
    assert_eq!(evaluate(&skipped, &events, &limits).unwrap().rows, events);
    let exhausted = EventQueryLimits {
        max_operations: 2,
        ..limits
    };
    assert_eq!(
        evaluate(&skipped, &events, &exhausted),
        Err(EventQueryError::OperationBound)
    );
    for predicate in [
        json!({"All": [{"Process": "memory-monitor"}, numeric]}),
        json!({"Any": [{"Process": "other"}, numeric]}),
        json!({"Not": numeric}),
    ] {
        assert_eq!(
            evaluate(&query(predicate), &events, &EventQueryLimits::default()),
            Err(EventQueryError::InvalidJsonText)
        );
    }
    for pointer in [
        "".to_string(),
        "used".to_string(),
        "/bad\n".to_string(),
        format!("/{}", "x".repeat(MAX_FIELD_BYTES)),
    ] {
        let invalid = query(
            json!({"Any": [{"Process": "memory-monitor"}, leaf(&pointer, "Equal", json!(0))]}),
        );
        assert_eq!(
            evaluate(&invalid, &events, &EventQueryLimits::default()),
            Err(EventQueryError::InvalidPointer)
        );
    }
}

#[test]
fn predicate_types_work_and_structural_bounds_remain_enforced() {
    let source = event(0, "{\"used\":2}".to_string());
    let numeric = leaf("/used", "GreaterThan", json!(1));
    let predicate = query(numeric.clone());
    let exact = EventQueryLimits {
        max_operations: 2,
        ..EventQueryLimits::default()
    };
    assert_eq!(
        evaluate(&predicate, std::slice::from_ref(&source), &exact)
            .unwrap()
            .rows,
        vec![source.clone()]
    );
    assert_eq!(
        evaluate(
            &predicate,
            std::slice::from_ref(&source),
            &EventQueryLimits {
                max_operations: 1,
                ..exact
            }
        ),
        Err(EventQueryError::OperationBound)
    );
    for value in [Value::Null, json!("1"), json!(true), json!([])] {
        assert!(serde_json::from_value::<EventQuery>(
            json!({"Filter": {"input": "All", "predicate": leaf("/used", "Equal", value)}})
        )
        .is_err());
    }
    assert!(serde_json::from_value::<EventQuery>(
        json!({"Filter": {"input": "All", "predicate": leaf("/used", "Approximately", json!(1))}})
    )
    .is_err());
    let nested = query(json!({"Not": {"Not": numeric}}));
    assert_eq!(
        evaluate(
            &nested,
            std::slice::from_ref(&source),
            &EventQueryLimits {
                max_nodes: 2,
                ..EventQueryLimits::default()
            }
        ),
        Err(EventQueryError::QueryBound)
    );
    assert_eq!(
        evaluate(
            &nested,
            &[source],
            &EventQueryLimits {
                max_depth: 1,
                ..EventQueryLimits::default()
            }
        ),
        Err(EventQueryError::QueryDepth)
    );
}

#[test]
fn receipts_bind_text_threshold_pointer_and_original_rows_with_inherited_moments() {
    let events = [
        event(0, "{\"used\":80,\"alias\":80}".to_string()),
        event(1, "{\"used\":20,\"alias\":20}".to_string()),
    ];
    let limits = EventQueryLimits::default();
    let base = query(leaf("/used", "GreaterThan", json!(50)));
    let original = EventQuery::UpTo {
        input: Box::new(base),
        moment_tick: 1,
    };
    let result = evaluate(&original, &events, &limits).unwrap();
    assert_eq!(result.rows, vec![events[0].clone()]);
    let receipt = receipt_for(&original, &events, &result, &limits).unwrap();
    validate_receipt(&receipt, &original, &events, &result, &limits).unwrap();
    for (pointer, threshold) in [("/used", 51), ("/alias", 50)] {
        let changed = EventQuery::UpTo {
            input: Box::new(query(leaf(pointer, "GreaterThan", json!(threshold)))),
            moment_tick: 1,
        };
        assert_eq!(
            validate_receipt(&receipt, &changed, &events, &result, &limits),
            Err(EventQueryError::ReceiptDrift)
        );
    }
    let mut changed = events.clone();
    changed[1].text = "{\"used\":21}".to_string();
    assert_eq!(
        validate_receipt(&receipt, &original, &changed, &result, &limits),
        Err(EventQueryError::SourceDrift)
    );
    let mut changed = result.clone();
    changed.rows[0].text = "{\"used\":81}".to_string();
    assert_eq!(
        validate_receipt(&receipt, &original, &events, &changed, &limits),
        Err(EventQueryError::ResultDrift)
    );
}
