use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const OPERATORS: [&str; 2] = ["RunningMin", "RunningMax"];
fn event(sequence: u64, tick: u64, reading: &str, kind: &str) -> EventEnvelope {
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
        text: json!({"reading":"7.5"}).to_string(),
        payload_digest: [9; 32],
        details: Some(json!({"reading":7.5})),
        derived: BTreeMap::from([
            ("reading".to_string(), reading.to_string()),
            ("extremum".to_string(), "obsolete".to_string()),
        ]),
    }
}
fn query(
    operator: &str,
    input: Value,
    predicate: Value,
    field: &str,
    value_field: &str,
) -> EventQuery {
    serde_json::from_value(json!({operator:{"input":input,"predicate":predicate,"field":field,"value_field":value_field}})).expect("numeric extrema must be supported")
}
fn matching() -> Value {
    json!({"Kind":"metric"})
}
fn annotate(mut row: EventEnvelope, field: &str, value: Option<&str>) -> EventEnvelope {
    if let Some(value) = value {
        row.derived.insert(field.to_string(), value.to_string());
    } else {
        row.derived.remove(field);
    }
    row
}

// r[verify chaoscontrol.running_extrema.prefixes]
#[test]
fn extrema_preserve_canonical_unique_prefixes_without_a_zero_seed() {
    let original = vec![
        event(0, 0, "bad", "skip"),
        event(1, 0, "-4", "metric"),
        event(2, 1, "-2", "metric"),
        event(3, 1, "missing", "skip"),
        event(4, u64::MAX, "8", "metric"),
        event(5, u64::MAX, "-9", "metric"),
    ];
    let limits = EventQueryLimits::default();
    for (operator, values) in [
        (
            "RunningMin",
            [
                None,
                Some("-4.0"),
                Some("-4.0"),
                Some("-4.0"),
                Some("-4.0"),
                Some("-9.0"),
            ],
        ),
        (
            "RunningMax",
            [
                None,
                Some("-4.0"),
                Some("-2.0"),
                Some("-2.0"),
                Some("8.0"),
                Some("8.0"),
            ],
        ),
    ] {
        for field in ["extremum", "reading", "kind"] {
            let expected: Vec<_> = original
                .iter()
                .cloned()
                .zip(values)
                .map(|(row, value)| annotate(row, field, value))
                .collect();
            let q = query(operator, json!("All"), matching(), field, "reading");
            let mut input = original.clone();
            input.push(original[1].clone());
            for _ in 0..2 {
                assert_eq!(evaluate(&q, &input, &limits).unwrap().rows, expected);
                input.reverse();
            }
        }
        let no_matches = query(
            operator,
            json!("All"),
            json!({"Any":[]}),
            "extremum",
            "reading",
        );
        let expected: Vec<_> = original
            .iter()
            .cloned()
            .map(|row| annotate(row, "extremum", None))
            .collect();
        assert_eq!(
            evaluate(&no_matches, &original, &limits).unwrap().rows,
            expected
        );
        assert_eq!(
            evaluate(&no_matches, &[], &limits).unwrap().rows,
            Vec::<EventEnvelope>::new()
        );
    }
}

#[test]
fn duplicate_projection_priority_and_equal_signed_zero_are_stable() {
    let limits = EventQueryLimits::default();
    for operator in OPERATORS {
        let q = query(operator, json!("All"), matching(), "extremum", "reading");
        let first = event(0, 0, "2", "metric");
        let mut duplicate = first.clone();
        duplicate
            .derived
            .insert("reading".to_string(), "bad".to_string());
        assert_eq!(
            evaluate(&q, &[first.clone(), duplicate.clone()], &limits)
                .unwrap()
                .rows,
            vec![annotate(first.clone(), "extremum", Some("2.0"))]
        );
        assert_eq!(
            evaluate(&q, &[duplicate, first], &limits),
            Err(EventQueryError::InvalidNumber)
        );
        for (a, b, expected) in [
            ("-0", "0", "-0.0"),
            ("0", "-0", "0.0"),
            ("-1e-400", "0", "-0.0"),
        ] {
            let rows = [event(0, 0, a, "metric"), event(1, 0, b, "metric")];
            let expected: Vec<_> = rows
                .iter()
                .cloned()
                .map(|row| annotate(row, "extremum", Some(expected)))
                .collect();
            assert_eq!(evaluate(&q, &rows, &limits).unwrap().rows, expected);
        }
    }
    for (operator, values) in [
        (
            "RunningMin",
            [
                "9007199254740992.0",
                "9007199254740992.0",
                "-1e+308",
                "-1e+308",
            ],
        ),
        (
            "RunningMax",
            [
                "9007199254740992.0",
                "9007199254740992.0",
                "9007199254740992.0",
                "1e+308",
            ],
        ),
    ] {
        let rows: Vec<_> = ["9007199254740993", "9007199254740992", "-1e308", "1e308"]
            .iter()
            .enumerate()
            .map(|(i, v)| event(i as u64, i as u64, v, "metric"))
            .collect();
        let expected: Vec<_> = rows
            .iter()
            .cloned()
            .zip(values)
            .map(|(row, value)| annotate(row, "extremum", Some(value)))
            .collect();
        assert_eq!(
            evaluate(
                &query(operator, json!("All"), matching(), "extremum", "reading"),
                &rows,
                &limits
            )
            .unwrap()
            .rows,
            expected
        );
    }
}

