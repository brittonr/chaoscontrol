use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn event(sequence: u64, tick: u64, since: &str) -> EventEnvelope {
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick,
        vm_index: Some(0),
        process: None,
        thread: None,
        kind: "metric".to_string(),
        text: json!({"since":"2","tick":999}).to_string(),
        payload_digest: [9; 32],
        details: Some(json!({"since":2,"tick":999})),
        derived: BTreeMap::from([
            ("since".to_string(), since.to_string()),
            ("elapsed".to_string(), "obsolete".to_string()),
            ("tick".to_string(), "0".to_string()),
        ]),
    }
}
fn query(input: Value, field: &str, since_field: &str) -> EventQuery {
    serde_json::from_value(
        json!({"MapElapsedTicks":{"input":input,"field":field,"since_field":since_field}}),
    )
    .expect("MapElapsedTicks must be supported")
}
fn project(input: Value) -> EventQuery {
    query(input, "elapsed", "since")
}
fn annotate(mut row: EventEnvelope, field: &str, value: Option<&str>) -> EventEnvelope {
    if let Some(value) = value {
        row.derived.insert(field.to_string(), value.to_string());
    } else {
        row.derived.remove(field);
    }
    row
}

// r[verify chaoscontrol.map_elapsed_ticks.projection]
#[test]
fn integer_duration_boundaries_and_undefined_since_values_preserve_evidence() {
    let cases = [
        (0, "0", Some("0")),
        (8, " 2 ", Some("6")),
        (8, "8", Some("0")),
        (8, "9", None),
        (9007199254740993, "9007199254740992", Some("1")),
        (u64::MAX, "18446744073709551614", Some("1")),
        (u64::MAX, "0", Some("18446744073709551615")),
        (u64::MAX, "18446744073709551615", Some("0")),
        (8, "18446744073709551616", None),
        (8, "-0", None),
        (8, "-1", None),
        (8, "1.0", None),
        (8, "1e0", None),
        (8, "01", None),
        (8, "+1", None),
        (8, "\"1\"", None),
        (8, "null", None),
        (8, "true", None),
        (8, "[]", None),
        (8, "{}", None),
        (8, "", None),
        (8, "1 trailing", None),
    ];
    let mut rows = Vec::new();
    let mut expected = Vec::new();
    for (i, (tick, since, value)) in cases.into_iter().enumerate() {
        let row = event(i as u64, tick, since);
        expected.push(annotate(row.clone(), "elapsed", value));
        rows.push(row);
    }
    let mut missing = event(rows.len() as u64, 8, "2");
    missing.derived.remove("since");
    expected.push(annotate(missing.clone(), "elapsed", None));
    rows.push(missing);
    expected.sort_by_key(|row| (row.tick, row.source_sequence));
    rows.reverse();
    assert_eq!(
        evaluate(&project(json!("All")), &rows, &EventQueryLimits::default())
            .unwrap()
            .rows,
        expected
    );
}
#[test]
fn aliases_and_canonical_precedence_read_the_original_since_value() {
    let limits = EventQueryLimits::default();
    let source = event(0, 8, "2");
    for field in ["since", "kind", "tick"] {
        assert_eq!(
            evaluate(
                &query(json!("All"), field, "since"),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![annotate(source.clone(), field, Some("6"))]
        );
    }
    let source = event(0, 8, "bad");
    assert_eq!(
        evaluate(
            &query(json!("All"), "since", "since"),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(source, "since", None)]
    );
    for canonical in ["kind", "text", "process", "source_kind"] {
        let mut source = event(0, 8, "2");
        source
            .derived
            .insert(canonical.to_string(), "1".to_string());
        assert_eq!(
            evaluate(
                &query(json!("All"), "elapsed", canonical),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![annotate(source, "elapsed", None)]
        );
    }
    let mut source = event(0, 8, "2");
    source.process = Some("2".to_string());
    source
        .derived
        .insert("process".to_string(), "7".to_string());
    assert_eq!(
        evaluate(
            &query(json!("All"), "elapsed", "process"),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(source, "elapsed", Some("6"))]
    );
}
#[test]
fn json_and_past_witness_ticks_compose_into_exact_wide_durations() {
    let limits = EventQueryLimits::default();
    for operator in ["Map", "MapText"] {
        let source = event(0, 8, "bad");
        let input = json!({operator:{"input":"All","field":"since","pointer":"/since"}});
        let mut expected = annotate(source.clone(), "elapsed", Some("6"));
        expected
            .derived
            .insert("since".to_string(), "2".to_string());
        assert_eq!(
            evaluate(&project(input), std::slice::from_ref(&source), &limits)
                .unwrap()
                .rows,
            vec![expected]
        );
    }
    let mut rows = [
        event(0, u64::MAX - 2, "obsolete"),
        event(1, u64::MAX - 1, "obsolete"),
        event(2, u64::MAX, "obsolete"),
    ];
    rows[0].kind = "error".to_string();
    rows[1].kind = "restart".to_string();
    rows[2].kind = "error".to_string();
    let input = json!({"WithLastValue":{"input":{"Filter":{"input":"All","predicate":{"Kind":"error"}}},"witness":{"MapTick":{"input":{"Filter":{"input":"All","predicate":{"Kind":"restart"}}},"field":"witness_tick"}},"field":"since","witness_field":"witness_tick","within_ticks":2}});
    let q = project(input);
    let mut first = annotate(rows[0].clone(), "elapsed", None);
    first.derived.remove("since");
    let mut last = annotate(rows[2].clone(), "elapsed", Some("1"));
    last.derived
        .insert("since".to_string(), (u64::MAX - 1).to_string());
    assert_eq!(
        evaluate(&q, &rows, &limits).unwrap().rows,
        vec![first, last]
    );
    let selected:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":q,"predicate":{"FieldNumber":{"field":"elapsed","comparison":"Equal","value":1}}}},"fields":["elapsed"]}})).unwrap();
    let mut expected = rows[2].clone();
    expected.derived = BTreeMap::from([("elapsed".to_string(), "1".to_string())]);
    assert_eq!(
        evaluate(&selected, &rows, &limits).unwrap().rows,
        vec![expected]
    );
}
#[test]
fn both_names_and_children_are_admitted_on_empty_sources() {
    let limits = EventQueryLimits::default();
    for invalid in [
        String::new(),
        "a\nb".to_string(),
        "é".repeat(MAX_FIELD_BYTES / 2) + "x",
    ] {
        for (field, since) in [(&invalid[..], "since"), ("elapsed", &invalid[..])] {
            assert_eq!(
                evaluate(&query(json!("All"), field, since), &[], &limits),
                Err(EventQueryError::InvalidField)
            );
        }
    }
    let exact = "é".repeat(MAX_FIELD_BYTES / 2);
    let mut source = event(0, 8, "2");
    source.derived.insert(exact.clone(), "2".to_string());
    assert_eq!(
        evaluate(
            &query(json!("All"), &exact, &exact),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(source, &exact, Some("6"))]
    );
    assert_eq!(
        evaluate(
            &query(
                json!({"Map":{"input":"All","field":"x","pointer":"bad"}}),
                "elapsed",
                "since"
            ),
            &[],
            &limits
        ),
        Err(EventQueryError::InvalidPointer)
    );
    assert_eq!(
        evaluate(&project(json!("All")), &[], &limits).unwrap().rows,
        Vec::<EventEnvelope>::new()
    );
}
#[test]
fn capacity_checks_cannot_be_hidden_by_narrowing_duplicates_or_repair() {
    let limits = EventQueryLimits::default();
    let q = query(json!("All"), "new", "since");
    let narrow = |input| EventQuery::Narrow {
        input: Box::new(input),
        fields: vec!["new".to_string()],
    };
    let mut source = event(0, u64::MAX, "0");
    source.derived.insert("pad".to_string(), String::new());
    let used: usize = source.derived.iter().map(|(k, v)| k.len() + v.len()).sum();
    source
        .derived
        .get_mut("pad")
        .unwrap()
        .push_str(&"x".repeat(MAX_DERIVED_BYTES - "new".len() - u64::MAX.to_string().len() - used));
    assert_eq!(
        evaluate(&q, std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![annotate(
            source.clone(),
            "new",
            Some("18446744073709551615")
        )]
    );
    source.derived.get_mut("pad").unwrap().push('x');
    assert_eq!(
        evaluate(
            &narrow(q.clone()),
            &[event(0, u64::MAX, "0"), source],
            &limits
        ),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, u64::MAX, "0");
    while source.derived.len() < MAX_DERIVED_FIELDS {
        source
            .derived
            .insert(format!("f{}", source.derived.len()), String::new());
    }
    assert_eq!(
        evaluate(
            &project(json!("All")),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(
            source.clone(),
            "elapsed",
            Some("18446744073709551615")
        )]
    );
    assert_eq!(
        evaluate(&narrow(q), &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, 8, "2");
    source.derived.get_mut("elapsed").unwrap().clear();
    let used: usize = source.derived.iter().map(|(k, v)| k.len() + v.len()).sum();
    source
        .derived
        .get_mut("elapsed")
        .unwrap()
        .push_str(&"x".repeat(MAX_DERIVED_BYTES - used));
    assert_eq!(
        evaluate(
            &project(json!("All")),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![annotate(source.clone(), "elapsed", Some("6"))]
    );
    let input = json!({"Map":{"input":"All","field":"overflow","pointer":"/since"}});
    assert_eq!(
        evaluate(&narrow(project(input)), &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
}
#[test]
fn shared_limits_moments_and_receipts_bind_duration_facts() {
    let rows = [event(0, 8, "2"), event(1, 9, "2")];
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
        moment_tick: 9,
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
    let input = json!({"UpTo":{"input":"All","moment_tick":8}});
    let q = project(input.clone());
    let result = evaluate(&q, &rows, &limits).unwrap();
    let receipt = receipt_for(&q, &rows, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(8));
    validate_receipt(&receipt, &q, &rows, &result, &limits).unwrap();
    for changed in [
        query(input.clone(), "other", "since"),
        query(input, "elapsed", "elapsed"),
        project(json!("All")),
    ] {
        assert!(validate_receipt(&receipt, &changed, &rows, &result, &limits).is_err());
    }
    let mut drift = rows.clone();
    drift[1].text = "unselected canonical drift".to_string();
    assert!(validate_receipt(&receipt, &q, &drift, &result, &limits).is_err());
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("elapsed".to_string(), "99".to_string());
    assert!(validate_receipt(&receipt, &q, &rows, &forged, &limits).is_err());
    let mixed: EventQuery = serde_json::from_value(
        json!({"Union":{"inputs":[q,{"UpTo":{"input":"All","moment_tick":9}}]}}),
    )
    .unwrap();
    assert_eq!(
        evaluate(&mixed, &rows, &limits),
        Err(EventQueryError::AmbiguousMoment)
    );
}
