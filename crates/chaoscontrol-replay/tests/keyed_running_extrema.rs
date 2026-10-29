use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
const OPERATORS: [&str; 2] = ["RunningMinBy", "RunningMaxBy"];
fn event(i: u64, tick: u64, key: Option<&str>, reading: &str, kind: &str) -> EventEnvelope {
    let mut derived = BTreeMap::from([
        ("reading".into(), reading.into()),
        ("extremum".into(), "stale".into()),
    ]);
    if let Some(key) = key {
        derived.insert("owner".into(), key.into());
    }
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.into(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: i,
        tick,
        vm_index: Some(0),
        process: Some("monitor".into()),
        thread: None,
        kind: kind.into(),
        text: json!({"service":"a","other":"b","reading":2.5}).to_string(),
        payload_digest: [9; 32],
        details: Some(json!({"service":"a","other":"b","reading":2.5})),
        derived,
    }
}
fn query(
    op: &str,
    input: Value,
    predicate: Value,
    field: &str,
    value: &str,
    key: &str,
) -> EventQuery {
    serde_json::from_value(json!({op:{"input":input,"predicate":predicate,"field":field,"value_field":value,"key_field":key}})).expect("keyed extrema must be supported")
}
fn extrema(op: &str) -> EventQuery {
    query(
        op,
        json!("All"),
        json!({"Kind":"metric"}),
        "extremum",
        "reading",
        "owner",
    )
}
fn annotate(mut row: EventEnvelope, field: &str, value: Option<&str>) -> EventEnvelope {
    if let Some(value) = value {
        row.derived.insert(field.into(), value.into());
    } else {
        row.derived.remove(field);
    }
    row
}
fn fill_to(row: &mut EventEnvelope, bytes: usize) {
    row.derived.insert("pad".into(), String::new());
    let used: usize = row.derived.iter().map(|(k, v)| k.len() + v.len()).sum();
    row.derived
        .get_mut("pad")
        .unwrap()
        .push_str(&"x".repeat(bytes - used));
}
// r[verify chaoscontrol.keyed_running_extrema.prefixes]
#[test]
fn independent_extrema_match_pairwise_prefix_oracle_across_interleaving_and_tick_boundaries() {
    let mut rows = Vec::new();
    for (tick, reading, kind) in [
        (0, "bad", "skip"),
        (0, "-4", "metric"),
        (1, "-2.5", "metric"),
        (1, "-8", "metric"),
        (u64::MAX, "0.5", "metric"),
    ] {
        for key in [
            None,
            Some(""),
            Some("a"),
            Some("b"),
            Some("é"),
            Some("e\u{301}"),
        ] {
            rows.push(event(rows.len() as u64, tick, key, reading, kind));
        }
    }
    let mut tie = event(0, 1, Some("a"), "-10", "metric");
    tie.source_id = [2; 32];
    rows.push(tie);
    let mut ordered = rows.clone();
    ordered.sort_by_key(|r| (r.tick, r.source_id, r.source_sequence));
    for op in OPERATORS {
        let expected: Vec<_> = ordered
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let selected = row.derived.get("owner").and_then(|key| {
                    let candidates: Vec<_> = ordered[..=i]
                        .iter()
                        .filter(|r| r.derived.get("owner") == Some(key) && r.kind == "metric")
                        .map(|r| r.derived["reading"].parse::<f64>().unwrap())
                        .collect();
                    candidates.iter().copied().find(|n| {
                        candidates.iter().all(|other| {
                            if op == "RunningMinBy" {
                                n <= other
                            } else {
                                n >= other
                            }
                        })
                    })
                });
                let text = selected.map(|n| serde_json::Number::from_f64(n).unwrap().to_string());
                annotate(row.clone(), "extremum", text.as_deref())
            })
            .collect();
        for _ in 0..2 {
            for rotation in [0, 1, 7] {
                rows.rotate_left(rotation);
                assert_eq!(
                    evaluate(&extrema(op), &rows, &EventQueryLimits::default())
                        .unwrap()
                        .rows,
                    expected
                );
            }
            rows.reverse();
        }
        let no_match = query(
            op,
            json!("All"),
            json!({"Any":[]}),
            "extremum",
            "reading",
            "owner",
        );
        assert_eq!(
            evaluate(&no_match, &ordered, &EventQueryLimits::default())
                .unwrap()
                .rows,
            ordered
                .iter()
                .map(|r| annotate(r.clone(), "extremum", None))
                .collect::<Vec<_>>()
        );
    }
}
#[test]
fn aliases_duplicate_priority_and_equal_signed_zero_preserve_original_membership() {
    let limits = EventQueryLimits::default();
    let rows = [
        event(0, 0, Some("2"), "-4", "metric"),
        event(1, 1, Some("3"), "-2", "metric"),
        event(2, 2, Some("2"), "-6", "metric"),
    ];
    for op in OPERATORS {
        let values = if op == "RunningMinBy" {
            ["-4.0", "-2.0", "-6.0"]
        } else {
            ["-4.0", "-2.0", "-4.0"]
        };
        for field in ["owner", "reading", "kind"] {
            assert_eq!(
                evaluate(
                    &query(
                        op,
                        json!("All"),
                        json!({"Kind":"metric"}),
                        field,
                        "reading",
                        "owner"
                    ),
                    &rows,
                    &limits
                )
                .unwrap()
                .rows,
                rows.iter()
                    .zip(values)
                    .map(|(r, n)| annotate(r.clone(), field, Some(n)))
                    .collect::<Vec<_>>()
            );
        }
        let q = query(
            op,
            json!("All"),
            json!({"Matches":{"field":"owner","text":"2"}}),
            "owner",
            "owner",
            "owner",
        );
        assert_eq!(
            evaluate(&q, &rows, &limits).unwrap().rows,
            rows.iter()
                .zip([Some("2.0"), None, Some("2.0")])
                .map(|(r, n)| annotate(r.clone(), "owner", n))
                .collect::<Vec<_>>()
        );
        let mut duplicate = rows[0].clone();
        duplicate.derived.insert("owner".into(), "wrong".into());
        duplicate.derived.insert("reading".into(), "bad".into());
        assert_eq!(
            evaluate(&extrema(op), &[rows[0].clone(), duplicate.clone()], &limits)
                .unwrap()
                .rows,
            vec![annotate(rows[0].clone(), "extremum", Some("-4.0"))]
        );
        assert_eq!(
            evaluate(&extrema(op), &[duplicate, rows[0].clone()], &limits),
            Err(EventQueryError::InvalidNumber)
        );
        for (first, second, want) in [
            ("-0", "0", "-0.0"),
            ("0", "-0", "0.0"),
            ("-1e-400", "0", "-0.0"),
            ("9007199254740992", "9007199254740993", "9007199254740992.0"),
        ] {
            let src = [
                event(0, 0, Some("a"), first, "metric"),
                event(1, 0, Some("b"), second, "metric"),
                event(2, 0, Some("a"), second, "metric"),
            ];
            let middle = serde_json::Number::from_f64(second.parse().unwrap())
                .unwrap()
                .to_string();
            assert_eq!(
                evaluate(&extrema(op), &src, &limits).unwrap().rows,
                vec![
                    annotate(src[0].clone(), "extremum", Some(want)),
                    annotate(src[1].clone(), "extremum", Some(&middle)),
                    annotate(src[2].clone(), "extremum", Some(want))
                ]
            );
        }
    }
}
#[test]
fn missing_keys_validate_matching_numbers_and_predicates_but_not_skipped_operands() {
    let limits = EventQueryLimits::default();
    for op in OPERATORS {
        for key in [None, Some("a")] {
            for bad in ["", "bad", "01", "+1", "null", "true", "\"2\"", "1e309"] {
                assert_eq!(
                    evaluate(&extrema(op), &[event(0, 0, key, bad, "metric")], &limits),
                    Err(EventQueryError::InvalidNumber)
                );
            }
            let src = event(0, 0, key, "bad", "skip");
            assert_eq!(
                evaluate(&extrema(op), std::slice::from_ref(&src), &limits)
                    .unwrap()
                    .rows,
                vec![annotate(src, "extremum", None)]
            );
        }
        let src = event(0, 0, None, "2", "metric");
        assert_eq!(
            evaluate(&extrema(op), std::slice::from_ref(&src), &limits)
                .unwrap()
                .rows,
            vec![annotate(src, "extremum", None)]
        );
        let mut src = event(0, 0, None, "2", "metric");
        src.text = "bad JSON".into();
        let predicate = json!({"JsonTextPresent":{"pointer":"/x"}});
        assert_eq!(
            evaluate(
                &query(
                    op,
                    json!("All"),
                    predicate.clone(),
                    "extremum",
                    "reading",
                    "owner"
                ),
                std::slice::from_ref(&src),
                &limits
            ),
            Err(EventQueryError::InvalidJsonText)
        );
        let skipped = query(
            op,
            json!("All"),
            json!({"Any":[{"All":[]},predicate]}),
            "extremum",
            "reading",
            "owner",
        );
        assert_eq!(
            evaluate(&skipped, std::slice::from_ref(&src), &limits)
                .unwrap()
                .rows,
            vec![annotate(src, "extremum", None)]
        );
    }
}
#[test]
fn canonical_keys_and_json_projections_compose_with_downstream_selection() {
    let limits = EventQueryLimits::default();
    let mut rows = [
        event(0, 0, Some("a"), "2", "metric"),
        event(1, 1, Some("b"), "4", "skip"),
        event(2, 2, Some("a"), "3", "metric"),
    ];
    for op in OPERATORS {
        for key in ["kind", "process", "source_kind", "text"] {
            for row in &mut rows {
                row.derived.insert(key.into(), "shadow".into());
            }
            let values = if op == "RunningMinBy" {
                if key == "kind" {
                    ["2.0", "4.0", "2.0"]
                } else {
                    ["2.0", "2.0", "2.0"]
                }
            } else if key == "kind" {
                ["2.0", "4.0", "3.0"]
            } else {
                ["2.0", "4.0", "4.0"]
            };
            assert_eq!(
                evaluate(
                    &query(
                        op,
                        json!("All"),
                        json!({"All":[]}),
                        "extremum",
                        "reading",
                        key
                    ),
                    &rows,
                    &limits
                )
                .unwrap()
                .rows,
                rows.iter()
                    .zip(values)
                    .map(|(r, n)| annotate(r.clone(), "extremum", Some(n)))
                    .collect::<Vec<_>>()
            );
        }
        for value in ["kind", "process", "source_kind", "text"] {
            assert_eq!(
                evaluate(
                    &query(
                        op,
                        json!("All"),
                        json!({"All":[]}),
                        "extremum",
                        value,
                        "owner"
                    ),
                    &rows,
                    &limits
                ),
                Err(EventQueryError::InvalidNumber)
            );
        }
        for project in ["Map", "MapText"] {
            let input = json!({project:{"input":{project:{"input":"All","field":"owner","pointer":"/service"}},"field":"reading","pointer":"/reading"}});
            let q = query(
                op,
                json!({"Union":{"inputs":[input.clone(),{project:{"input":input,"field":"owner","pointer":"/other"}}]}}),
                json!({"Kind":"metric"}),
                "extremum",
                "reading",
                "owner",
            );
            let q:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":q,"predicate":{"FieldNumber":{"field":"extremum","comparison":"Equal","value":2.5}}}},"fields":["owner","extremum"]}})).unwrap();
            let want: Vec<_> = rows
                .iter()
                .map(|r| {
                    let mut r = r.clone();
                    r.derived = BTreeMap::from([
                        ("owner".into(), "a".into()),
                        ("extremum".into(), "2.5".into()),
                    ]);
                    r
                })
                .collect();
            assert_eq!(evaluate(&q, &rows, &limits).unwrap().rows, want);
        }
    }
}
#[test]
fn admission_and_capacity_cannot_be_hidden_by_empty_sources_or_narrowing() {
    let limits = EventQueryLimits::default();
    let narrow = |input| EventQuery::Narrow {
        input: Box::new(input),
        fields: vec![],
    };
    for op in OPERATORS {
        for bad in [
            String::new(),
            "x\ny".into(),
            "é".repeat(MAX_FIELD_BYTES / 2) + "x",
        ] {
            for (field, value, key) in [
                (bad.as_str(), "reading", "owner"),
                ("extremum", bad.as_str(), "owner"),
                ("extremum", "reading", bad.as_str()),
            ] {
                assert_eq!(
                    evaluate(
                        &query(op, json!("All"), json!({"All":[]}), field, value, key),
                        &[],
                        &limits
                    ),
                    Err(EventQueryError::InvalidField)
                );
            }
        }
        assert_eq!(
            evaluate(
                &query(
                    op,
                    json!({"Map":{"input":"All","field":"x","pointer":"bad"}}),
                    json!({"All":[]}),
                    "extremum",
                    "reading",
                    "owner"
                ),
                &[],
                &limits
            ),
            Err(EventQueryError::InvalidPointer)
        );
        assert_eq!(
            evaluate(
                &query(
                    op,
                    json!("All"),
                    json!({"Regex":{"field":"kind","pattern":"["}}),
                    "extremum",
                    "reading",
                    "owner"
                ),
                &[],
                &limits
            ),
            Err(EventQueryError::InvalidRegex)
        );
        let name = "é".repeat(MAX_FIELD_BYTES / 2);
        let mut src = event(0, 0, None, "2", "metric");
        src.derived.insert(name.clone(), "2".into());
        assert_eq!(
            evaluate(
                &query(op, json!("All"), json!({"All":[]}), &name, &name, &name),
                std::slice::from_ref(&src),
                &limits
            )
            .unwrap()
            .rows,
            vec![annotate(src, &name, Some("2.0"))]
        );
        let mut src = event(0, 0, Some("a"), "2", "metric");
        src.derived.remove("extremum");
        fill_to(&mut src, MAX_DERIVED_BYTES - "extremum".len() - 3);
        assert_eq!(
            evaluate(&extrema(op), std::slice::from_ref(&src), &limits)
                .unwrap()
                .rows,
            vec![annotate(src.clone(), "extremum", Some("2.0"))]
        );
        src.derived.get_mut("pad").unwrap().push('x');
        assert_eq!(
            evaluate(&narrow(extrema(op)), &[src], &limits),
            Err(EventQueryError::InvalidEvent)
        );
        let mut src = event(0, 0, Some("a"), "2", "metric");
        while src.derived.len() < MAX_DERIVED_FIELDS {
            src.derived
                .insert(format!("f{}", src.derived.len()), String::new());
        }
        assert_eq!(
            evaluate(&extrema(op), std::slice::from_ref(&src), &limits)
                .unwrap()
                .rows,
            vec![annotate(src.clone(), "extremum", Some("2.0"))]
        );
        src.derived.remove("extremum");
        src.derived.insert("extra".into(), String::new());
        assert_eq!(
            evaluate(&narrow(extrema(op)), &[src], &limits),
            Err(EventQueryError::InvalidEvent)
        );
        let mut src = event(0, 0, None, "2", "metric");
        fill_to(&mut src, MAX_DERIVED_BYTES);
        let input = json!({"Map":{"input":"All","field":"overflow","pointer":"/service"}});
        assert_eq!(
            evaluate(
                &narrow(query(
                    op,
                    input,
                    json!({"All":[]}),
                    "extremum",
                    "reading",
                    "owner"
                )),
                &[src],
                &limits
            ),
            Err(EventQueryError::InvalidEvent)
        );
    }
}
#[test]
fn shared_limits_moments_and_receipts_bind_operator_keys_and_all_evidence() {
    let rows = [
        event(0, 0, Some("a"), "2", "metric"),
        event(1, 1, Some("b"), "4", "metric"),
        event(2, 2, Some("a"), "3", "metric"),
    ];
    for op in OPERATORS {
        let exact = EventQueryLimits {
            max_nodes: 2,
            max_depth: 1,
            max_scanned_events: 3,
            max_operations: 6,
            ..EventQueryLimits::default()
        };
        let q = extrema(op);
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
                    max_nodes: 1,
                    ..exact
                },
                EventQueryError::QueryBound,
            ),
            (
                EventQueryLimits {
                    max_operations: 5,
                    ..exact
                },
                EventQueryError::OperationBound,
            ),
            (
                EventQueryLimits {
                    max_scanned_events: 2,
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
        let prefix = evaluate(
            &q,
            &rows,
            &EventQueryLimits {
                max_result_rows: 2,
                ..exact
            },
        )
        .unwrap();
        assert_eq!(prefix.rows, result.rows[..2]);
        assert_eq!(prefix.matched, 3);
        assert!(prefix.truncated);
        assert_eq!(
            evaluate(
                &EventQuery::UpTo {
                    input: Box::new(q),
                    moment_tick: 2
                },
                &rows,
                &exact
            ),
            Err(EventQueryError::QueryDepth)
        );
        let limits = EventQueryLimits::default();
        let input = json!({"UpTo":{"input":"All","moment_tick":1}});
        let q = query(
            op,
            input.clone(),
            json!({"Kind":"metric"}),
            "extremum",
            "reading",
            "owner",
        );
        let result = evaluate(&q, &rows, &limits).unwrap();
        let receipt = receipt_for(&q, &rows, &result, &limits).unwrap();
        assert_eq!(receipt.moment_tick, Some(1));
        validate_receipt(&receipt, &q, &rows, &result, &limits).unwrap();
        let other = if op == "RunningMinBy" {
            "RunningMaxBy"
        } else {
            "RunningMinBy"
        };
        for changed in [
            query(
                other,
                input.clone(),
                json!({"Kind":"metric"}),
                "extremum",
                "reading",
                "owner",
            ),
            query(
                op,
                input.clone(),
                json!({"Kind":"metric"}),
                "extremum",
                "reading",
                "process",
            ),
            query(
                op,
                input.clone(),
                json!({"All":[]}),
                "extremum",
                "reading",
                "owner",
            ),
            query(
                op,
                input.clone(),
                json!({"Kind":"metric"}),
                "extremum",
                "owner",
                "owner",
            ),
            query(
                op,
                input,
                json!({"Kind":"metric"}),
                "other",
                "reading",
                "owner",
            ),
        ] {
            assert!(validate_receipt(&receipt, &changed, &rows, &result, &limits).is_err());
        }
        let mut drift = rows.clone();
        drift[2].text = "unselected drift".into();
        assert!(validate_receipt(&receipt, &q, &drift, &result, &limits).is_err());
        let mut forged = result.clone();
        forged.rows[0]
            .derived
            .insert("extremum".into(), "99".into());
        assert!(validate_receipt(&receipt, &q, &rows, &forged, &limits).is_err());
        let mixed: EventQuery = serde_json::from_value(
            json!({"Union":{"inputs":[q,{"UpTo":{"input":"All","moment_tick":2}}]}}),
        )
        .unwrap();
        assert_eq!(
            evaluate(&mixed, &rows, &limits),
            Err(EventQueryError::AmbiguousMoment)
        );
    }
}
