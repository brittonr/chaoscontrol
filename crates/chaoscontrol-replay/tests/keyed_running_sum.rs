use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn event(seq: u64, tick: u64, key: Option<&str>, amount: &str, kind: &str) -> EventEnvelope {
    let mut derived = BTreeMap::from([
        ("total".to_string(), "stale".to_string()),
        ("amount".to_string(), amount.to_string()),
    ]);
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
        text: json!({"service":"a","other":"b","amount":2.5}).to_string(),
        payload_digest: [9; 32],
        details: Some(json!({"service":"a","other":"b","amount":2.5})),
        derived,
    }
}
fn encoded(input: Value, predicate: Value, field: &str, value: &str, key: &str) -> Value {
    json!({"RunningSumBy":{"input":input,"predicate":predicate,"field":field,"value_field":value,"key_field":key}})
}
fn query(input: Value, predicate: Value, field: &str, value: &str, key: &str) -> EventQuery {
    serde_json::from_value(encoded(input, predicate, field, value, key))
        .expect("RunningSumBy must be supported")
}
fn sum() -> EventQuery {
    query(
        json!("All"),
        json!({"Kind":"metric"}),
        "total",
        "amount",
        "owner",
    )
}
fn annotate(mut row: EventEnvelope, field: &str, value: Option<&str>) -> EventEnvelope {
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
// r[verify chaoscontrol.keyed_running_sum.prefixes]
#[test]
fn interleaved_totals_match_independent_prefix_oracle_across_permutations_and_boundaries() {
    let mut events = Vec::new();
    for key in [
        None,
        Some(""),
        Some("a"),
        Some("b"),
        Some("é"),
        Some("e\u{301}"),
    ] {
        for (tick, amount, kind) in [
            (0, "bad", "skip"),
            (1, "2.5", "metric"),
            (1, "-1", "metric"),
            (u64::MAX, "4", "metric"),
        ] {
            events.push(event(events.len() as u64, tick, key, amount, kind));
        }
    }
    let mut tie = event(0, 1, Some("a"), "0.5", "metric");
    tie.source_id = [2; 32];
    events.push(tie);
    let order = |r: &EventEnvelope| (r.tick, r.source_id, r.source_sequence);
    let mut ordered = events.clone();
    ordered.sort_by_key(order);
    let mut expected = Vec::new();
    for (i, row) in ordered.iter().enumerate() {
        let total = row.derived.get("owner").map(|key| {
            ordered[..=i]
                .iter()
                .filter(|p| p.derived.get("owner") == Some(key) && p.kind == "metric")
                .fold(0.0, |acc, p| {
                    acc + p.derived["amount"].parse::<f64>().unwrap()
                })
        });
        let text = total.map(|n| serde_json::Number::from_f64(n).unwrap().to_string());
        expected.push(annotate(row.clone(), "total", text.as_deref()));
    }
    for _ in 0..2 {
        for rotation in [0, 1, 7] {
            events.rotate_left(rotation);
            assert_eq!(
                evaluate(&sum(), &events, &EventQueryLimits::default())
                    .unwrap()
                    .rows,
                expected
            );
        }
        events.reverse();
    }
}
#[test]
fn original_keys_operands_and_predicates_survive_aliases_and_duplicate_priority() {
    let limits = EventQueryLimits::default();
    let events = [
        event(0, 0, Some("2"), "2", "metric"),
        event(1, 1, Some("3"), "3", "metric"),
        event(2, 2, Some("2"), "4", "metric"),
    ];
    for field in ["owner", "amount", "kind"] {
        let q = query(
            json!("All"),
            json!({"Kind":"metric"}),
            field,
            "amount",
            "owner",
        );
        assert_eq!(
            evaluate(&q, &events, &limits).unwrap().rows,
            events
                .iter()
                .zip(["2.0", "3.0", "6.0"])
                .map(|(r, n)| annotate(r.clone(), field, Some(n)))
                .collect::<Vec<_>>()
        );
    }
    let q = query(
        json!("All"),
        json!({"Matches":{"field":"owner","text":"2"}}),
        "owner",
        "owner",
        "owner",
    );
    assert_eq!(
        evaluate(&q, &events, &limits).unwrap().rows,
        events
            .iter()
            .zip(["2.0", "0.0", "4.0"])
            .map(|(r, n)| annotate(r.clone(), "owner", Some(n)))
            .collect::<Vec<_>>()
    );
    let project = |pointer: &str| json!({"Map":{"input":"All","field":"owner","pointer":pointer}});
    let input = json!({"Union":{"inputs":[project("/service"),project("/other")]}});
    let q = query(input, json!({"Kind":"metric"}), "total", "amount", "owner");
    let expected: Vec<_> = events
        .iter()
        .zip(["2.0", "5.0", "9.0"])
        .map(|(r, n)| {
            let mut r = annotate(r.clone(), "total", Some(n));
            r.derived.insert("owner".to_string(), "a".to_string());
            r
        })
        .collect();
    assert_eq!(evaluate(&q, &events, &limits).unwrap().rows, expected);
}
#[test]
fn finite_number_rules_and_missing_key_errors_do_not_cross_groups() {
    let limits = EventQueryLimits::default();
    for (values, totals) in [
        (vec!["0.1", "0.2"], vec!["0.1", "0.30000000000000004"]),
        (
            vec!["9007199254740992", "1", "-9007199254740992"],
            vec!["9007199254740992.0", "9007199254740992.0", "0.0"],
        ),
        (vec!["-0", "-1e-400"], vec!["0.0", "0.0"]),
    ] {
        let rows: Vec<_> = values
            .iter()
            .enumerate()
            .map(|(i, v)| event(i as u64, i as u64, Some("a"), v, "metric"))
            .collect();
        assert_eq!(
            evaluate(&sum(), &rows, &limits).unwrap().rows,
            rows.iter()
                .zip(totals)
                .map(|(r, n)| annotate(r.clone(), "total", Some(n)))
                .collect::<Vec<_>>()
        );
    }
    for invalid in [
        "",
        "bad",
        "01",
        "+1",
        "1 trailing",
        "null",
        "true",
        "\"2\"",
        "1e309",
    ] {
        for key in [None, Some("a")] {
            let source = event(0, 0, key, invalid, "metric");
            assert_eq!(
                evaluate(&sum(), &[source], &limits),
                Err(EventQueryError::InvalidNumber)
            );
        }
    }
    let rows = [
        event(0, 0, Some("a"), "1e308", "metric"),
        event(1, 1, Some("b"), "1e308", "metric"),
    ];
    let result = evaluate(&sum(), &rows, &limits).unwrap();
    for row in result.rows {
        assert_eq!(row.derived["total"].parse::<f64>().unwrap(), 1e308);
    }
    let mut same = rows;
    same[1].derived.insert("owner".to_string(), "a".to_string());
    let mut overflow = same.to_vec();
    overflow.push(event(2, 2, Some("a"), "-1e308", "metric"));
    assert_eq!(
        evaluate(&sum(), &overflow, &limits),
        Err(EventQueryError::InvalidNumber)
    );
    let source = event(0, 0, Some("a"), "bad", "skip");
    assert_eq!(
        evaluate(&sum(), std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![annotate(source, "total", Some("0.0"))]
    );
    let source = event(0, 0, None, "2", "metric");
    assert_eq!(
        evaluate(&sum(), std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![annotate(source, "total", None)]
    );
    let mut source = event(0, 0, None, "2", "metric");
    source.text = "invalid JSON".to_string();
    let p = json!({"JsonTextPresent":{"pointer":"/x"}});
    assert_eq!(
        evaluate(
            &query(json!("All"), p.clone(), "total", "amount", "owner"),
            std::slice::from_ref(&source),
            &limits
        ),
        Err(EventQueryError::InvalidJsonText)
    );
    assert_eq!(
        evaluate(
            &query(
                json!("All"),
                json!({"Any":[{"All":[]},p]}),
                "total",
                "amount",
                "owner"
            ),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(source, "total", None)]
    );
}
#[test]
fn canonical_selectors_json_projection_and_downstream_filters_compose() {
    let limits = EventQueryLimits::default();
    let mut rows = [
        event(0, 0, Some("a"), "2", "metric"),
        event(1, 1, Some("b"), "3", "skip"),
        event(2, 2, Some("a"), "4", "metric"),
    ];
    for key in ["kind", "text", "process", "source_kind"] {
        for row in &mut rows {
            row.derived.insert(key.to_string(), "shadow".to_string());
        }
        let totals = if key == "kind" {
            ["2.0", "3.0", "6.0"]
        } else {
            ["2.0", "5.0", "9.0"]
        };
        assert_eq!(
            evaluate(
                &query(json!("All"), json!({"All":[]}), "total", "amount", key),
                &rows,
                &limits
            )
            .unwrap()
            .rows,
            rows.iter()
                .zip(totals)
                .map(|(r, n)| annotate(r.clone(), "total", Some(n)))
                .collect::<Vec<_>>()
        );
    }
    rows[1].process = None;
    assert_eq!(
        evaluate(
            &query(
                json!("All"),
                json!({"All":[]}),
                "total",
                "amount",
                "process"
            ),
            &rows,
            &limits
        )
        .unwrap()
        .rows,
        vec![
            annotate(rows[0].clone(), "total", Some("2.0")),
            annotate(rows[1].clone(), "total", None),
            annotate(rows[2].clone(), "total", Some("6.0"))
        ]
    );
    for operand in ["kind", "text", "source_kind", "process"] {
        assert_eq!(
            evaluate(
                &query(json!("All"), json!({"All":[]}), "total", operand, "owner"),
                &rows,
                &limits
            ),
            Err(EventQueryError::InvalidNumber)
        );
    }
    for operator in ["Map", "MapText"] {
        let input = json!({operator:{"input":{operator:{"input":"All","field":"owner","pointer":"/service"}},"field":"amount","pointer":"/amount"}});
        let totals = encoded(input, json!({"All":[]}), "total", "amount", "owner");
        let q:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":totals,"predicate":{"FieldNumber":{"field":"total","comparison":"GreaterThan","value":5}}}},"fields":["total"]}})).unwrap();
        let mut expected = rows[2].clone();
        expected.derived = BTreeMap::from([("total".to_string(), "7.5".to_string())]);
        assert_eq!(evaluate(&q, &rows, &limits).unwrap().rows, vec![expected]);
    }
}
#[test]
fn name_child_predicate_and_capacity_admission_cannot_be_hidden() {
    let limits = EventQueryLimits::default();
    for invalid in [
        String::new(),
        "a\nb".to_string(),
        "é".repeat(MAX_FIELD_BYTES / 2) + "x",
    ] {
        for (field, value, key) in [
            (&invalid[..], "amount", "owner"),
            ("total", &invalid[..], "owner"),
            ("total", "amount", &invalid[..]),
        ] {
            assert_eq!(
                evaluate(
                    &query(json!("All"), json!({"All":[]}), field, value, key),
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
            &query(bad, json!({"All":[]}), "total", "amount", "owner"),
            &[],
            &limits
        ),
        Err(EventQueryError::InvalidPointer)
    );
    assert_eq!(
        evaluate(
            &query(
                json!("All"),
                json!({"Regex":{"field":"kind","pattern":"["}}),
                "total",
                "amount",
                "owner"
            ),
            &[],
            &limits
        ),
        Err(EventQueryError::InvalidRegex)
    );
    let name = "é".repeat(MAX_FIELD_BYTES / 2);
    let mut source = event(0, 0, None, "1", "metric");
    source.derived.insert(name.clone(), "2".to_string());
    assert_eq!(
        evaluate(
            &query(json!("All"), json!({"All":[]}), &name, &name, &name),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(source, &name, Some("2.0"))]
    );
    let narrow = |input| EventQuery::Narrow {
        input: Box::new(input),
        fields: vec![],
    };
    let mut source = event(0, 0, Some("a"), "1", "metric");
    source.derived.remove("total");
    fill_to(&mut source, MAX_DERIVED_BYTES - "total".len() - 3);
    assert_eq!(
        evaluate(&sum(), std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![annotate(source.clone(), "total", Some("1.0"))]
    );
    source.derived.get_mut("pad").unwrap().push('x');
    assert_eq!(
        evaluate(&narrow(sum()), &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, 0, Some("a"), "1", "metric");
    while source.derived.len() < MAX_DERIVED_FIELDS {
        source
            .derived
            .insert(format!("f{}", source.derived.len()), String::new());
    }
    assert_eq!(
        evaluate(&sum(), std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![annotate(source.clone(), "total", Some("1.0"))]
    );
    source.derived.remove("total");
    source.derived.insert("extra".to_string(), String::new());
    assert_eq!(
        evaluate(&narrow(sum()), &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, 0, None, "1", "metric");
    fill_to(&mut source, MAX_DERIVED_BYTES);
    let input = json!({"Map":{"input":"All","field":"overflow","pointer":"/service"}});
    assert_eq!(
        evaluate(
            &narrow(query(input, json!({"All":[]}), "total", "amount", "owner")),
            &[source],
            &limits
        ),
        Err(EventQueryError::InvalidEvent)
    );
}
#[test]
fn shared_limits_prefixes_moments_and_receipts_bind_all_sum_inputs() {
    let rows = [
        event(0, 0, Some("a"), "2", "metric"),
        event(1, 1, Some("b"), "3", "metric"),
        event(2, 2, Some("a"), "4", "metric"),
    ];
    let exact = EventQueryLimits {
        max_nodes: 2,
        max_depth: 1,
        max_scanned_events: 3,
        max_operations: 6,
        ..EventQueryLimits::default()
    };
    let q = sum();
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
    let nested = EventQuery::UpTo {
        input: Box::new(sum()),
        moment_tick: 2,
    };
    assert_eq!(
        evaluate(&nested, &rows, &exact),
        Err(EventQueryError::QueryDepth)
    );
    let limits = EventQueryLimits::default();
    let input = json!({"UpTo":{"input":"All","moment_tick":1}});
    let q = query(
        input.clone(),
        json!({"Kind":"metric"}),
        "total",
        "amount",
        "owner",
    );
    let result = evaluate(&q, &rows, &limits).unwrap();
    let receipt = receipt_for(&q, &rows, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(1));
    validate_receipt(&receipt, &q, &rows, &result, &limits).unwrap();
    for changed in [
        query(
            input.clone(),
            json!({"Kind":"metric"}),
            "total",
            "amount",
            "process",
        ),
        query(input.clone(), json!({"All":[]}), "total", "amount", "owner"),
        query(
            input.clone(),
            json!({"Kind":"metric"}),
            "total",
            "owner",
            "owner",
        ),
        query(input, json!({"Kind":"metric"}), "other", "amount", "owner"),
    ] {
        assert!(validate_receipt(&receipt, &changed, &rows, &result, &limits).is_err());
    }
    let mut drift = rows.clone();
    drift[2].text = "unselected drift".to_string();
    assert!(validate_receipt(&receipt, &q, &drift, &result, &limits).is_err());
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("total".to_string(), "99".to_string());
    assert!(validate_receipt(&receipt, &q, &rows, &forged, &limits).is_err());
    let conflict: EventQuery = serde_json::from_value(
        json!({"Union":{"inputs":[q,{"UpTo":{"input":"All","moment_tick":2}}]}}),
    )
    .unwrap();
    assert_eq!(
        evaluate(&conflict, &rows, &limits),
        Err(EventQueryError::AmbiguousMoment)
    );
}
