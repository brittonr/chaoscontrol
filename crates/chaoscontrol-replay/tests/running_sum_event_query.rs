use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn event(sequence: u64, tick: u64, amount: &str, kind: &str) -> EventEnvelope {
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick,
        vm_index: Some(0),
        process: Some("monitor".to_string()),
        thread: Some(7),
        kind: kind.to_string(),
        text: json!({"amount":"2.5"}).to_string(),
        payload_digest: [9; 32],
        details: Some(json!({"amount":2.5})),
        derived: BTreeMap::from([
            ("amount".to_string(), amount.to_string()),
            ("total".to_string(), "obsolete".to_string()),
        ]),
    }
}
fn sum(input: Value, predicate: Value, field: &str, value_field: &str) -> EventQuery {
    serde_json::from_value(json!({"RunningSum":{"input":input,"predicate":predicate,"field":field,"value_field":value_field}})).expect("RunningSum must be supported")
}
fn matching() -> Value {
    json!({"Kind":"metric"})
}

// r[verify chaoscontrol.running_sum.prefixes]
#[test]
fn canonical_unique_prefixes_sum_matching_values_before_output_aliasing() {
    let mut events = vec![
        event(2, u64::MAX, "-1.5", "metric"),
        event(0, 0, "2.5", "metric"),
        event(1, 0, "bad", "skip"),
    ];
    events.push(events[1].clone());
    let mut expected = vec![events[1].clone(), events[2].clone(), events[0].clone()];
    for (row, total) in expected.iter_mut().zip(["2.5", "2.5", "1.0"]) {
        row.derived.insert("total".to_string(), total.to_string());
    }
    for _ in 0..2 {
        assert_eq!(
            evaluate(
                &sum(json!("All"), matching(), "total", "amount"),
                &events,
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            expected
        );
        events.reverse();
    }
    let events = [event(0, 0, "2", "metric"), event(1, 1, "3", "metric")];
    let mut expected = events.to_vec();
    expected[0]
        .derived
        .insert("amount".to_string(), "2.0".to_string());
    expected[1]
        .derived
        .insert("amount".to_string(), "5.0".to_string());
    assert_eq!(
        evaluate(
            &sum(json!("All"), matching(), "amount", "amount"),
            &events,
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        expected
    );
}

#[test]
fn numeric_rounding_and_first_duplicate_projection_priority_follow_canonical_order() {
    let limits = EventQueryLimits::default();
    for (values, totals) in [
        (
            vec!["9007199254740992", "1", "-9007199254740992"],
            vec!["9007199254740992.0", "9007199254740992.0", "0.0"],
        ),
        (
            vec!["0.1", "0.2", "1e-400"],
            vec!["0.1", "0.30000000000000004", "0.30000000000000004"],
        ),
        (vec!["-0", "0"], vec!["0.0", "0.0"]),
    ] {
        let mut rows: Vec<_> = values
            .iter()
            .enumerate()
            .map(|(i, v)| event(i as u64, i as u64, v, "metric"))
            .collect();
        let mut expected = rows.clone();
        for (row, total) in expected.iter_mut().zip(totals) {
            row.derived.insert("total".to_string(), total.to_string());
        }
        rows.reverse();
        assert_eq!(
            evaluate(
                &sum(json!("All"), matching(), "total", "amount"),
                &rows,
                &limits
            )
            .unwrap()
            .rows,
            expected
        );
    }
    let first = event(0, 0, "2", "metric");
    let mut duplicate = first.clone();
    duplicate
        .derived
        .insert("amount".to_string(), "bad".to_string());
    let mut expected = first.clone();
    expected
        .derived
        .insert("total".to_string(), "2.0".to_string());
    assert_eq!(
        evaluate(
            &sum(json!("All"), matching(), "total", "amount"),
            &[first, duplicate],
            &limits
        )
        .unwrap()
        .rows,
        vec![expected]
    );
}

#[test]
fn invalid_matching_numbers_and_overflow_fail_but_nonmatching_operands_are_not_parsed() {
    let limits = EventQueryLimits::default();
    let q = sum(json!("All"), matching(), "total", "amount");
    for text in [
        "",
        "bad",
        "NaN",
        "Infinity",
        "1e400",
        "\"1\"",
        "true",
        "null",
        "[]",
        "{}",
        "01",
        "+1",
        "1 trailing",
    ] {
        assert_eq!(
            evaluate(&q, &[event(0, 0, text, "metric")], &limits),
            Err(EventQueryError::InvalidNumber)
        );
        let skipped = event(0, 0, text, "skip");
        let mut expected = skipped.clone();
        expected
            .derived
            .insert("total".to_string(), "0.0".to_string());
        assert_eq!(
            evaluate(&q, std::slice::from_ref(&skipped), &limits)
                .unwrap()
                .rows,
            vec![expected]
        );
    }
    let mut skipped = event(0, 0, "1", "skip");
    skipped.derived.remove("amount");
    let mut expected = skipped.clone();
    expected
        .derived
        .insert("total".to_string(), "0.0".to_string());
    assert_eq!(
        evaluate(&q, &[skipped], &limits).unwrap().rows,
        vec![expected]
    );
    let mut missing = event(0, 0, "1", "metric");
    missing.derived.remove("amount");
    assert_eq!(
        evaluate(&q, &[missing], &limits),
        Err(EventQueryError::InvalidNumber)
    );
    let overflow = [
        event(0, 0, "1e308", "metric"),
        event(1, 1, "1e308", "metric"),
        event(2, 2, "-1e308", "metric"),
    ];
    assert_eq!(
        evaluate(&q, &overflow, &limits),
        Err(EventQueryError::InvalidNumber)
    );
    let mut source = event(0, 0, "1", "metric");
    source.process = None;
    for field in ["kind", "process", "source_kind", "text"] {
        source.derived.insert(field.to_string(), "5".to_string());
        assert_eq!(
            evaluate(
                &sum(json!("All"), matching(), "total", field),
                std::slice::from_ref(&source),
                &limits
            ),
            Err(EventQueryError::InvalidNumber)
        );
    }
    source.text = "invalid JSON".to_string();
    assert_eq!(
        evaluate(
            &sum(
                json!("All"),
                json!({"JsonTextPresent":{"pointer":"/x"}}),
                "total",
                "amount"
            ),
            &[source],
            &limits
        ),
        Err(EventQueryError::InvalidJsonText)
    );
}

#[test]
fn json_projection_downstream_filters_and_name_admission_reuse_existing_contracts() {
    let limits = EventQueryLimits::default();
    let rows = [event(0, 0, "bad", "metric"), event(1, 1, "bad", "metric")];
    for operator in ["Map", "MapText"] {
        let input = json!({operator:{"input":"All","field":"amount","pointer":"/amount"}});
        let q:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":sum(input,matching(),"total","amount"),"predicate":{"FieldNumber":{"field":"total","comparison":"GreaterThan","value":3}}}},"fields":["total"]}})).unwrap();
        let mut expected = rows[1].clone();
        expected.derived = BTreeMap::from([("total".to_string(), "5.0".to_string())]);
        assert_eq!(evaluate(&q, &rows, &limits).unwrap().rows, vec![expected]);
    }
    for invalid in [
        String::new(),
        "a\nb".to_string(),
        "é".repeat(MAX_FIELD_BYTES / 2) + "x",
    ] {
        for (field, value_field) in [(&invalid[..], "amount"), ("total", &invalid[..])] {
            assert_eq!(
                evaluate(
                    &sum(json!("All"), matching(), field, value_field),
                    &[],
                    &limits
                ),
                Err(EventQueryError::InvalidField)
            );
        }
    }
    assert_eq!(
        evaluate(
            &sum(
                json!("All"),
                json!({"Any":[{"All":[]},{"Regex":{"field":"kind","pattern":"["}}]}),
                "total",
                "amount"
            ),
            &[],
            &limits
        ),
        Err(EventQueryError::InvalidRegex)
    );
    let exact = "é".repeat(MAX_FIELD_BYTES / 2);
    let mut source = event(0, 0, "2", "metric");
    source.derived.insert(exact.clone(), "2".to_string());
    let mut expected = source.clone();
    expected.derived.insert(exact.clone(), "2.0".to_string());
    assert_eq!(
        evaluate(
            &sum(json!("All"), matching(), &exact, &exact),
            &[source],
            &limits
        )
        .unwrap()
        .rows,
        vec![expected]
    );
}

#[test]
fn intermediate_generated_bytes_and_field_capacity_cannot_be_hidden_by_narrowing() {
    let limits = EventQueryLimits::default();
    let q = sum(json!("All"), matching(), "total", "amount");
    let mut source = event(0, 0, "2", "metric");
    source.derived.remove("total");
    source.derived.insert("pad".to_string(), String::new());
    let used: usize = source.derived.iter().map(|(k, v)| k.len() + v.len()).sum();
    source
        .derived
        .get_mut("pad")
        .unwrap()
        .push_str(&"x".repeat(MAX_DERIVED_BYTES - "total".len() - "2.0".len() - used));
    let mut expected = source.clone();
    expected
        .derived
        .insert("total".to_string(), "2.0".to_string());
    assert_eq!(
        evaluate(&q, std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![expected]
    );
    let narrow = |q| EventQuery::Narrow {
        input: Box::new(q),
        fields: vec!["total".to_string()],
    };
    source.derived.get_mut("pad").unwrap().push('x');
    assert_eq!(
        evaluate(&narrow(q.clone()), &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, 0, "2", "metric");
    while source.derived.len() < MAX_DERIVED_FIELDS {
        source
            .derived
            .insert(format!("f{}", source.derived.len()), String::new());
    }
    let mut expected = source.clone();
    expected
        .derived
        .insert("total".to_string(), "2.0".to_string());
    assert_eq!(
        evaluate(&q, std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![expected]
    );
    source.derived.remove("total");
    source.derived.insert("extra".to_string(), String::new());
    assert_eq!(
        evaluate(&narrow(q), std::slice::from_ref(&source), &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let input = json!({"Map":{"input":"All","field":"total","pointer":"/amount"}});
    assert_eq!(
        evaluate(
            &narrow(sum(input, matching(), "total", "amount")),
            &[source],
            &limits
        ),
        Err(EventQueryError::InvalidEvent)
    );
}

#[test]
fn shared_work_structure_result_limits_moments_and_receipts_bind_totals() {
    let rows = [event(0, 0, "2", "metric"), event(1, 1, "3", "metric")];
    let q = sum(json!("All"), matching(), "total", "amount");
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
    let compound = sum(json!("All"), json!({"All":[matching()]}), "total", "amount");
    assert_eq!(
        evaluate(
            &compound,
            &rows,
            &EventQueryLimits {
                max_nodes: 3,
                max_operations: 6,
                ..exact
            }
        )
        .unwrap(),
        result
    );
    assert_eq!(
        evaluate(
            &compound,
            &rows,
            &EventQueryLimits {
                max_nodes: 3,
                max_operations: 5,
                ..exact
            }
        ),
        Err(EventQueryError::OperationBound)
    );
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
    let q = sum(input.clone(), matching(), "total", "amount");
    let result = evaluate(&q, &rows, &limits).unwrap();
    let receipt = receipt_for(&q, &rows, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(0));
    validate_receipt(&receipt, &q, &rows, &result, &limits).unwrap();
    for changed in [
        sum(input.clone(), matching(), "other", "amount"),
        sum(input.clone(), matching(), "total", "total"),
        sum(input.clone(), json!({"All":[]}), "total", "amount"),
    ] {
        assert!(validate_receipt(&receipt, &changed, &rows, &result, &limits).is_err());
    }
    let mut changed = rows.clone();
    changed[1].text = "unselected canonical drift".to_string();
    assert!(validate_receipt(&receipt, &q, &changed, &result, &limits).is_err());
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("total".to_string(), "99".to_string());
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
