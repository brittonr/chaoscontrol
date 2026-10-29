use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn event(seq: u64, tick: u64, kind: &str, key: Option<&str>) -> EventEnvelope {
    let mut derived = BTreeMap::from([("last".to_string(), "stale".to_string())]);
    if let Some(key) = key {
        derived.insert("owner".to_string(), key.to_string());
    }
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: seq,
        tick,
        vm_index: Some(0),
        process: Some("monitor".to_string()),
        thread: None,
        kind: kind.to_string(),
        text: format!("{kind}-{seq}"),
        payload_digest: [2; 32],
        details: Some(json!({"key_a":"a","key_b":"b","value":20})),
        derived,
    }
}
fn filter(kind: &str) -> Value {
    json!({"Filter":{"input":"All","predicate":{"Kind":kind}}})
}
fn encoded(
    input: Value,
    witness: Value,
    field: &str,
    value: &str,
    key: &str,
    within: u64,
) -> Value {
    json!({"WithLastValueBy":{"input":input,"witness":witness,"field":field,"witness_field":value,"key_field":key,"within_ticks":within}})
}
fn query(
    input: Value,
    witness: Value,
    field: &str,
    value: &str,
    key: &str,
    within: u64,
) -> EventQuery {
    serde_json::from_value(encoded(input, witness, field, value, key, within))
        .expect("WithLastValueBy must be supported")
}
fn keyed(within: u64) -> EventQuery {
    query(
        filter("candidate"),
        filter("witness"),
        "last",
        "text",
        "owner",
        within,
    )
}
fn annotated(mut row: EventEnvelope, field: &str, value: Option<&str>) -> EventEnvelope {
    if let Some(value) = value {
        row.derived.insert(field.to_string(), value.to_string());
    } else {
        row.derived.remove(field);
    }
    row
}
fn fill_to(row: &mut EventEnvelope, bytes: usize) {
    row.derived.insert("pad".to_string(), String::new());
    let used: usize = row.derived.iter().map(|(k, v)| k.len() + v.len()).sum();
    row.derived
        .get_mut("pad")
        .unwrap()
        .push_str(&"x".repeat(bytes - used));
}

