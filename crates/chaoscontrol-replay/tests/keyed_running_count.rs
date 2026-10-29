use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn event(seq: u64, tick: u64, key: Option<&str>, matching: bool) -> EventEnvelope {
    let mut derived = BTreeMap::from([("seen".to_string(), "stale".to_string())]);
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
        kind: if matching { "error" } else { "info" }.to_string(),
        text: json!({"service":"a","alternate":"b"}).to_string(),
        payload_digest: [9; 32],
        details: Some(json!({"service":"a","alternate":"b"})),
        derived,
    }
}
fn encoded(input: Value, predicate: Value, field: &str, key: &str) -> Value {
    json!({"RunningCountBy":{"input":input,"predicate":predicate,"field":field,"key_field":key}})
}
fn query(input: Value, predicate: Value, field: &str, key: &str) -> EventQuery {
    serde_json::from_value(encoded(input, predicate, field, key))
        .expect("RunningCountBy must be supported")
}
fn count() -> EventQuery {
    query(json!("All"), json!({"Kind":"error"}), "seen", "owner")
}
fn annotate(mut row: EventEnvelope, field: &str, value: Option<u64>) -> EventEnvelope {
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
// r[verify chaoscontrol.keyed_running_count.prefixes]
#[test]
fn per_key_prefixes_match_independent_pairwise_oracle_with_ties_and_permutations() {
    let mut events = Vec::new();
    for key in [
        None,
        Some(""),
        Some("a"),
        Some("b"),
        Some("é"),
        Some("e\u{301}"),
    ] {
        for (tick, matching) in [(0, false), (1, true), (1, false), (u64::MAX, true)] {
            events.push(event(events.len() as u64, tick, key, matching));
        }
    }
    let mut tied = event(0, 1, Some("a"), true);
    tied.source_id = [2; 32];
    events.push(tied);
    let order = |r: &EventEnvelope| (r.tick, r.source_id, r.source_sequence);
    let mut expected = events.clone();
    expected.sort_by_key(order);
    for row in &mut expected {
        let key = row.derived.get("owner");
        let total = key.map(|k| {
            events
                .iter()
                .filter(|p| {
                    p.derived.get("owner") == Some(k) && order(p) <= order(row) && p.kind == "error"
                })
                .count() as u64
        });
        *row = annotate(row.clone(), "seen", total);
    }
    for _ in 0..2 {
        for rotation in [0, 1, 7] {
            events.rotate_left(rotation);
            assert_eq!(
                evaluate(&count(), &events, &EventQueryLimits::default())
                    .unwrap()
                    .rows,
                expected
            );
        }
        events.reverse();
    }
}
#[test]
fn original_keys_predicates_and_duplicate_projection_priority_survive_aliases() {
    let limits = EventQueryLimits::default();
    let events = [
        event(0, 0, Some("a"), false),
        event(1, 1, Some("b"), true),
        event(2, 2, Some("a"), true),
        event(3, 3, Some("a"), true),
    ];
    for field in ["owner", "kind", "seen"] {
        let q = query(json!("All"), json!({"Kind":"error"}), field, "owner");
        let expected: Vec<_> = events
            .iter()
            .zip([0, 1, 1, 2])
            .map(|(r, n)| annotate(r.clone(), field, Some(n)))
            .collect();
        assert_eq!(evaluate(&q, &events, &limits).unwrap().rows, expected);
    }
    let q = query(
        json!("All"),
        json!({"Matches":{"field":"owner","text":"a"}}),
        "owner",
        "owner",
    );
    let expected: Vec<_> = events
        .iter()
        .zip([1, 0, 2, 3])
        .map(|(r, n)| annotate(r.clone(), "owner", Some(n)))
        .collect();
    assert_eq!(evaluate(&q, &events, &limits).unwrap().rows, expected);
    let project = |pointer: &str| json!({"Map":{"input":"All","field":"owner","pointer":pointer}});
    let input = json!({"Union":{"inputs":[project("/service"),project("/alternate")]}});
    let q = query(input, json!({"All":[]}), "seen", "owner");
    let expected: Vec<_> = events
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut r = annotate(r.clone(), "seen", Some(i as u64 + 1));
            r.derived.insert("owner".to_string(), "a".to_string());
            r
        })
        .collect();
    assert_eq!(evaluate(&q, &events, &limits).unwrap().rows, expected);
}
#[test]
fn canonical_keys_json_projection_and_downstream_counting_compose() {
    let limits = EventQueryLimits::default();
    let mut events = [
        event(0, 0, Some("a"), false),
        event(1, 1, Some("b"), true),
        event(2, 2, Some("a"), true),
    ];
    for key in ["kind", "text", "process", "source_kind"] {
        for row in &mut events {
            row.derived.insert(key.to_string(), "shadow".to_string());
        }
        let totals = if key == "kind" { [1, 1, 2] } else { [1, 2, 3] };
        let q = query(json!("All"), json!({"All":[]}), "seen", key);
        assert_eq!(
            evaluate(&q, &events, &limits).unwrap().rows,
            events
                .iter()
                .zip(totals)
                .map(|(r, n)| annotate(r.clone(), "seen", Some(n)))
                .collect::<Vec<_>>()
        );
    }
    events[1].process = None;
    let q = query(json!("All"), json!({"All":[]}), "seen", "process");
    assert_eq!(
        evaluate(&q, &events, &limits).unwrap().rows,
        vec![
            annotate(events[0].clone(), "seen", Some(1)),
            annotate(events[1].clone(), "seen", None),
            annotate(events[2].clone(), "seen", Some(2))
        ]
    );
    for operator in ["Map", "MapText"] {
        let input = json!({operator:{"input":"All","field":"owner","pointer":"/service"}});
        let counts = encoded(input, json!({"Kind":"error"}), "seen", "owner");
        let q:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":{"RunningCount":{"input":counts,"predicate":{"Kind":"error"},"field":"global"}},"predicate":{"FieldNumber":{"field":"seen","comparison":"GreaterThanOrEqual","value":2}}}},"fields":["seen","global"]}})).unwrap();
        let mut expected = events[2].clone();
        expected.derived = BTreeMap::from([
            ("seen".to_string(), "2".to_string()),
            ("global".to_string(), "2".to_string()),
        ]);
        assert_eq!(evaluate(&q, &events, &limits).unwrap().rows, vec![expected]);
    }
}
#[test]
fn missing_keys_do_not_hide_predicate_errors_and_admission_runs_on_empty_inputs() {
    let limits = EventQueryLimits::default();
    let mut source = event(0, 0, None, true);
    source.text = "bad JSON".to_string();
    let predicate = json!({"JsonTextNumber":{"pointer":"/n","comparison":"Equal","value":1}});
    assert_eq!(
        evaluate(
            &query(json!("All"), predicate.clone(), "seen", "owner"),
            std::slice::from_ref(&source),
            &limits
        ),
        Err(EventQueryError::InvalidJsonText)
    );
    let skipped = json!({"Any":[{"All":[]},predicate]});
    assert_eq!(
        evaluate(
            &query(json!("All"), skipped, "seen", "owner"),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(source, "seen", None)]
    );
    for invalid in [
        String::new(),
        "a\nb".to_string(),
        "é".repeat(MAX_FIELD_BYTES / 2) + "x",
    ] {
        for (field, key) in [(&invalid[..], "owner"), ("seen", &invalid[..])] {
            assert_eq!(
                evaluate(
                    &query(json!("All"), json!({"All":[]}), field, key),
                    &[],
                    &limits
                ),
                Err(EventQueryError::InvalidField)
            );
        }
    }
    let bad = json!({"Map":{"input":"All","field":"x","pointer":"bad"}});
    assert_eq!(
        evaluate(
            &query(bad, json!({"All":[]}), "seen", "owner"),
            &[],
            &limits
        ),
        Err(EventQueryError::InvalidPointer)
    );
    let bad = json!({"JsonNumber":{"pointer":"bad","comparison":"Equal","value":1}});
    assert_eq!(
        evaluate(&query(json!("All"), bad, "seen", "owner"), &[], &limits),
        Err(EventQueryError::InvalidPointer)
    );
    let name = "é".repeat(MAX_FIELD_BYTES / 2);
    let mut source = event(0, 0, None, true);
    source.derived.insert(name.clone(), "key".to_string());
    assert_eq!(
        evaluate(
            &query(json!("All"), json!({"All":[]}), &name, &name),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(source, &name, Some(1))]
    );
}
#[test]
fn exact_and_generated_capacity_cannot_be_hidden_by_downstream_narrowing() {
    let limits = EventQueryLimits::default();
    let mut source = event(0, 0, Some("a"), true);
    source.derived.remove("seen");
    fill_to(&mut source, MAX_DERIVED_BYTES - "seen".len() - 1);
    assert_eq!(
        evaluate(&count(), std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![annotate(source.clone(), "seen", Some(1))]
    );
    let narrow = |input| EventQuery::Narrow {
        input: Box::new(input),
        fields: vec![],
    };
    source.derived.get_mut("pad").unwrap().push('x');
    assert_eq!(
        evaluate(&narrow(count()), &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, 0, Some("a"), true);
    while source.derived.len() < MAX_DERIVED_FIELDS {
        source
            .derived
            .insert(format!("f{}", source.derived.len()), String::new());
    }
    assert_eq!(
        evaluate(&count(), std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![annotate(source.clone(), "seen", Some(1))]
    );
    source.derived.remove("seen");
    source.derived.insert("extra".to_string(), String::new());
    assert_eq!(
        evaluate(&narrow(count()), &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, 0, None, true);
    fill_to(&mut source, MAX_DERIVED_BYTES);
    let input = json!({"Map":{"input":"All","field":"overflow","pointer":"/service"}});
    assert_eq!(
        evaluate(
            &narrow(query(input, json!({"All":[]}), "seen", "owner")),
            &[source],
            &limits
        ),
        Err(EventQueryError::InvalidEvent)
    );
}
#[test]
fn shared_limits_prefixes_moments_and_receipts_bind_keyed_count_facts() {
    let events = [
        event(0, 0, Some("a"), true),
        event(1, 1, Some("b"), true),
        event(2, 2, Some("a"), true),
    ];
    let exact = EventQueryLimits {
        max_nodes: 2,
        max_depth: 1,
        max_scanned_events: 3,
        max_operations: 6,
        ..EventQueryLimits::default()
    };
    let q = count();
    let result = evaluate(&q, &events, &exact).unwrap();
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
        assert_eq!(evaluate(&q, &events, &limits), Err(error));
    }
    let prefix = evaluate(
        &q,
        &events,
        &EventQueryLimits {
            max_result_rows: 2,
            ..exact
        },
    )
    .unwrap();
    assert_eq!(prefix.rows, result.rows[..2]);
    assert_eq!(prefix.matched, 3);
    assert!(prefix.truncated);
    let limits = EventQueryLimits::default();
    let input = json!({"UpTo":{"input":"All","moment_tick":1}});
    let q = query(input.clone(), json!({"Kind":"error"}), "seen", "owner");
    let result = evaluate(&q, &events, &limits).unwrap();
    let receipt = receipt_for(&q, &events, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(1));
    validate_receipt(&receipt, &q, &events, &result, &limits).unwrap();
    for changed in [
        query(input.clone(), json!({"Kind":"error"}), "seen", "process"),
        query(input.clone(), json!({"All":[]}), "seen", "owner"),
        query(input, json!({"Kind":"error"}), "other", "owner"),
    ] {
        assert!(validate_receipt(&receipt, &changed, &events, &result, &limits).is_err());
    }
    let mut changed = events.clone();
    changed[2].text = "unselected drift".to_string();
    assert!(validate_receipt(&receipt, &q, &changed, &result, &limits).is_err());
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("seen".to_string(), "99".to_string());
    assert!(validate_receipt(&receipt, &q, &events, &forged, &limits).is_err());
    let conflict: EventQuery = serde_json::from_value(
        json!({"Union":{"inputs":[q,{"UpTo":{"input":"All","moment_tick":2}}]}}),
    )
    .unwrap();
    assert_eq!(
        evaluate(&conflict, &events, &limits),
        Err(EventQueryError::AmbiguousMoment)
    );
    let nested = EventQuery::UpTo {
        input: Box::new(count()),
        moment_tick: 2,
    };
    assert_eq!(
        evaluate(&nested, &events, &exact),
        Err(EventQueryError::QueryDepth)
    );
}
