use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventPredicate, EventQuery,
    EventQueryError, EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn event(sequence: u64, text: &str) -> EventEnvelope {
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick: sequence,
        vm_index: Some(0),
        process: Some("monitor".to_string()),
        thread: Some(7),
        kind: "log".to_string(),
        text: "unstructured log".to_string(),
        payload_digest: [9; 32],
        details: Some(json!({"value": 100})),
        derived: BTreeMap::from([
            ("amount".to_string(), text.to_string()),
            ("context".to_string(), "preserved".to_string()),
        ]),
    }
}

fn leaf(field: &str, comparison: &str, value: Value) -> Value {
    json!({"FieldNumber": {"field": field, "comparison": comparison, "value": value}})
}

fn query(predicate: Value) -> EventQuery {
    serde_json::from_value(json!({"Filter": {"input": "All", "predicate": predicate}}))
        .expect("numeric field predicates must be supported")
}

// r[verify chaoscontrol.field_number.comparison]
#[test]
fn numeric_fields_use_all_six_numeric_relations_not_lexical_order() {
    let values = [
        "-3",
        "2",
        "10",
        "2.5",
        "2e1",
        "",
        "junk",
        "true",
        "null",
        "\"2\"",
        "[2]",
        "{}",
        "NaN",
        "Infinity",
        "1e400",
        "02",
        "+2",
        "0x2",
        "2 trailing",
        ".2",
        "2.",
    ];
    let mut events: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(index, text)| event(index as u64, text))
        .collect();
    let mut missing = event(events.len() as u64, "unused");
    missing.derived.remove("amount");
    events.push(missing);
    for (relation, indexes) in [
        ("LessThan", vec![0, 1]),
        ("LessThanOrEqual", vec![0, 1, 3]),
        ("Equal", vec![3]),
        ("NotEqual", vec![0, 1, 2, 4]),
        ("GreaterThanOrEqual", vec![2, 3, 4]),
        ("GreaterThan", vec![2, 4]),
    ] {
        let expected: Vec<_> = indexes.into_iter().map(|i| events[i].clone()).collect();
        assert_eq!(
            evaluate(
                &query(leaf("amount", relation, json!(2.5))),
                &events,
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            expected,
            "{relation}"
        );
    }
    let complement = query(json!({"Not": leaf("amount", "Equal", json!(2.5))}));
    let expected: Vec<_> = events
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 3)
        .map(|(_, row)| row.clone())
        .collect();
    assert_eq!(
        evaluate(&complement, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        expected
    );
}