#[test]
fn matching_numbers_are_required_but_skipped_operands_are_not_parsed() {
    let limits = EventQueryLimits::default();
    for operator in OPERATORS {
        let q = query(operator, json!("All"), matching(), "extremum", "reading");
        for text in [
            "",
            "null",
            "true",
            "\"2\"",
            "[]",
            "{}",
            "NaN",
            "Infinity",
            "1e400",
            "01",
            "+1",
            "1 trailing",
        ] {
            assert_eq!(
                evaluate(&q, &[event(0, 0, text, "metric")], &limits),
                Err(EventQueryError::InvalidNumber)
            );
            let skipped = event(0, 0, text, "skip");
            assert_eq!(
                evaluate(&q, std::slice::from_ref(&skipped), &limits)
                    .unwrap()
                    .rows,
                vec![annotate(skipped, "extremum", None)]
            );
        }
        let mut missing = event(0, 0, "1", "metric");
        missing.derived.remove("reading");
        assert_eq!(
            evaluate(&q, std::slice::from_ref(&missing), &limits),
            Err(EventQueryError::InvalidNumber)
        );
        missing.kind = "skip".to_string();
        assert_eq!(
            evaluate(&q, std::slice::from_ref(&missing), &limits)
                .unwrap()
                .rows,
            vec![annotate(missing, "extremum", None)]
        );
        let mut row = event(0, 0, "2", "metric");
        row.derived.insert("kind".to_string(), "3".to_string());
        assert_eq!(
            evaluate(
                &query(operator, json!("All"), matching(), "extremum", "kind"),
                std::slice::from_ref(&row),
                &limits
            ),
            Err(EventQueryError::InvalidNumber)
        );
        row.derived.insert("process".to_string(), "3".to_string());
        assert_eq!(
            evaluate(
                &query(operator, json!("All"), matching(), "extremum", "process"),
                &[row],
                &limits
            ),
            Err(EventQueryError::InvalidNumber)
        );
    }
}

