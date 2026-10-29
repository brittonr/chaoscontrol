use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn event(sequence: u64, tick: u64) -> EventEnvelope {
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick,
        vm_index: Some(0),
        process: Some("json-monitor".to_string()),
        thread: Some(7),
        kind: "serial".to_string(),
        text: r#"{"component":"storage","used":80}"#.to_string(),
        payload_digest: [9; 32],
        details: Some(json!({"status": "ready"})),
        derived: BTreeMap::from([
            ("selected".to_string(), "值".to_string()),
            ("Selected".to_string(), "case-sensitive".to_string()),
            ("temporary".to_string(), "discard".to_string()),
            ("kind".to_string(), "shadow".to_string()),
        ]),
    }
}

fn narrow(input: Value, fields: Value) -> EventQuery {
    serde_json::from_value(json!({"Narrow": {"input": input, "fields": fields}}))
        .expect("bounded projection narrowing must be supported")
}

// r[verify chaoscontrol.narrow_query.projections]
#[test]
fn narrowing_preserves_full_source_envelopes_and_selects_exact_keys() {
    let events = [event(0, 30), event(1, 10)];
    for (fields, selected) in [
        (json!(["selected", "missing", "selected"]), vec!["selected"]),
        (
            json!(["Selected", "selected"]),
            vec!["Selected", "selected"],
        ),
        (
            json!(["selected", "Selected"]),
            vec!["Selected", "selected"],
        ),
        (json!(["missing"]), vec![]),
        (json!([]), vec![]),
    ] {
        let mut expected = vec![events[1].clone(), events[0].clone()];
        for row in &mut expected {
            row.derived = selected
                .iter()
                .map(|key| (key.to_string(), events[0].derived[*key].clone()))
                .collect();
        }
        let result = evaluate(
            &narrow(json!("All"), fields),
            &events,
            &EventQueryLimits::default(),
        )
        .unwrap();
        assert_eq!(result.rows, expected);
        assert_eq!(
            (result.scanned, result.matched, result.truncated),
            (2, 2, false)
        );
    }
}

