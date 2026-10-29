use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn event(sequence: u64, current: &str, previous: &str) -> EventEnvelope {
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick: sequence,
        vm_index: Some(0),
        process: None,
        thread: None,
        kind: "metric".to_string(),
        text: json!({"current":"20","previous":"8"}).to_string(),
        payload_digest: [9; 32],
        details: Some(json!({"current":20,"previous":8})),
        derived: BTreeMap::from([
            ("current".to_string(), current.to_string()),
            ("previous".to_string(), previous.to_string()),
            ("delta".to_string(), "obsolete".to_string()),
        ]),
    }
}
fn query(input: Value, field: &str, minuend: &str, subtrahend: &str) -> EventQuery {
    serde_json::from_value(json!({"MapDifference":{"input":input,"field":field,"minuend":minuend,"subtrahend":subtrahend}})).expect("MapDifference must be supported")
}
fn project(input: Value) -> EventQuery {
    query(input, "delta", "current", "previous")
}
fn annotate(mut row: EventEnvelope, field: &str, value: Option<&str>) -> EventEnvelope {
    if let Some(value) = value {
        row.derived.insert(field.to_string(), value.to_string());
    } else {
        row.derived.remove(field);
    }
    row
}

// r[verify chaoscontrol.map_difference.projection]
#[test]
fn finite_differences_preserve_rows_and_undefined_results_remove_stale_output() {
    let cases = [
        ("20", "8", Some("12.0")),
        (" -1 ", "2", Some("-3.0")),
        ("1", "-2", Some("3.0")),
        ("0", "0", Some("0.0")),
        ("-0", "0", Some("-0.0")),
        ("-0", "-0", Some("0.0")),
        ("0.3", "0.2", Some("0.09999999999999998")),
        ("9007199254740993", "9007199254740992", Some("0.0")),
        ("1e-400", "0", Some("0.0")),
        ("1e-308", "0", Some("1e-308")),
        ("1e308", "-1e308", None),
        ("-1e308", "1e308", None),
        ("1e400", "1", None),
        ("1", "NaN", None),
        ("01", "1", None),
        ("1", "2 trailing", None),
        ("\"1\"", "1", None),
        ("1", "true", None),
        ("null", "1", None),
        ("1", "[]", None),
        ("{}", "1", None),
        ("", "1", None),
    ];
    let mut rows = Vec::new();
    let mut expected = Vec::new();
    for (i, (left, right, value)) in cases.into_iter().enumerate() {
        let row = event(i as u64, left, right);
        expected.push(annotate(row.clone(), "delta", value));
        rows.push(row);
    }
    for missing in ["current", "previous"] {
        let mut row = event(rows.len() as u64, "2", "1");
        row.derived.remove(missing);
        expected.push(annotate(row.clone(), "delta", None));
        rows.push(row);
    }
    rows.reverse();
    assert_eq!(
        evaluate(&project(json!("All")), &rows, &EventQueryLimits::default())
            .unwrap()
            .rows,
        expected
    );
}

