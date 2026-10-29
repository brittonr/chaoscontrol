use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn event(seq: u64, tick: u64, kind: &str, key: Option<&str>) -> EventEnvelope {
    let mut derived = BTreeMap::from([("next".into(), "stale".into())]);
    if let Some(key) = key {
        derived.insert("owner".into(), key.into());
    }
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.into(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: seq,
        tick,
        vm_index: Some(0),
        process: Some("monitor".into()),
        thread: None,
        kind: kind.into(),
        text: json!({"service":"a","value":20,"other":"b"}).to_string(),
        payload_digest: [2; 32],
        details: Some(json!({"service":"a","value":20,"other":"b"})),
        derived,
    }
}
fn filter(kind: &str) -> Value {
    json!({"Filter":{"input":"All","predicate":{"Kind":kind}}})
}
fn query(
    input: Value,
    witness: Value,
    field: &str,
    value: &str,
    within: u64,
    key: Option<&str>,
) -> EventQuery {
    let mut body = json!({"input":input,"witness":witness,"field":field,"witness_field":value,"within_ticks":within});
    let op = if let Some(key) = key {
        body["key_field"] = json!(key);
        "WithNextValueBy"
    } else {
        "WithNextValue"
    };
    serde_json::from_value(json!({op:body})).expect("forward witness annotation must be supported")
}
fn next(within: u64, key: Option<&str>) -> EventQuery {
    query(
        filter("candidate"),
        filter("witness"),
        "next",
        "text",
        within,
        key,
    )
}
fn annotated(mut row: EventEnvelope, field: &str, value: Option<&str>) -> EventEnvelope {
    if let Some(v) = value {
        row.derived.insert(field.into(), v.into());
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
// r[verify chaoscontrol.next_witness_value.annotation]
#[test]
fn forward_values_match_pairwise_oracle_across_keys_ties_permutations_and_tick_boundaries() {
    let mut rows = Vec::new();
    for key in [
        None,
        Some(""),
        Some("a"),
        Some("b"),
        Some("é"),
        Some("e\u{301}"),
    ] {
        for tick in [0, 1, 3, 8, u64::MAX - 1, u64::MAX] {
            rows.push(event(rows.len() as u64, tick, "candidate", key));
        }
        for tick in [0, 2, 3, 7, u64::MAX] {
            let mut r = event(rows.len() as u64, tick, "witness", key);
            r.text = format!("witness-{}", r.source_sequence);
            rows.push(r);
        }
    }
    let mut tie = event(0, 2, "witness", Some("a"));
    tie.source_id = [2; 32];
    tie.text = "tie".into();
    rows.push(tie);
    for key_field in [None, Some("owner")] {
        for window in [0, 1, 4, u64::MAX] {
            let mut expected = Vec::new();
            for row in rows.iter().filter(|r| r.kind == "candidate") {
                let witness = rows
                    .iter()
                    .filter(|w| {
                        w.kind == "witness"
                            && w.tick
                                .checked_sub(row.tick)
                                .is_some_and(|d| d > 0 && d <= window)
                            && key_field.is_none_or(|k| {
                                row.derived.contains_key(k)
                                    && row.derived.get(k) == w.derived.get(k)
                            })
                    })
                    .min_by_key(|w| (w.tick, w.source_id, w.source_sequence));
                expected.push(annotated(
                    row.clone(),
                    "next",
                    witness.map(|w| w.text.as_str()),
                ));
            }
            expected.sort_by_key(|r| (r.tick, r.source_id, r.source_sequence));
            for _ in 0..2 {
                assert_eq!(
                    evaluate(
                        &next(window, key_field),
                        &rows,
                        &EventQueryLimits::default()
                    )
                    .unwrap()
                    .rows,
                    expected
                );
                rows.reverse();
            }
        }
    }
}
#[test]
fn nearest_missing_values_empty_strings_aliases_and_duplicate_projections_are_explicit() {
    let limits = EventQueryLimits::default();
    let mut rows = [
        event(0, 1, "candidate", Some("a")),
        event(1, 2, "witness", Some("a")),
        event(2, 3, "witness", Some("a")),
    ];
    rows[2].derived.insert("value".into(), "farther".into());
    for key in [None, Some("owner")] {
        let q = query(
            filter("candidate"),
            filter("witness"),
            "next",
            "value",
            5,
            key,
        );
        assert_eq!(
            evaluate(&q, &rows, &limits).unwrap().rows,
            vec![annotated(rows[0].clone(), "next", None)]
        );
        rows[1].derived.insert("value".into(), String::new());
        assert_eq!(
            evaluate(&q, &rows, &limits).unwrap().rows,
            vec![annotated(rows[0].clone(), "next", Some(""))]
        );
        rows[1].derived.remove("value");
        for field in ["owner", "text", "value"] {
            let q = query(
                filter("candidate"),
                filter("witness"),
                field,
                "text",
                5,
                key,
            );
            assert_eq!(
                evaluate(&q, &rows, &limits).unwrap().rows,
                vec![annotated(rows[0].clone(), field, Some(&rows[1].text))]
            );
        }
    }
    let project =
        |pointer| json!({"Map":{"input":filter("witness"),"field":"owner","pointer":pointer}});
    let input = json!({"Map":{"input":filter("candidate"),"field":"owner","pointer":"/other"}});
    let witnesses = json!({"Union":{"inputs":[project("/service"),project("/other")]}});
    let q = query(input, witnesses, "next", "text", 5, Some("owner"));
    let mut expected = annotated(rows[0].clone(), "next", None);
    expected.derived.insert("owner".into(), "b".into());
    assert_eq!(evaluate(&q, &rows, &limits).unwrap().rows, vec![expected]);
    let mut duplicate = rows[1].clone();
    duplicate
        .derived
        .insert("value".into(), "later projection".into());
    let q = query(
        filter("candidate"),
        filter("witness"),
        "next",
        "value",
        5,
        None,
    );
    assert_eq!(
        evaluate(&q, &[rows[0].clone(), rows[1].clone(), duplicate], &limits)
            .unwrap()
            .rows,
        vec![annotated(rows[0].clone(), "next", None)]
    );
}
#[test]
fn canonical_keys_and_values_and_json_downstream_composition_preserve_evidence() {
    let limits = EventQueryLimits::default();
    let mut rows = [
        event(0, 1, "candidate", Some("a")),
        event(1, 2, "witness", Some("b")),
    ];
    for key in ["process", "source_kind", "text"] {
        for row in &mut rows {
            row.derived.insert(key.into(), "shadow".into());
        }
        assert_eq!(
            evaluate(&next(5, Some(key)), &rows, &limits).unwrap().rows,
            vec![annotated(rows[0].clone(), "next", Some(&rows[1].text))]
        );
    }
    assert_eq!(
        evaluate(&next(5, Some("kind")), &rows, &limits)
            .unwrap()
            .rows,
        vec![annotated(rows[0].clone(), "next", None)]
    );
    rows[1].process = None;
    assert_eq!(
        evaluate(&next(5, Some("process")), &rows, &limits)
            .unwrap()
            .rows,
        vec![annotated(rows[0].clone(), "next", None)]
    );
    for key in [None, Some("owner")] {
        for (field, want) in [
            ("kind", Some("witness")),
            ("process", None),
            ("source_kind", Some("serial")),
        ] {
            assert_eq!(
                evaluate(
                    &query(
                        filter("candidate"),
                        filter("witness"),
                        "kind",
                        field,
                        5,
                        key.map(|_| "text")
                    ),
                    &rows,
                    &limits
                )
                .unwrap()
                .rows,
                vec![annotated(rows[0].clone(), "kind", want)]
            );
        }
    }
    for project in ["Map", "MapText"] {
        let input =
            json!({project:{"input":filter("candidate"),"field":"owner","pointer":"/service"}});
        let witness = json!({project:{"input":{project:{"input":filter("witness"),"field":"owner","pointer":"/service"}},"field":"amount","pointer":"/value"}});
        let q = query(input, witness, "next", "amount", 5, Some("owner"));
        let q:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":q,"predicate":{"FieldNumber":{"field":"next","comparison":"Equal","value":20}}}},"fields":["next"]}})).unwrap();
        let mut want = rows[0].clone();
        want.derived = BTreeMap::from([("next".into(), "20".into())]);
        assert_eq!(evaluate(&q, &rows, &limits).unwrap().rows, vec![want]);
    }
}
#[test]
fn both_children_and_all_names_validate_even_for_empty_candidates_and_zero_windows() {
    let limits = EventQueryLimits::default();
    for key in [None, Some("owner")] {
        for bad in [
            String::new(),
            "a\nb".into(),
            "é".repeat(MAX_FIELD_BYTES / 2) + "x",
        ] {
            for (field, value) in [(bad.as_str(), "text"), ("next", bad.as_str())] {
                assert_eq!(
                    evaluate(
                        &query(filter("absent"), filter("witness"), field, value, 0, key),
                        &[],
                        &limits
                    ),
                    Err(EventQueryError::InvalidField)
                );
            }
            if key.is_some() {
                assert_eq!(
                    evaluate(
                        &query(json!("All"), json!("All"), "next", "text", 0, Some(&bad)),
                        &[],
                        &limits
                    ),
                    Err(EventQueryError::InvalidField)
                );
            }
        }
        let bad = json!({"Map":{"input":"All","field":"x","pointer":"bad"}});
        for q in [
            query(bad.clone(), json!("All"), "next", "text", 0, key),
            query(json!("All"), bad, "next", "text", 0, key),
        ] {
            assert_eq!(
                evaluate(&q, &[], &limits),
                Err(EventQueryError::InvalidPointer)
            );
        }
        let mut rows = [
            event(0, 1, "candidate", Some("a")),
            event(1, 2, "witness", Some("a")),
        ];
        rows[1].text = "invalid JSON".into();
        let bad = json!({"MapText":{"input":filter("witness"),"field":"x","pointer":"/value"}});
        assert_eq!(
            evaluate(
                &query(filter("absent"), bad, "next", "x", 0, key),
                &rows,
                &limits
            ),
            Err(EventQueryError::InvalidJsonText)
        );
        let name = "é".repeat(MAX_FIELD_BYTES / 2);
        for r in &mut rows {
            r.derived.insert(name.clone(), "a".into());
        }
        let q = query(
            filter("candidate"),
            filter("witness"),
            &name,
            &name,
            1,
            key.map(|_| name.as_str()),
        );
        assert_eq!(
            evaluate(&q, &rows, &limits).unwrap().rows,
            vec![annotated(rows[0].clone(), &name, Some("a"))]
        );
    }
}
#[test]
fn intermediate_and_generated_capacity_cannot_be_hidden_by_narrowing_or_duplicate_witnesses() {
    let limits = EventQueryLimits::default();
    let narrow = |q| EventQuery::Narrow {
        input: Box::new(q),
        fields: vec![],
    };
    for key in [None, Some("owner")] {
        let mut rows = [
            event(0, 1, "candidate", Some("a")),
            event(1, 2, "witness", Some("a")),
        ];
        rows[1].derived.insert("value".into(), "v".into());
        rows[0].derived.remove("next");
        fill_to(&mut rows[0], MAX_DERIVED_BYTES - 5);
        let q = query(
            filter("candidate"),
            filter("witness"),
            "next",
            "value",
            5,
            key,
        );
        assert_eq!(
            evaluate(&q, &rows, &limits).unwrap().rows,
            vec![annotated(rows[0].clone(), "next", Some("v"))]
        );
        rows[0].derived.get_mut("pad").unwrap().push('x');
        assert_eq!(
            evaluate(&narrow(q), &rows, &limits),
            Err(EventQueryError::InvalidEvent)
        );
        let mut rows = [
            event(0, 1, "candidate", Some("a")),
            event(1, 2, "witness", Some("a")),
        ];
        while rows[0].derived.len() < MAX_DERIVED_FIELDS {
            let n = rows[0].derived.len();
            rows[0].derived.insert(format!("f{n}"), String::new());
        }
        assert_eq!(
            evaluate(&next(5, key), &rows, &limits).unwrap().rows,
            vec![annotated(rows[0].clone(), "next", Some(&rows[1].text))]
        );
        rows[0].derived.remove("next");
        rows[0].derived.insert("extra".into(), String::new());
        assert_eq!(
            evaluate(&narrow(next(5, key)), &rows, &limits),
            Err(EventQueryError::InvalidEvent)
        );
        let mut witness = event(0, 2, "witness", Some("a"));
        fill_to(&mut witness, MAX_DERIVED_BYTES);
        let bad =
            json!({"Map":{"input":filter("witness"),"field":"overflow","pointer":"/service"}});
        let duplicate = json!({"Union":{"inputs":[filter("witness"),bad]}});
        assert_eq!(
            evaluate(
                &query(filter("absent"), duplicate, "next", "text", 0, key),
                &[witness],
                &limits
            ),
            Err(EventQueryError::InvalidEvent)
        );
    }
}
#[test]
fn shared_work_limits_prefixes_moments_and_receipts_bind_future_context() {
    let rows = [
        event(0, 1, "candidate", Some("a")),
        event(1, 2, "witness", Some("a")),
        event(2, 3, "candidate", Some("a")),
    ];
    for key in [None, Some("owner")] {
        let exact = EventQueryLimits {
            max_nodes: 5,
            max_depth: 2,
            max_scanned_events: 3,
            max_operations: 15,
            ..EventQueryLimits::default()
        };
        let q = next(5, key);
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
                    max_operations: 14,
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
                max_result_rows: 1,
                ..exact
            },
        )
        .unwrap();
        assert_eq!(prefix.rows, result.rows[..1]);
        assert_eq!(prefix.matched, 2);
        assert!(prefix.truncated);
        let limits = EventQueryLimits::default();
        let outer = EventQuery::UpTo {
            input: Box::new(q.clone()),
            moment_tick: 1,
        };
        let output = evaluate(&outer, &rows, &limits).unwrap();
        assert_eq!(
            output.rows,
            vec![annotated(rows[0].clone(), "next", Some(&rows[1].text))]
        );
        let bounded = |input| json!({"UpTo":{"input":input,"moment_tick":1}});
        let scoped = query(
            bounded(filter("candidate")),
            bounded(filter("witness")),
            "next",
            "text",
            5,
            key,
        );
        assert_eq!(
            evaluate(&scoped, &rows, &limits).unwrap().rows,
            vec![annotated(rows[0].clone(), "next", None)]
        );
        let mixed = query(
            bounded(filter("candidate")),
            json!({"UpTo":{"input":filter("witness"),"moment_tick":2}}),
            "next",
            "text",
            5,
            key,
        );
        assert_eq!(
            evaluate(&mixed, &rows, &limits),
            Err(EventQueryError::AmbiguousMoment)
        );
        let receipt = receipt_for(&outer, &rows, &output, &limits).unwrap();
        assert_eq!(receipt.moment_tick, Some(1));
        validate_receipt(&receipt, &outer, &rows, &output, &limits).unwrap();
        for changed in [
            next(4, key),
            query(
                filter("candidate"),
                filter("witness"),
                "other",
                "text",
                5,
                key,
            ),
            query(
                filter("candidate"),
                filter("witness"),
                "next",
                "kind",
                5,
                key,
            ),
            query(
                filter("candidate"),
                filter("absent"),
                "next",
                "text",
                5,
                key,
            ),
            next(5, Some("process")),
        ] {
            let changed = EventQuery::UpTo {
                input: Box::new(changed),
                moment_tick: 1,
            };
            assert!(validate_receipt(&receipt, &changed, &rows, &output, &limits).is_err());
        }
        let mut past = serde_json::to_value(&outer).unwrap();
        let body = past["UpTo"]["input"].as_object_mut().unwrap();
        let op = if key.is_some() {
            "WithNextValueBy"
        } else {
            "WithNextValue"
        };
        let value = body.remove(op).unwrap();
        body.insert(
            if key.is_some() {
                "WithLastValueBy"
            } else {
                "WithLastValue"
            }
            .into(),
            value,
        );
        let past: EventQuery = serde_json::from_value(past).unwrap();
        assert!(validate_receipt(&receipt, &past, &rows, &output, &limits).is_err());
        let mut drift = rows.clone();
        drift[1].text = "future witness drift".into();
        assert!(validate_receipt(&receipt, &outer, &drift, &output, &limits).is_err());
        let mut forged = output.clone();
        forged.rows[0]
            .derived
            .insert("next".into(), "forged".into());
        assert!(validate_receipt(&receipt, &outer, &rows, &forged, &limits).is_err());
    }
}