#[test]
fn projections_compose_before_and_after_narrowing_with_canonical_precedence() {
    let events = [event(0, 10)];
    let projected = json!({"MapText": {
        "input": {"Map": {"input": "All", "field": "state", "pointer": "/status"}},
        "field": "component", "pointer": "/component"
    }});
    let selected = narrow(projected, json!(["component", "kind"]));
    let filtered = EventQuery::Matches {
        input: Box::new(selected.clone()),
        field: "component".to_string(),
        text: "storage".to_string(),
    };
    let mut expected = events[0].clone();
    expected.derived = BTreeMap::from([
        ("component".to_string(), "storage".to_string()),
        ("kind".to_string(), "shadow".to_string()),
    ]);
    assert_eq!(
        evaluate(&filtered, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        vec![expected.clone()]
    );
    for (field, text, matches) in [
        ("state", "ready", false),
        ("kind", "shadow", false),
        ("kind", "serial", true),
    ] {
        let query = EventQuery::Matches {
            input: Box::new(selected.clone()),
            field: field.to_string(),
            text: text.to_string(),
        };
        assert_eq!(
            evaluate(&query, &events, &EventQueryLimits::default())
                .unwrap()
                .rows,
            if matches {
                vec![expected.clone()]
            } else {
                vec![]
            }
        );
    }
    let remapped = EventQuery::Map {
        input: Box::new(narrow(json!("All"), json!([]))),
        field: "state".to_string(),
        pointer: "/status".to_string(),
    };
    expected.derived = BTreeMap::from([("state".to_string(), "ready".to_string())]);
    assert_eq!(
        evaluate(&remapped, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        vec![expected]
    );
}

#[test]
fn selection_admission_enforces_count_utf8_and_names_even_without_rows() {
    let limits = EventQueryLimits::default();
    let name = "é".repeat(MAX_FIELD_BYTES / 2);
    let mut source = event(0, 10);
    source.derived.insert(name.clone(), "boundary".to_string());
    let mut expected = source.clone();
    expected.derived = BTreeMap::from([(name.clone(), "boundary".to_string())]);
    assert_eq!(
        evaluate(
            &narrow(json!("All"), json!(vec![&name; MAX_DERIVED_FIELDS])),
            &[source],
            &limits
        )
        .unwrap()
        .rows,
        vec![expected]
    );
    assert_eq!(
        evaluate(
            &narrow(json!("All"), json!(vec!["x"; MAX_DERIVED_FIELDS + 1])),
            &[],
            &limits
        ),
        Err(EventQueryError::QueryBound)
    );
    for invalid in [
        String::new(),
        format!("{name}x"),
        "line\nbreak".to_string(),
        "null\0byte".to_string(),
    ] {
        assert_eq!(
            evaluate(&narrow(json!("All"), json!([invalid])), &[], &limits),
            Err(EventQueryError::InvalidField)
        );
    }
    assert_eq!(
        evaluate(&narrow(json!("All"), json!([])), &[], &limits)
            .unwrap()
            .rows,
        vec![]
    );
}

#[test]
fn removing_all_projections_cannot_hide_invalid_child_rows_or_parsing() {
    let limits = EventQueryLimits::default();
    let mut source = event(0, 10);
    source.derived = (0..MAX_DERIVED_FIELDS)
        .map(|i| (format!("f{i}"), String::new()))
        .collect();
    let query = narrow(
        json!({"Map": {"input": "All", "field": "overflow", "pointer": "/status"}}),
        json!([]),
    );
    assert_eq!(
        evaluate(&query, &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, 10);
    source.derived = BTreeMap::from([(
        "large".to_string(),
        "x".repeat(MAX_DERIVED_BYTES - "large".len()),
    )]);
    let query = narrow(
        json!({"Map": {"input": "All", "field": "extra", "pointer": "/status"}}),
        json!([]),
    );
    assert_eq!(
        evaluate(&query, &[source], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut source = event(0, 10);
    source.text = "not JSON".to_string();
    let query = narrow(
        json!({"MapText": {"input": "All", "field": "unused", "pointer": "/x"}}),
        json!([]),
    );
    assert_eq!(
        evaluate(&query, &[source], &limits),
        Err(EventQueryError::InvalidJsonText)
    );
}

#[test]
fn narrowing_shares_work_structure_and_result_boundaries() {
    let events = [event(0, 30), event(1, 10)];
    let query = narrow(json!("All"), json!(["selected"]));
    let exact = EventQueryLimits {
        max_nodes: 2,
        max_depth: 1,
        max_operations: 4,
        ..EventQueryLimits::default()
    };
    let result = evaluate(&query, &events, &exact).unwrap();
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
                max_result_bytes: bytes - 1,
                ..exact
            },
            EventQueryError::ResultBound,
        ),
        (
            EventQueryLimits {
                max_scanned_events: 1,
                ..exact
            },
            EventQueryError::ScanBound,
        ),
    ] {
        assert_eq!(evaluate(&query, &events, &limits), Err(error));
    }
    let wrapped = EventQuery::UpTo {
        input: Box::new(query.clone()),
        moment_tick: 30,
    };
    assert_eq!(
        evaluate(
            &wrapped,
            &events,
            &EventQueryLimits {
                max_nodes: 3,
                ..exact
            }
        ),
        Err(EventQueryError::QueryDepth)
    );
    let truncated = evaluate(
        &query,
        &events,
        &EventQueryLimits {
            max_result_rows: 1,
            ..exact
        },
    )
    .unwrap();
    assert_eq!(truncated.rows, result.rows[..1]);
    assert_eq!((truncated.matched, truncated.truncated), (2, true));
}

#[test]
fn narrowed_moments_duplicate_precedence_and_receipts_retain_all_inputs() {
    let limits = EventQueryLimits::default();
    let events = [event(0, 30), event(1, 10)];
    let query = narrow(
        json!({"UpTo": {"input": "All", "moment_tick": 10}}),
        json!(["selected", "missing"]),
    );
    let result = evaluate(&query, &events, &limits).unwrap();
    let receipt = receipt_for(&query, &events, &result, &limits).unwrap();
    assert_eq!(result.rows[0].source_sequence, 1);
    assert_eq!(receipt.moment_tick, Some(10));
    validate_receipt(&receipt, &query, &events, &result, &limits).unwrap();
    for fields in [
        json!(["missing", "selected"]),
        json!(["selected"]),
        json!([]),
    ] {
        let changed = narrow(json!({"UpTo": {"input": "All", "moment_tick": 10}}), fields);
        assert!(validate_receipt(&receipt, &changed, &events, &result, &limits).is_err());
    }
    let mut changed = events.clone();
    changed[0].text = "changed canonical source outside selected moment".to_string();
    assert_eq!(evaluate(&query, &changed, &limits).unwrap(), result);
    assert!(validate_receipt(&receipt, &query, &changed, &result, &limits).is_err());
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("temporary".to_string(), "forged".to_string());
    assert!(validate_receipt(&receipt, &query, &events, &forged, &limits).is_err());
    let mixed =
        json!({"Union": {"inputs": [query, {"UpTo": {"input": "All", "moment_tick": 30}}]}});
    let mixed: EventQuery = serde_json::from_value(mixed).unwrap();
    assert_eq!(
        evaluate(&mixed, &events, &limits),
        Err(EventQueryError::AmbiguousMoment)
    );
    let outer = EventQuery::UpTo {
        input: Box::new(mixed),
        moment_tick: 10,
    };
    assert_eq!(
        receipt_for(
            &outer,
            &events,
            &evaluate(&outer, &events, &limits).unwrap(),
            &limits
        )
        .unwrap()
        .moment_tick,
        Some(10)
    );
    let mut first = event(0, 10);
    first.derived = BTreeMap::from([("selected".to_string(), "first".to_string())]);
    let mut second = first.clone();
    second
        .derived
        .insert("selected".to_string(), "second".to_string());
    assert_eq!(
        evaluate(
            &narrow(json!("All"), json!(["selected"])),
            &[first.clone(), second],
            &limits
        )
        .unwrap()
        .rows,
        vec![first]
    );
}
