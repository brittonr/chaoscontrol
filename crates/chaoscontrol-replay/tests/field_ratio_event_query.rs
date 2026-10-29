use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventPredicate, EventQuery,
    EventQueryError, EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn event(sequence: u64, numerator: &str, denominator: &str) -> EventEnvelope {
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick: sequence,
        vm_index: Some(0),
        process: Some("monitor".to_string()),
        thread: Some(7),
        kind: "storage".to_string(),
        text: "unstructured log".to_string(),
        payload_digest: [9; 32],
        details: Some(json!({"used":20,"total":100})),
        derived: BTreeMap::from([
            ("used".to_string(), numerator.to_string()),
            ("total".to_string(), denominator.to_string()),
        ]),
    }
}

fn leaf(numerator: &str, denominator: &str, comparison: &str, value: Value) -> Value {
    json!({"FieldRatio":{"numerator":numerator,"denominator":denominator,"comparison":comparison,"value":value}})
}

fn query(predicate: Value) -> EventQuery {
    serde_json::from_value(json!({"Filter":{"input":"All","predicate":predicate}}))
        .expect("finite field ratio predicates must be supported")
}

// r[verify chaoscontrol.field_ratio.comparison]
#[test]
fn ratio_relations_compare_quotients_and_exclude_every_undefined_ratio() {
    let pairs = [
        ("-1", "2"),
        ("0", "2"),
        ("1", "10"),
        ("1", "2"),
        ("4", "2"),
        ("1", "0"),
        ("1", "-0.0"),
        ("bad", "2"),
        ("1", "NaN"),
        ("1e308", "1e-308"),
        ("1e400", "2"),
        ("1", ""),
        ("\"1\"", "2"),
        ("1", "true"),
        ("01", "2"),
        ("1", "2 trailing"),
    ];
    let mut events: Vec<_> = pairs
        .iter()
        .enumerate()
        .map(|(i, (n, d))| event(i as u64, n, d))
        .collect();
    let mut missing = event(events.len() as u64, "1", "2");
    missing.derived.remove("total");
    events.push(missing);
    let mut missing = event(events.len() as u64, "1", "2");
    missing.derived.remove("used");
    events.push(missing);
    for (relation, indexes) in [
        ("LessThan", vec![0, 1]),
        ("LessThanOrEqual", vec![0, 1, 2]),
        ("Equal", vec![2]),
        ("NotEqual", vec![0, 1, 3, 4]),
        ("GreaterThanOrEqual", vec![2, 3, 4]),
        ("GreaterThan", vec![3, 4]),
    ] {
        let expected: Vec<_> = indexes.into_iter().map(|i| events[i].clone()).collect();
        assert_eq!(
            evaluate(
                &query(leaf("used", "total", relation, json!(0.1))),
                &events,
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            expected,
            "{relation}"
        );
    }
    let expected: Vec<_> = events
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 2)
        .map(|(_, row)| row.clone())
        .collect();
    assert_eq!(
        evaluate(
            &query(json!({"Not":leaf("used","total","Equal",json!(0.1))})),
            &events,
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        expected
    );
}

#[test]
fn negative_signed_zero_underflow_precision_and_same_field_cases_are_explicit() {
    for (numerator, denominator, expected) in [
        ("-1", "-2", json!(0.5)),
        ("1", "-2", json!(-0.5)),
        ("-0", "2", json!(0)),
        ("1e-308", "1e308", json!(0)),
        ("9007199254740993", "1", json!(9007199254740992_u64)),
        (" \t1e1\n", " 2 ", json!(5)),
    ] {
        let events = [event(0, numerator, denominator)];
        assert_eq!(
            evaluate(
                &query(leaf("used", "total", "Equal", expected.clone())),
                &events,
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            events
        );
        assert_eq!(
            evaluate(
                &query(leaf("used", "total", "NotEqual", expected)),
                &events,
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            vec![]
        );
    }
    let events = [event(0, "2", "irrelevant"), event(1, "0", "irrelevant")];
    assert_eq!(
        evaluate(
            &query(leaf("used", "used", "Equal", json!(1))),
            &events,
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        events[..1]
    );
}

#[test]
fn ratio_filtering_composes_with_both_projection_sources_and_running_counts() {
    for (operator, used, total) in [
        ("Map", json!(20), json!(100)),
        ("MapText", json!("20"), json!("100")),
    ] {
        let mut source = event(0, "old", "old");
        source.details = Some(json!({"used":used,"total":total}));
        source.text = json!({"used":used,"total":total}).to_string();
        let input = json!({operator:{"input":{operator:{"input":"All","field":"used","pointer":"/used"}},"field":"total","pointer":"/total"}});
        let query: EventQuery=serde_json::from_value(json!({"Filter":{"input":input,"predicate":leaf("used","total","GreaterThan",json!(0.1))}})).unwrap();
        let mut expected = source.clone();
        expected.derived = BTreeMap::from([
            ("used".to_string(), "20".to_string()),
            ("total".to_string(), "100".to_string()),
        ]);
        assert_eq!(
            evaluate(&query, &[source], &EventQueryLimits::default())
                .unwrap()
                .rows,
            vec![expected]
        );
    }
    let events = [
        event(0, "unused", "4"),
        event(1, "unused", "4"),
        event(2, "unused", "4"),
    ];
    let query: EventQuery=serde_json::from_value(json!({"Filter":{"input":{"RunningCount":{"input":"All","predicate":{"All":[]},"field":"seen"}},"predicate":leaf("seen","total","GreaterThan",json!(0.5))}})).unwrap();
    let mut expected = events[2].clone();
    expected.derived.insert("seen".to_string(), "3".to_string());
    assert_eq!(
        evaluate(&query, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        vec![expected]
    );
}

#[test]
fn canonical_name_precedence_and_both_operand_admission_are_preserved() {
    let limits = EventQueryLimits::default();
    let mut source = event(0, "1", "2");
    for field in ["kind", "text", "process", "source_kind"] {
        source.derived.insert(field.to_string(), "10".to_string());
    }
    for field in ["kind", "text", "process", "source_kind"] {
        assert_eq!(
            evaluate(
                &query(leaf(field, "total", "GreaterThan", json!(1))),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![]
        );
    }
    source.text = "4".to_string();
    source.process = None;
    assert_eq!(
        evaluate(
            &query(leaf("text", "total", "Equal", json!(2))),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![source.clone()]
    );
    assert_eq!(
        evaluate(
            &query(leaf("process", "total", "Equal", json!(5))),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![]
    );
    for invalid in [
        String::new(),
        "a\nb".to_string(),
        "é".repeat(MAX_FIELD_BYTES / 2) + "x",
    ] {
        for predicate in [
            leaf(&invalid, "total", "Equal", json!(0)),
            leaf("used", &invalid, "Equal", json!(0)),
        ] {
            assert_eq!(
                evaluate(&query(json!({"Any":[{"All":[]},predicate]})), &[], &limits),
                Err(EventQueryError::InvalidField)
            );
        }
    }
    for threshold in [json!("0.1"), json!(true), Value::Null, json!([]), json!({})] {
        assert!(serde_json::from_value::<EventPredicate>(leaf(
            "used", "total", "Equal", threshold
        ))
        .is_err());
    }
    let field = "é".repeat(MAX_FIELD_BYTES / 2);
    source.derived.insert(field.clone(), "1".to_string());
    assert_eq!(
        evaluate(
            &query(leaf(&field, "total", "Equal", json!(0.5))),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![source.clone()]
    );
    assert_eq!(
        evaluate(
            &query(leaf("total", &field, "Equal", json!(2))),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![source]
    );
}

#[test]
fn byte_capacity_and_shared_structure_work_scan_and_result_bounds_remain_effective() {
    let limits = EventQueryLimits::default();
    let mut source = event(0, "1", "2");
    source.derived = BTreeMap::from([
        (
            "used".to_string(),
            " ".repeat(MAX_DERIVED_BYTES - "used".len() - "total".len() - 2) + "1",
        ),
        ("total".to_string(), "2".to_string()),
    ]);
    assert_eq!(
        evaluate(
            &query(leaf("used", "total", "Equal", json!(0.5))),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![source]
    );
    let events = [event(0, "20", "100"), event(1, "30", "100")];
    let query = query(
        json!({"All":[{"SourceKind":"serial"},leaf("used","total","GreaterThan",json!(0.1))]}),
    );
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
    let outer = EventQuery::UpTo {
        input: Box::new(query.clone()),
        moment_tick: 1,
    };
    assert_eq!(
        evaluate(
            &outer,
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
fn ratio_receipts_bind_both_operands_relation_threshold_and_all_canonical_sources() {
    let events = [event(0, "20", "100"), event(1, "30", "100")];
    let limits = EventQueryLimits::default();
    let make = |predicate| {
        serde_json::from_value::<EventQuery>(json!({"Filter":{"input":{"UpTo":{"input":"All","moment_tick":0}},"predicate":predicate}})).unwrap()
    };
    let query = make(leaf("used", "total", "GreaterThan", json!(0.1)));
    let result = evaluate(&query, &events, &limits).unwrap();
    assert_eq!(result.rows, events[..1]);
    let receipt = receipt_for(&query, &events, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(0));
    validate_receipt(&receipt, &query, &events, &result, &limits).unwrap();
    for predicate in [
        leaf("used", "total", "GreaterThanOrEqual", json!(0.1)),
        leaf("used", "total", "GreaterThan", json!(0)),
        leaf("total", "used", "GreaterThan", json!(0.1)),
        leaf("used", "used", "GreaterThan", json!(0.1)),
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
        .insert("used".to_string(), "99".to_string());
    assert!(validate_receipt(&receipt, &query, &events, &forged, &limits).is_err());
}