#[test]
fn projected_extrema_compose_with_numeric_filters_and_admit_names_and_predicates() {
    let limits = EventQueryLimits::default();
    for operator in OPERATORS {
        let mut rows = [event(0, 0, "bad", "metric"), event(1, 1, "bad", "metric")];
        rows[1].details = Some(json!({"reading":-2.5}));
        rows[1].text = json!({"reading":"-2.5"}).to_string();
        for projection in ["Map", "MapText"] {
            let input = json!({projection:{"input":"All","field":"reading","pointer":"/reading"}});
            let comparison = if operator == "RunningMin" {
                "LessThan"
            } else {
                "GreaterThan"
            };
            let threshold = if operator == "RunningMin" { 0 } else { 7 };
            let q:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":query(operator,input,matching(),"extremum","reading"),"predicate":{"FieldNumber":{"field":"extremum","comparison":comparison,"value":threshold}}}},"fields":["extremum"]}})).unwrap();
            let indices: &[usize] = if operator == "RunningMin" {
                &[1]
            } else {
                &[0, 1]
            };
            let value = if operator == "RunningMin" {
                "-2.5"
            } else {
                "7.5"
            };
            let expected: Vec<_> = indices
                .iter()
                .map(|&i| {
                    let mut row = rows[i].clone();
                    row.derived = BTreeMap::from([("extremum".to_string(), value.to_string())]);
                    row
                })
                .collect();
            assert_eq!(evaluate(&q, &rows, &limits).unwrap().rows, expected);
        }
        for invalid in [
            String::new(),
            "a\nb".to_string(),
            "é".repeat(MAX_FIELD_BYTES / 2) + "x",
        ] {
            for (field, value_field) in [(&invalid[..], "reading"), ("extremum", &invalid[..])] {
                assert_eq!(
                    evaluate(
                        &query(operator, json!("All"), matching(), field, value_field),
                        &[],
                        &limits
                    ),
                    Err(EventQueryError::InvalidField)
                );
            }
        }
        let exact = "é".repeat(MAX_FIELD_BYTES / 2);
        let mut source = event(0, 0, "2", "metric");
        source.derived.insert(exact.clone(), "2".to_string());
        assert_eq!(
            evaluate(
                &query(operator, json!("All"), matching(), &exact, &exact),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![annotate(source, &exact, Some("2.0"))]
        );
        assert_eq!(
            evaluate(
                &query(
                    operator,
                    json!("All"),
                    json!({"Any":[{"All":[]},{"Regex":{"field":"kind","pattern":"["}}]}),
                    "extremum",
                    "reading"
                ),
                &[],
                &limits
            ),
            Err(EventQueryError::InvalidRegex)
        );
        let leaf =
            json!({"JsonTextNumber":{"pointer":"/reading","comparison":"GreaterThan","value":0}});
        let mut source = event(0, 0, "2", "metric");
        source.text = "bad".to_string();
        assert_eq!(
            evaluate(
                &query(
                    operator,
                    json!("All"),
                    json!({"Any":[{"All":[]},leaf]}),
                    "extremum",
                    "reading"
                ),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![annotate(source.clone(), "extremum", Some("2.0"))]
        );
        assert_eq!(
            evaluate(
                &query(operator, json!("All"), leaf, "extremum", "reading"),
                &[source],
                &limits
            ),
            Err(EventQueryError::InvalidJsonText)
        );
    }
}

#[test]
fn generated_and_intermediate_capacity_cannot_be_hidden_by_narrowing_or_no_match() {
    let limits = EventQueryLimits::default();
    let narrow = |input| EventQuery::Narrow {
        input: Box::new(input),
        fields: vec!["extremum".to_string()],
    };
    for operator in OPERATORS {
        let q = query(operator, json!("All"), matching(), "extremum", "reading");
        let mut source = event(0, 0, "2", "metric");
        source.derived.remove("extremum");
        source.derived.insert("pad".to_string(), String::new());
        let used: usize = source.derived.iter().map(|(k, v)| k.len() + v.len()).sum();
        source
            .derived
            .get_mut("pad")
            .unwrap()
            .push_str(&"x".repeat(MAX_DERIVED_BYTES - "extremum".len() - "2.0".len() - used));
        assert_eq!(
            evaluate(&q, std::slice::from_ref(&source), &limits)
                .unwrap()
                .rows,
            vec![annotate(source.clone(), "extremum", Some("2.0"))]
        );
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
        assert_eq!(
            evaluate(&q, std::slice::from_ref(&source), &limits)
                .unwrap()
                .rows,
            vec![annotate(source.clone(), "extremum", Some("2.0"))]
        );
        source.derived.remove("extremum");
        source.derived.insert("extra".to_string(), String::new());
        assert_eq!(
            evaluate(&narrow(q), std::slice::from_ref(&source), &limits),
            Err(EventQueryError::InvalidEvent)
        );
        let input = json!({"Map":{"input":"All","field":"extremum","pointer":"/reading"}});
        assert_eq!(
            evaluate(
                &narrow(query(
                    operator,
                    input,
                    json!({"Any":[]}),
                    "extremum",
                    "reading"
                )),
                &[source],
                &limits
            ),
            Err(EventQueryError::InvalidEvent)
        );
    }
}

#[test]
fn work_limits_moments_and_receipts_bind_extrema_and_operator_choice() {
    let rows = [event(0, 0, "2", "metric"), event(1, 1, "3", "metric")];
    for operator in OPERATORS {
        let q = query(operator, json!("All"), matching(), "extremum", "reading");
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
        let compound = query(
            operator,
            json!("All"),
            json!({"All":[matching()]}),
            "extremum",
            "reading",
        );
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
        let q = query(operator, input.clone(), matching(), "extremum", "reading");
        let result = evaluate(&q, &rows, &limits).unwrap();
        let receipt = receipt_for(&q, &rows, &result, &limits).unwrap();
        assert_eq!(receipt.moment_tick, Some(0));
        validate_receipt(&receipt, &q, &rows, &result, &limits).unwrap();
        let other = if operator == "RunningMin" {
            "RunningMax"
        } else {
            "RunningMin"
        };
        for changed in [
            query(other, input.clone(), matching(), "extremum", "reading"),
            query(operator, input.clone(), matching(), "other", "reading"),
            query(operator, input.clone(), matching(), "extremum", "extremum"),
            query(operator, input, json!({"All":[]}), "extremum", "reading"),
        ] {
            assert!(validate_receipt(&receipt, &changed, &rows, &result, &limits).is_err());
        }
        let mut drift = rows.clone();
        drift[1].text = "unselected canonical drift".to_string();
        assert!(validate_receipt(&receipt, &q, &drift, &result, &limits).is_err());
        let mut forged = result.clone();
        forged.rows[0]
            .derived
            .insert("extremum".to_string(), "99".to_string());
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
}