// r[verify chaoscontrol.keyed_last_witness.annotation]
#[test]
fn interleaved_keys_match_pairwise_oracle_across_permutations_ties_and_tick_boundaries() {
    let mut events = Vec::new();
    for key in [
        None,
        Some(""),
        Some("a"),
        Some("b"),
        Some("é"),
        Some("e\u{301}"),
    ] {
        for tick in [0, 3, 8, u64::MAX] {
            events.push(event(events.len() as u64, tick, "candidate", key));
        }
        for tick in [0, 2, 3, 7, u64::MAX - 1, u64::MAX] {
            events.push(event(events.len() as u64, tick, "witness", key));
        }
    }
    let mut tied = event(0, 2, "witness", Some("a"));
    tied.source_id = [2; 32];
    events.push(tied);
    for _ in 0..2 {
        for within in [0, 1, 4, u64::MAX] {
            let mut expected = Vec::new();
            for row in events.iter().filter(|r| r.kind == "candidate") {
                let key = row.derived.get("owner");
                let witness = events
                    .iter()
                    .filter(|w| {
                        w.kind == "witness"
                            && key.is_some()
                            && w.derived.get("owner") == key
                            && row
                                .tick
                                .checked_sub(w.tick)
                                .is_some_and(|d| d > 0 && d <= within)
                    })
                    .max_by_key(|w| (w.tick, w.source_id, w.source_sequence));
                expected.push(annotated(
                    row.clone(),
                    "last",
                    witness.map(|w| w.text.as_str()),
                ));
            }
            expected.sort_by_key(|r| (r.tick, r.source_id, r.source_sequence));
            assert_eq!(
                evaluate(&keyed(within), &events, &EventQueryLimits::default())
                    .unwrap()
                    .rows,
                expected
            );
        }
        events.reverse();
    }
}
#[test]
fn missing_nearest_values_aliases_and_duplicate_keys_keep_original_priority() {
    let limits = EventQueryLimits::default();
    let mut events = [
        event(0, 3, "candidate", Some("a")),
        event(1, 1, "witness", Some("a")),
        event(2, 2, "witness", Some("a")),
        event(3, 2, "witness", Some("b")),
    ];
    events[1]
        .derived
        .insert("label".to_string(), "old".to_string());
    events[3]
        .derived
        .insert("label".to_string(), "wrong owner".to_string());
    for value in [None, Some("")] {
        if let Some(value) = value {
            events[2]
                .derived
                .insert("label".to_string(), value.to_string());
        }
        let q = query(
            filter("candidate"),
            filter("witness"),
            "last",
            "label",
            "owner",
            5,
        );
        assert_eq!(
            evaluate(&q, &events, &limits).unwrap().rows,
            vec![annotated(events[0].clone(), "last", value)]
        );
    }
    for field in ["owner", "text"] {
        let q = query(
            filter("candidate"),
            filter("witness"),
            field,
            "text",
            "owner",
            5,
        );
        assert_eq!(
            evaluate(&q, &events, &limits).unwrap().rows,
            vec![annotated(events[0].clone(), field, Some("witness-2"))]
        );
    }
    let project = |pointer: &str| json!({"Map":{"input":filter("witness"),"field":"owner","pointer":pointer}});
    let witness = json!({"Union":{"inputs":[project("/key_a"),project("/key_b")]}});
    let events = [
        event(0, 3, "candidate", Some("a")),
        event(1, 3, "candidate", Some("b")),
        event(2, 2, "witness", None),
    ];
    let q = query(filter("candidate"), witness, "last", "text", "owner", 5);
    assert_eq!(
        evaluate(&q, &events, &limits).unwrap().rows,
        vec![
            annotated(events[0].clone(), "last", Some("witness-2")),
            annotated(events[1].clone(), "last", None)
        ]
    );
}
#[test]
fn canonical_keys_and_independent_json_projections_compose() {
    let limits = EventQueryLimits::default();
    for key in ["kind", "text", "process", "source_kind"] {
        let mut events = [
            event(0, 2, "candidate", Some("a")),
            event(1, 1, "witness", Some("b")),
        ];
        for row in &mut events {
            row.derived.insert(key.to_string(), "shadow".to_string());
        }
        let value = if matches!(key, "process" | "source_kind") {
            Some("witness-1")
        } else {
            None
        };
        let q = query(
            filter("candidate"),
            filter("witness"),
            "last",
            "text",
            key,
            5,
        );
        assert_eq!(
            evaluate(&q, &events, &limits).unwrap().rows,
            vec![annotated(events[0].clone(), "last", value)]
        );
        if key == "process" {
            for row in &mut events {
                row.process = None;
            }
            assert_eq!(
                evaluate(&q, &events, &limits).unwrap().rows,
                vec![annotated(events[0].clone(), "last", None)]
            );
        }
    }
    for operator in ["Map", "MapText"] {
        let mut events = [
            event(0, 2, "candidate", Some("wrong-a")),
            event(1, 1, "witness", Some("wrong-b")),
        ];
        for row in &mut events {
            row.text = json!({"key_a":"a","value":20}).to_string();
        }
        let project = |kind: &str| json!({operator:{"input":filter(kind),"field":"owner","pointer":"/key_a"}});
        let witness =
            json!({operator:{"input":project("witness"),"field":"amount","pointer":"/value"}});
        let input = encoded(project("candidate"), witness, "last", "amount", "owner", 5);
        let q:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":input,"predicate":{"FieldNumber":{"field":"last","comparison":"Equal","value":20}}}},"fields":["last"]}})).unwrap();
        let mut expected = events[0].clone();
        expected.derived = BTreeMap::from([("last".to_string(), "20".to_string())]);
        assert_eq!(evaluate(&q, &events, &limits).unwrap().rows, vec![expected]);
    }
}
#[test]
fn all_names_and_both_children_are_admitted_on_empty_and_zero_windows() {
    let limits = EventQueryLimits::default();
    for invalid in [
        String::new(),
        "a\nb".to_string(),
        "é".repeat(MAX_FIELD_BYTES / 2) + "x",
    ] {
        for (field, value, key) in [
            (&invalid[..], "text", "owner"),
            ("last", &invalid[..], "owner"),
            ("last", "text", &invalid[..]),
        ] {
            assert_eq!(
                evaluate(
                    &query(filter("absent"), filter("witness"), field, value, key, 0),
                    &[],
                    &limits
                ),
                Err(EventQueryError::InvalidField)
            );
        }
    }
    let name = "é".repeat(MAX_FIELD_BYTES / 2);
    let mut events = [event(0, 2, "candidate", None), event(1, 1, "witness", None)];
    for row in &mut events {
        row.derived.insert(name.clone(), "a".to_string());
    }
    let q = query(
        filter("candidate"),
        filter("witness"),
        &name,
        "text",
        &name,
        5,
    );
    assert_eq!(
        evaluate(&q, &events, &limits).unwrap().rows,
        vec![annotated(events[0].clone(), &name, Some("witness-1"))]
    );
    let bad = json!({"Map":{"input":"All","field":"x","pointer":"invalid"}});
    for q in [
        query(bad.clone(), json!("All"), "last", "text", "owner", 0),
        query(json!("All"), bad, "last", "text", "owner", 0),
    ] {
        assert_eq!(
            evaluate(&q, &[], &limits),
            Err(EventQueryError::InvalidPointer)
        );
    }
    events[1].text = "invalid JSON".to_string();
    let malformed =
        json!({"MapText":{"input":filter("witness"),"field":"owner","pointer":"/key_a"}});
    assert_eq!(
        evaluate(
            &query(filter("absent"), malformed, "last", "text", "owner", 0),
            &events,
            &limits
        ),
        Err(EventQueryError::InvalidJsonText)
    );
    fill_to(&mut events[1], MAX_DERIVED_BYTES);
    let invalid = json!({"Map":{"input":filter("witness"),"field":"extra","pointer":"/key_a"}});
    assert_eq!(
        evaluate(
            &query(filter("absent"), invalid, "last", "text", "owner", 0),
            &events,
            &limits
        ),
        Err(EventQueryError::InvalidEvent)
    );
}
#[test]
fn generated_capacity_is_checked_before_narrowing_and_witness_deduplication() {
    let limits = EventQueryLimits::default();
    let mut events = [
        event(0, 2, "candidate", Some("a")),
        event(1, 1, "witness", Some("a")),
    ];
    events[0].derived.remove("last");
    events[1]
        .derived
        .insert("label".to_string(), "v".to_string());
    fill_to(&mut events[0], MAX_DERIVED_BYTES - "last".len() - 1);
    let q = query(
        filter("candidate"),
        filter("witness"),
        "last",
        "label",
        "owner",
        5,
    );
    assert_eq!(
        evaluate(&q, &events, &limits).unwrap().rows,
        vec![annotated(events[0].clone(), "last", Some("v"))]
    );
    let narrow = |input| EventQuery::Narrow {
        input: Box::new(input),
        fields: vec![],
    };
    events[0].derived.get_mut("pad").unwrap().push('x');
    assert_eq!(
        evaluate(&narrow(q.clone()), &events, &limits),
        Err(EventQueryError::InvalidEvent)
    );
    events[0] = event(0, 2, "candidate", Some("a"));
    while events[0].derived.len() < MAX_DERIVED_FIELDS {
        events[0]
            .derived
            .insert(format!("f{}", events[0].derived.len()), String::new());
    }
    assert_eq!(
        evaluate(&q, &events, &limits).unwrap().rows,
        vec![annotated(events[0].clone(), "last", Some("v"))]
    );
    events[0].derived.remove("last");
    events[0].derived.insert("extra".to_string(), String::new());
    assert_eq!(
        evaluate(&narrow(q), &events, &limits),
        Err(EventQueryError::InvalidEvent)
    );
    events[1].derived.insert("pad".to_string(), String::new());
    fill_to(&mut events[1], MAX_DERIVED_BYTES);
    let child = json!({"Union":{"inputs":[filter("witness"),{"Map":{"input":filter("witness"),"field":"extra","pointer":"/key_a"}}]}});
    assert_eq!(
        evaluate(
            &query(filter("absent"), child, "last", "text", "owner", 0),
            &events,
            &limits
        ),
        Err(EventQueryError::InvalidEvent)
    );
}
#[test]
fn shared_limits_moments_and_receipts_bind_key_and_both_branches() {
    let events = [
        event(0, 2, "candidate", Some("a")),
        event(1, 1, "witness", Some("a")),
    ];
    let q = keyed(1);
    let exact = EventQueryLimits {
        max_nodes: 5,
        max_depth: 2,
        max_scanned_events: 2,
        max_operations: 10,
        ..EventQueryLimits::default()
    };
    let result = evaluate(&q, &events, &exact).unwrap();
    assert_eq!(
        result.rows,
        vec![annotated(events[0].clone(), "last", Some("witness-1"))]
    );
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
                max_operations: 9,
                ..exact
            },
            EventQueryError::OperationBound,
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
    let input = json!({"UpTo":{"input":filter("candidate"),"moment_tick":2}});
    let witness = json!({"UpTo":{"input":filter("witness"),"moment_tick":2}});
    let q = query(input.clone(), witness.clone(), "last", "text", "owner", 1);
    let limits = EventQueryLimits::default();
    let result = evaluate(&q, &events, &limits).unwrap();
    let receipt = receipt_for(&q, &events, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(2));
    validate_receipt(&receipt, &q, &events, &result, &limits).unwrap();
    for key in ["process", "kind"] {
        assert!(validate_receipt(
            &receipt,
            &query(input.clone(), witness.clone(), "last", "text", key, 1),
            &events,
            &result,
            &limits
        )
        .is_err());
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
        "owner",
        1,
    );
    assert_eq!(
        evaluate(&conflict, &events, &limits),
        Err(EventQueryError::AmbiguousMoment)
    );
}
