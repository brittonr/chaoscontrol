use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
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
            ("ratio".to_string(), "obsolete".to_string()),
        ]),
    }
}
fn encoded(input: Value, field: &str, numerator: &str, denominator: &str) -> Value {
    json!({"MapRatio":{"input":input,"field":field,"numerator":numerator,"denominator":denominator}})
}
fn query(input: Value, field: &str, numerator: &str, denominator: &str) -> EventQuery {
    serde_json::from_value(encoded(input, field, numerator, denominator))
        .expect("MapRatio must be supported")
}
fn project(input: Value) -> EventQuery {
    query(input, "ratio", "used", "total")
}

// r[verify chaoscontrol.map_ratio.projection]
#[test]
fn finite_ratios_replace_stale_output_and_undefined_ratios_remove_it() {
    let pairs = [
        ("1", "2", Some("0.5")),
        (" -1 ", "2", Some("-0.5")),
        ("0", "2", Some("0.0")),
        ("-0", "2", Some("-0.0")),
        ("1e-308", "1e308", Some("0.0")),
        ("9007199254740993", "1", Some("9007199254740992.0")),
        ("1", "0", None),
        ("1", "-0.0", None),
        ("bad", "2", None),
        ("1", "NaN", None),
        ("1e308", "1e-308", None),
        ("1e400", "2", None),
        ("1", "", None),
        ("\"1\"", "2", None),
        ("1", "true", None),
        ("01", "2", None),
        ("1", "2 trailing", None),
    ];
    let mut events = Vec::new();
    let mut expected = Vec::new();
    for (i, (n, d, value)) in pairs.into_iter().enumerate() {
        let source = event(i as u64, n, d);
        let mut output = source.clone();
        if let Some(value) = value {
            output
                .derived
                .insert("ratio".to_string(), value.to_string());
        } else {
            output.derived.remove("ratio");
        }
        events.push(source);
        expected.push(output);
    }
    for missing in ["used", "total"] {
        let mut source = event(events.len() as u64, "1", "2");
        source.derived.remove(missing);
        let mut output = source.clone();
        output.derived.remove("ratio");
        events.push(source);
        expected.push(output);
    }
    events.reverse();
    assert_eq!(
        evaluate(
            &project(json!("All")),
            &events,
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        expected
    );
}

#[test]
fn output_aliases_read_original_operands_and_canonical_lookup_stays_authoritative() {
    let limits = EventQueryLimits::default();
    let source = event(0, "1", "2");
    for field in ["used", "total"] {
        let mut expected = source.clone();
        expected
            .derived
            .insert(field.to_string(), "0.5".to_string());
        assert_eq!(
            evaluate(
                &query(json!("All"), field, "used", "total"),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![expected]
        );
    }
    let mut source = source;
    source.process = None;
    source
        .derived
        .insert("process".to_string(), "100".to_string());
    for numerator in ["kind", "process", "source_kind", "text"] {
        source
            .derived
            .insert(numerator.to_string(), "100".to_string());
        let mut expected = source.clone();
        expected.derived.remove("ratio");
        assert_eq!(
            evaluate(
                &query(json!("All"), "ratio", numerator, "total"),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![expected]
        );
    }
    let mut expected = source.clone();
    expected
        .derived
        .insert("kind".to_string(), "0.5".to_string());
    assert_eq!(
        evaluate(
            &query(json!("All"), "kind", "used", "total"),
            &[source],
            &limits
        )
        .unwrap()
        .rows,
        vec![expected]
    );
}

#[test]
fn ratio_annotations_compose_with_json_projections_counts_filters_and_narrowing() {
    let limits = EventQueryLimits::default();
    for operator in ["Map", "MapText"] {
        let mut source = event(0, "bad", "bad");
        source.text = json!({"used":"20","total":"100"}).to_string();
        let input = json!({operator:{"input":{operator:{"input":"All","field":"used","pointer":"/used"}},"field":"total","pointer":"/total"}});
        let query:EventQuery=serde_json::from_value(json!({"Narrow":{"input":{"Filter":{"input":project(input),"predicate":{"FieldNumber":{"field":"ratio","comparison":"GreaterThan","value":0.1}}}},"fields":["ratio"]}})).unwrap();
        let mut expected = source.clone();
        expected.derived = BTreeMap::from([("ratio".to_string(), "0.2".to_string())]);
        assert_eq!(
            evaluate(&query, &[source], &limits).unwrap().rows,
            vec![expected]
        );
    }
    let events = [event(0, "1", "2"), event(1, "1", "2")];
    let input = json!({"RunningCount":{"input":"All","predicate":{"All":[]},"field":"seen"}});
    let mut expected = events.to_vec();
    for (i, row) in expected.iter_mut().enumerate() {
        row.derived.insert("seen".to_string(), (i + 1).to_string());
        row.derived.insert(
            "ratio".to_string(),
            if i == 0 { "0.5" } else { "1.0" }.to_string(),
        );
    }
    assert_eq!(
        evaluate(&query(input, "ratio", "seen", "total"), &events, &limits)
            .unwrap()
            .rows,
        expected
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
        for names in [
            (&invalid[..], "used", "total"),
            ("ratio", &invalid[..], "total"),
            ("ratio", "used", &invalid[..]),
        ] {
            assert_eq!(
                evaluate(
                    &query(json!("All"), names.0, names.1, names.2),
                    &[],
                    &limits
                ),
                Err(EventQueryError::InvalidField)
            );
        }
    }
    let exact = "é".repeat(MAX_FIELD_BYTES / 2);
    let mut source = event(0, "1", "2");
    source.derived.insert(exact.clone(), "2".to_string());
    for names in [
        (&exact[..], "used", "total"),
        ("ratio", &exact[..], "total"),
        ("ratio", "used", &exact[..]),
    ] {
        let mut expected = source.clone();
        let value = if names.1 == exact { "1.0" } else { "0.5" };
        expected
            .derived
            .insert(names.0.to_string(), value.to_string());
        assert_eq!(
            evaluate(
                &query(json!("All"), names.0, names.1, names.2),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![expected]
        );
    }
}

fn fill_to(source: &mut EventEnvelope, bytes: usize) {
    source.derived.insert("pad".to_string(), String::new());
    let used: usize = source
        .derived
        .iter()
        .map(|(key, value)| key.len() + value.len())
        .sum();
    source
        .derived
        .get_mut("pad")
        .unwrap()
        .push_str(&"x".repeat(bytes - used));
}

#[test]
fn intermediate_and_generated_capacity_cannot_be_hidden_by_narrowing_or_replacement() {
    let limits = EventQueryLimits::default();
    let query = project(json!("All"));
    let mut source = event(0, "1", "2");
    source.derived.remove("ratio");
    fill_to(&mut source, MAX_DERIVED_BYTES - "ratio".len() - "0.5".len());
    let mut expected = source.clone();
    expected
        .derived
        .insert("ratio".to_string(), "0.5".to_string());
    assert_eq!(
        evaluate(&query, std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![expected]
    );
    source.derived.get_mut("pad").unwrap().push('x');
    let narrow = |query: EventQuery| EventQuery::Narrow {
        input: Box::new(query),
        fields: vec!["ratio".to_string()],
    };
    assert_eq!(
        evaluate(&narrow(query.clone()), &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, "1", "2");
    source.derived.insert("ratio".to_string(), "0".to_string());
    while source.derived.len() < MAX_DERIVED_FIELDS {
        source
            .derived
            .insert(format!("field-{}", source.derived.len()), String::new());
    }
    let mut expected = source.clone();
    expected
        .derived
        .insert("ratio".to_string(), "0.5".to_string());
    assert_eq!(
        evaluate(&query, std::slice::from_ref(&source), &limits)
            .unwrap()
            .rows,
        vec![expected]
    );
    source.derived.remove("ratio");
    source.derived.insert("extra".to_string(), String::new());
    assert_eq!(
        evaluate(&narrow(query), &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, "1", "2");
    source.derived.remove("ratio");
    fill_to(&mut source, MAX_DERIVED_BYTES);
    let oversized_input = json!({"Map":{"input":"All","field":"ratio","pointer":"/used"}});
    let hidden = self::query(oversized_input, "ratio", "missing", "total");
    assert_eq!(
        evaluate(&narrow(hidden), &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
}

#[test]
fn shared_bounds_moments_and_receipts_bind_ratio_projection_facts() {
    let events = [event(0, "4", "4"), event(1, "2", "4")];
    let query = project(json!("All"));
    let exact = EventQueryLimits {
        max_nodes: 2,
        max_depth: 1,
        max_operations: 4,
        max_scanned_events: 2,
        ..EventQueryLimits::default()
    };
    let result = evaluate(&query, &events, &exact).unwrap();
    let mut expected = events.to_vec();
    expected[0]
        .derived
        .insert("ratio".to_string(), "1.0".to_string());
    expected[1]
        .derived
        .insert("ratio".to_string(), "0.5".to_string());
    assert_eq!(result.rows, expected);
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
        assert_eq!(evaluate(&query, &events, &limits), Err(error));
    }
    let nested = EventQuery::Narrow {
        input: Box::new(query.clone()),
        fields: vec!["ratio".to_string()],
    };
    assert_eq!(
        evaluate(
            &nested,
            &events,
            &EventQueryLimits {
                max_nodes: 3,
                max_operations: 6,
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
    assert_eq!(prefix.rows, expected[..1]);
    assert_eq!(prefix.matched, 2);
    assert!(prefix.truncated);
    let limits = EventQueryLimits::default();
    let original = project(json!({"UpTo":{"input":"All","moment_tick":0}}));
    let result = evaluate(&original, &events, &limits).unwrap();
    let receipt = receipt_for(&original, &events, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(0));
    validate_receipt(&receipt, &original, &events, &result, &limits).unwrap();
    for changed in [
        self::query(
            json!({"UpTo":{"input":"All","moment_tick":0}}),
            "ratio",
            "missing",
            "total",
        ),
        self::query(
            json!({"UpTo":{"input":"All","moment_tick":0}}),
            "ratio",
            "total",
            "used",
        ),
        self::query(
            json!({"UpTo":{"input":"All","moment_tick":0}}),
            "ratio",
            "used",
            "used",
        ),
        self::query(
            json!({"UpTo":{"input":"All","moment_tick":0}}),
            "other",
            "used",
            "total",
        ),
    ] {
        assert!(validate_receipt(&receipt, &changed, &events, &result, &limits).is_err());
    }
    let mut changed = events.clone();
    changed[1].text = "canonical drift outside moment".to_string();
    assert!(validate_receipt(&receipt, &original, &changed, &result, &limits).is_err());
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("ratio".to_string(), "99".to_string());
    assert!(validate_receipt(&receipt, &original, &events, &forged, &limits).is_err());
    let mixed: EventQuery = serde_json::from_value(
        json!({"Union":{"inputs":[original,{"UpTo":{"input":"All","moment_tick":1}}]}}),
    )
    .unwrap();
    assert_eq!(
        evaluate(&mixed, &events, &limits),
        Err(EventQueryError::AmbiguousMoment)
    );
}