#[test]
fn whitespace_signed_zero_and_binary64_rounding_follow_shared_comparison() {
    for (strings, threshold) in [
        (vec!["-0", "0.0", "1e-400"], json!(0)),
        (
            vec!["9007199254740992", "9007199254740993"],
            json!(9007199254740992_u64),
        ),
        (vec!["\t 2.5 \r\n", "25e-1"], json!(2.5)),
    ] {
        let events: Vec<_> = strings
            .iter()
            .enumerate()
            .map(|(i, text)| event(i as u64, text))
            .collect();
        assert_eq!(
            evaluate(
                &query(leaf("amount", "Equal", threshold.clone())),
                &events,
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            events
        );
        assert_eq!(
            evaluate(
                &query(leaf("amount", "NotEqual", threshold)),
                &events,
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            vec![]
        );
    }
}

#[test]
fn field_numbers_compose_with_counts_both_projection_sources_and_narrowing() {
    let events = [event(30, "bad"), event(10, "bad"), event(20, "bad")];
    let counter =
        json!({"RunningCount": {"input": "All", "predicate": {"All": []}, "field": "seen"}});
    let counted: EventQuery = serde_json::from_value(json!({"Filter": {"input": counter, "predicate": leaf("seen", "GreaterThanOrEqual", json!(2))}})).unwrap();
    let mut expected = vec![events[2].clone(), events[0].clone()];
    expected[0]
        .derived
        .insert("seen".to_string(), "2".to_string());
    expected[1]
        .derived
        .insert("seen".to_string(), "3".to_string());
    assert_eq!(
        evaluate(&counted, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        expected
    );
    let narrowed = EventQuery::Narrow {
        input: Box::new(counted),
        fields: vec!["seen".to_string()],
    };
    for row in &mut expected {
        row.derived = BTreeMap::from([("seen".to_string(), row.derived["seen"].clone())]);
    }
    assert_eq!(
        evaluate(&narrowed, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        expected
    );
    for (operator, value) in [("Map", json!(12)), ("MapText", json!("12"))] {
        let mut source = event(0, "old");
        source.details = Some(json!({"value": value}));
        source.text = json!({"value": value}).to_string();
        let projected: EventQuery = serde_json::from_value(json!({"Filter": {"input": {operator: {"input":"All", "field":"amount", "pointer":"/value"}}, "predicate": leaf("amount", "GreaterThan", json!(10))}})).unwrap();
        let mut expected = source.clone();
        expected
            .derived
            .insert("amount".to_string(), "12".to_string());
        assert_eq!(
            evaluate(&projected, &[source], &EventQueryLimits::default())
                .unwrap()
                .rows,
            vec![expected]
        );
    }
}

#[test]
fn canonical_lookup_precedence_and_non_json_logs_remain_unchanged() {
    let mut source = event(0, "12");
    for field in ["kind", "text", "process", "source_kind"] {
        source.derived.insert(field.to_string(), "99".to_string());
    }
    let limits = EventQueryLimits::default();
    for field in ["kind", "text", "process", "source_kind"] {
        assert_eq!(
            evaluate(
                &query(leaf(field, "GreaterThan", json!(1))),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![]
        );
    }
    assert_eq!(
        evaluate(
            &query(leaf("amount", "GreaterThan", json!(1))),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![source.clone()]
    );
    assert_eq!(
        evaluate(
            &query(
                json!({"JsonTextNumber": {"pointer": "/value", "comparison": "GreaterThan", "value": 1}})
            ),
            std::slice::from_ref(&source),
            &limits
        ),
        Err(EventQueryError::InvalidJsonText)
    );
    source.kind = "12".to_string();
    source.text = "13".to_string();
    source.process = Some("14".to_string());
    for field in ["kind", "text", "process"] {
        assert_eq!(
            evaluate(
                &query(leaf(field, "GreaterThan", json!(10))),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![source.clone()]
        );
    }
    source.process = None;
    assert_eq!(
        evaluate(
            &query(leaf("process", "GreaterThan", json!(10))),
            &[source],
            &limits
        )
        .unwrap()
        .rows,
        vec![]
    );
}

#[test]
fn field_admission_byte_capacity_and_shared_limits_are_enforced() {
    let limits = EventQueryLimits::default();
    for field in [
        String::new(),
        "a\nb".to_string(),
        "é".repeat(MAX_FIELD_BYTES / 2) + "x",
    ] {
        let skipped = query(json!({"Any": [{"All": []}, leaf(&field, "Equal", json!(0))]}));
        assert_eq!(
            evaluate(&skipped, &[], &limits),
            Err(EventQueryError::InvalidField)
        );
    }
    for value in [json!("2"), Value::Null, json!(true), json!({}), json!([])] {
        assert!(serde_json::from_value::<EventPredicate>(leaf("amount", "Equal", value)).is_err());
    }
    let field = "é".repeat(MAX_FIELD_BYTES / 2);
    let mut source = event(0, "2");
    source.derived = BTreeMap::from([(field.clone(), "2".to_string())]);
    assert_eq!(
        evaluate(
            &query(leaf(&field, "Equal", json!(2))),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![source]
    );
    let mut source = event(0, "2");
    source.derived = BTreeMap::from([(
        "amount".to_string(),
        " ".repeat(MAX_DERIVED_BYTES - "amount".len() - 1) + "2",
    )]);
    assert_eq!(
        evaluate(
            &query(leaf("amount", "Equal", json!(2))),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![source]
    );
    let events = [event(0, "2"), event(1, "3")];
    let query =
        query(json!({"All": [leaf("amount", "GreaterThan", json!(1)), {"SourceKind":"serial"}]}));
    let exact = EventQueryLimits {
        max_nodes: 4,
        max_depth: 1,
        max_operations: 8,
        ..limits
    };
    let result = evaluate(&query, &events, &exact).unwrap();
    assert_eq!(result.rows, events);
    let bytes = serde_json::to_vec(&result.rows).unwrap().len();
    assert_eq!(
        evaluate(
            &query,
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
                max_operations: 7,
                ..exact
            },
            EventQueryError::OperationBound,
        ),
        (
            EventQueryLimits {
                max_nodes: 3,
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
        assert_eq!(evaluate(&query, &events, &limits), Err(error));
    }
    let wrapped = EventQuery::UpTo {
        input: Box::new(query.clone()),
        moment_tick: 1,
    };
    assert_eq!(
        evaluate(
            &wrapped,
            &events,
            &EventQueryLimits {
                max_nodes: 5,
                ..exact
            }
        ),
        Err(EventQueryError::QueryDepth)
    );
    let prefix = evaluate(
        &query,
        &events,
        &EventQueryLimits {
            max_result_rows: 1,
            ..exact
        },
    )
    .unwrap();
    assert_eq!(prefix.rows, result.rows[..1]);
    assert_eq!((prefix.matched, prefix.truncated), (2, true));
}

#[test]
fn numeric_field_receipts_bind_field_relation_threshold_and_canonical_source() {
    let events = [event(0, "2"), event(1, "3")];
    let limits = EventQueryLimits::default();
    let input = json!({"UpTo":{"input":"All","moment_tick":0}});
    let make = |predicate| {
        serde_json::from_value::<EventQuery>(
            json!({"Filter":{"input":input,"predicate":predicate}}),
        )
        .unwrap()
    };
    let query = make(leaf("amount", "GreaterThan", json!(1)));
    let result = evaluate(&query, &events, &limits).unwrap();
    assert_eq!(result.rows, events[..1]);
    let receipt = receipt_for(&query, &events, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(0));
    validate_receipt(&receipt, &query, &events, &result, &limits).unwrap();
    for predicate in [
        leaf("amount", "GreaterThanOrEqual", json!(1)),
        leaf("amount", "GreaterThan", json!(0)),
        leaf("missing", "GreaterThan", json!(1)),
    ] {
        assert!(validate_receipt(&receipt, &make(predicate), &events, &result, &limits).is_err());
    }
    let mut changed = events.clone();
    changed[1].text = "changed outside moment".to_string();
    assert_eq!(evaluate(&query, &changed, &limits).unwrap(), result);
    assert!(validate_receipt(&receipt, &query, &changed, &result, &limits).is_err());
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("amount".to_string(), "9".to_string());
    assert!(validate_receipt(&receipt, &query, &events, &forged, &limits).is_err());
}