#[test]
fn aliases_read_original_operands_and_canonical_fields_override_shadows() {
    let limits = EventQueryLimits::default();
    let source = event(0, "20", "8");
    for field in ["current", "previous", "kind"] {
        assert_eq!(
            evaluate(
                &query(json!("All"), field, "current", "previous"),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![annotate(source.clone(), field, Some("12.0"))]
        );
    }
    assert_eq!(
        evaluate(
            &query(json!("All"), "current", "current", "current"),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(source, "current", Some("0.0"))]
    );
    for canonical in ["kind", "process", "source_kind", "text"] {
        let mut source = event(0, "20", "8");
        source
            .derived
            .insert(canonical.to_string(), "100".to_string());
        for (left, right) in [(canonical, "previous"), ("current", canonical)] {
            assert_eq!(
                evaluate(
                    &query(json!("All"), "delta", left, right),
                    std::slice::from_ref(&source),
                    &limits
                )
                .unwrap()
                .rows,
                vec![annotate(source.clone(), "delta", None)]
            );
        }
    }
}

#[test]
fn difference_composes_with_json_counts_past_witnesses_filters_and_narrowing() {
    let limits = EventQueryLimits::default();
    for operator in ["Map", "MapText"] {
        let source = event(0, "bad", "bad");
        let input = json!({operator:{"input":{operator:{"input":"All","field":"current","pointer":"/current"}},"field":"previous","pointer":"/previous"}});
        let q:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":project(input),"predicate":{"FieldNumber":{"field":"delta","comparison":"GreaterThan","value":10}}}},"fields":["delta"]}})).unwrap();
        let mut expected = source.clone();
        expected.derived = BTreeMap::from([("delta".to_string(), "12.0".to_string())]);
        assert_eq!(
            evaluate(&q, &[source], &limits).unwrap().rows,
            vec![expected]
        );
    }
    let rows = [event(0, "20", "8"), event(1, "20", "8")];
    let input = json!({"RunningCount":{"input":"All","predicate":{"All":[]},"field":"current"}});
    let mut expected = rows.to_vec();
    for (i, row) in expected.iter_mut().enumerate() {
        row.derived
            .insert("current".to_string(), (i + 1).to_string());
        row.derived.insert(
            "delta".to_string(),
            if i == 0 { "-7.0" } else { "-6.0" }.to_string(),
        );
    }
    assert_eq!(
        evaluate(&project(input), &rows, &limits).unwrap().rows,
        expected
    );
    let mut witness = event(0, "5", "bad");
    witness.kind = "baseline".to_string();
    let candidate = event(1, "12", "obsolete");
    let input = json!({"WithLastValue":{"input":{"Filter":{"input":"All","predicate":{"Kind":"metric"}}},"witness":{"Filter":{"input":"All","predicate":{"Kind":"baseline"}}},"field":"previous","witness_field":"current","within_ticks":1}});
    let mut expected = annotate(candidate.clone(), "delta", Some("7.0"));
    expected
        .derived
        .insert("previous".to_string(), "5".to_string());
    assert_eq!(
        evaluate(&project(input), &[candidate, witness], &limits)
            .unwrap()
            .rows,
        vec![expected]
    );
}

#[test]
fn all_three_names_are_admitted_even_without_rows() {
    let limits = EventQueryLimits::default();
    for invalid in [
        String::new(),
        "a\nb".to_string(),
        "é".repeat(MAX_FIELD_BYTES / 2) + "x",
    ] {
        for (field, left, right) in [
            (&invalid[..], "current", "previous"),
            ("delta", &invalid[..], "previous"),
            ("delta", "current", &invalid[..]),
        ] {
            assert_eq!(
                evaluate(&query(json!("All"), field, left, right), &[], &limits),
                Err(EventQueryError::InvalidField)
            );
        }
    }
    let exact = "é".repeat(MAX_FIELD_BYTES / 2);
    for (field, left, right) in [
        (&exact[..], "current", "previous"),
        ("delta", &exact[..], "previous"),
        ("delta", "current", &exact[..]),
    ] {
        let mut source = event(0, "20", "8");
        if left == exact {
            source.derived.insert(exact.clone(), "20".to_string());
        } else if right == exact {
            source.derived.insert(exact.clone(), "8".to_string());
        }
        assert_eq!(
            evaluate(
                &query(json!("All"), field, left, right),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![annotate(source, field, Some("12.0"))]
        );
    }
}

#[test]
fn intermediate_and_generated_capacity_cannot_be_hidden_by_narrowing_or_duplicates() {
    let limits = EventQueryLimits::default();
    let q = project(json!("All"));
    let narrow = |q| EventQuery::Narrow {
        input: Box::new(q),
        fields: vec!["delta".to_string()],
    };
    let mut source = event(0, "20", "8");
    source.derived.remove("delta");
    source.derived.insert("pad".to_string(), String::new());
    let used: usize = source.derived.iter().map(|(k, v)| k.len() + v.len()).sum();
    source
        .derived
        .get_mut("pad")
        .unwrap()
        .push_str(&"x".repeat(MAX_DERIVED_BYTES - "delta".len() - "12.0".len() - used));
    assert_eq!(
        evaluate(&q, std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![annotate(source.clone(), "delta", Some("12.0"))]
    );
    source.derived.get_mut("pad").unwrap().push('x');
    let first = event(0, "20", "8");
    assert_eq!(
        evaluate(&narrow(q.clone()), &[first, source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, "20", "8");
    while source.derived.len() < MAX_DERIVED_FIELDS {
        source
            .derived
            .insert(format!("f{}", source.derived.len()), String::new());
    }
    assert_eq!(
        evaluate(&q, std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![annotate(source.clone(), "delta", Some("12.0"))]
    );
    source.derived.remove("delta");
    source.derived.insert("extra".to_string(), String::new());
    assert_eq!(
        evaluate(&narrow(q), std::slice::from_ref(&source), &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let input = json!({"Map":{"input":"All","field":"delta","pointer":"/current"}});
    assert_eq!(
        evaluate(
            &narrow(query(input, "delta", "missing", "previous")),
            &[source],
            &limits
        ),
        Err(EventQueryError::InvalidEvent)
    );
}

#[test]
fn shared_limits_moments_and_receipts_bind_operand_order_and_projection() {
    let rows = [event(0, "20", "8"), event(1, "20", "8")];
    let q = project(json!("All"));
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
    let q = project(input.clone());
    let result = evaluate(&q, &rows, &limits).unwrap();
    let receipt = receipt_for(&q, &rows, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(0));
    validate_receipt(&receipt, &q, &rows, &result, &limits).unwrap();
    for changed in [
        query(input.clone(), "other", "current", "previous"),
        query(input.clone(), "delta", "previous", "current"),
        query(input, "delta", "current", "current"),
    ] {
        assert!(validate_receipt(&receipt, &changed, &rows, &result, &limits).is_err());
    }
    let mut drift = rows.clone();
    drift[1].text = "unselected canonical drift".to_string();
    assert!(validate_receipt(&receipt, &q, &drift, &result, &limits).is_err());
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("delta".to_string(), "99".to_string());
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
