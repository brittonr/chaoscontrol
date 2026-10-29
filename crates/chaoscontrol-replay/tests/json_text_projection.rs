use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_FIELDS,
    MAX_EVENT_TEXT_BYTES, MAX_FIELD_BYTES,
};
use serde_json::json;
use std::collections::BTreeMap;

fn event(sequence: u64, tick: u64, text: &str) -> EventEnvelope {
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick,
        vm_index: Some(0),
        process: Some("json-monitor".to_string()),
        thread: None,
        kind: "serial".to_string(),
        text: text.to_string(),
        payload_digest: [1; 32],
        details: Some(json!({"component": "canonical-details"})),
        derived: BTreeMap::new(),
    }
}

fn projection(pointer: &str, field: &str) -> EventQuery {
    serde_json::from_value(json!({"MapText": {"input": "All", "pointer": pointer, "field": field}}))
        .expect("JSON text projections must be supported")
}

// r[verify chaoscontrol.json_text_projection.mapping]
#[test]
fn json_stdout_projection_and_selection_preserve_source_fields() {
    let events = vec![
        event(0, 30, r#"{"component":"storage","used":80}"#),
        event(1, 10, r#"{"component":"control","used":90}"#),
        event(2, 20, r#"{"component":"storage","used":50}"#),
    ];
    let query = EventQuery::Matches {
        input: Box::new(projection("/component", "component")),
        field: "component".to_string(),
        text: "storage".to_string(),
    };
    let mut expected = vec![events[2].clone(), events[0].clone()];
    for event in &mut expected {
        event
            .derived
            .insert("component".to_string(), "storage".to_string());
    }
    assert_eq!(
        evaluate(&query, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        expected
    );
}

#[test]
fn scalar_conversion_and_missing_values_follow_map_semantics() {
    let mut source = event(
        0,
        10,
        r#"{"escaped/field":{"~key":"值"},"array":[17],"fraction":-2.5,"boolean":true,"null":null,"object":{}}"#,
    );
    source
        .derived
        .insert("selected".to_string(), "previous".to_string());
    let limits = EventQueryLimits::default();
    for (pointer, selected) in [
        ("/escaped~1field/~0key", "值"),
        ("/array/0", "17"),
        ("/fraction", "-2.5"),
        ("/boolean", "true"),
        ("/missing", "previous"),
        ("/null", "previous"),
        ("/array", "previous"),
        ("/object", "previous"),
    ] {
        let mut expected = source.clone();
        expected
            .derived
            .insert("selected".to_string(), selected.to_string());
        assert_eq!(
            evaluate(&projection(pointer, "selected"), &[source.clone()], &limits)
                .unwrap()
                .rows,
            vec![expected]
        );
    }
    let mut expected = source.clone();
    expected
        .derived
        .insert("text".to_string(), "true".to_string());
    assert_eq!(
        evaluate(&projection("/boolean", "text"), &[source], &limits)
            .unwrap()
            .rows,
        vec![expected]
    );
}

#[test]
fn malformed_trailing_and_deep_json_fail_without_partial_results() {
    let valid = event(0, 10, r#"{"x":1}"#);
    let query = projection("/x", "x");
    let limits = EventQueryLimits::default();
    for invalid in [
        "{".to_string(),
        r#"{"x":1}tail"#.to_string(),
        r#"{"x":NaN}"#.to_string(),
        format!("{}0{}", "[".repeat(200), "]".repeat(200)),
    ] {
        let events = [valid.clone(), event(1, 20, &invalid)];
        assert_eq!(
            evaluate(&query, &events, &limits),
            Err(EventQueryError::InvalidJsonText)
        );
    }
    let mut exact = valid;
    exact
        .text
        .push_str(&" ".repeat(MAX_EVENT_TEXT_BYTES - exact.text.len()));
    let mut expected = exact.clone();
    expected.derived.insert("x".to_string(), "1".to_string());
    assert_eq!(
        evaluate(&query, &[exact.clone()], &limits).unwrap().rows,
        vec![expected]
    );
    exact.text.push(' ');
    assert_eq!(
        evaluate(&query, &[exact], &limits),
        Err(EventQueryError::InvalidText)
    );
}

#[test]
fn projection_admission_and_derived_field_capacity_remain_bounded() {
    let key = "p".repeat(MAX_FIELD_BYTES - 1);
    let pointer = format!("/{key}");
    let field = "f".repeat(MAX_FIELD_BYTES);
    let source = event(0, 10, &json!({key: 1}).to_string());
    let limits = EventQueryLimits::default();
    let mut expected = source.clone();
    expected.derived.insert(field.clone(), "1".to_string());
    assert_eq!(
        evaluate(
            &projection(&pointer, &field),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![expected]
    );
    for (pointer, field, error) in [
        (
            String::new(),
            "field".to_string(),
            EventQueryError::InvalidPointer,
        ),
        (
            "not-a-pointer".to_string(),
            "field".to_string(),
            EventQueryError::InvalidPointer,
        ),
        (
            format!("{pointer}x"),
            "field".to_string(),
            EventQueryError::InvalidPointer,
        ),
        (
            "/x".to_string(),
            format!("{field}x"),
            EventQueryError::InvalidField,
        ),
    ] {
        assert_eq!(
            evaluate(&projection(&pointer, &field), &[], &limits),
            Err(error)
        );
    }
    let mut full = event(0, 10, r#"{"x":1}"#);
    full.derived = (0..MAX_DERIVED_FIELDS)
        .map(|i| (format!("f{i}"), String::new()))
        .collect();
    let mut expected = full.clone();
    expected.derived.insert("f0".to_string(), "1".to_string());
    assert_eq!(
        evaluate(&projection("/x", "f0"), &[full.clone()], &limits)
            .unwrap()
            .rows,
        vec![expected]
    );
    assert_eq!(
        evaluate(&projection("/x", "new"), &[full], &limits),
        Err(EventQueryError::InvalidEvent)
    );
}

#[test]
fn projection_charges_existing_row_structural_and_result_budgets() {
    let events = [event(0, 10, r#"{"x":1}"#), event(1, 20, r#"{"x":2}"#)];
    let query = projection("/x", "x");
    let exact = EventQueryLimits {
        max_nodes: 2,
        max_depth: 1,
        max_operations: events.len() * 2,
        ..EventQueryLimits::default()
    };
    let result = evaluate(&query, &events, &exact).unwrap();
    let mut expected = events.to_vec();
    for (index, event) in expected.iter_mut().enumerate() {
        event
            .derived
            .insert("x".to_string(), (index + 1).to_string());
    }
    assert_eq!(result.rows, expected);
    assert_eq!(
        evaluate(
            &query,
            &events,
            &EventQueryLimits {
                max_operations: exact.max_operations - 1,
                ..exact
            }
        ),
        Err(EventQueryError::OperationBound)
    );
    assert_eq!(
        evaluate(
            &query,
            &events,
            &EventQueryLimits {
                max_nodes: 1,
                ..exact
            }
        ),
        Err(EventQueryError::QueryBound)
    );
    let wrapped = EventQuery::UpTo {
        input: Box::new(query.clone()),
        moment_tick: 20,
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
    assert_eq!(
        evaluate(
            &query,
            &events,
            &EventQueryLimits {
                max_result_bytes: bytes - 1,
                ..exact
            }
        ),
        Err(EventQueryError::ResultBound)
    );
}

#[test]
fn receipts_bind_text_projection_pointer_results_sources_and_moments() {
    let events = [
        event(0, 10, r#"{"component":"storage","alias":"storage"}"#),
        event(1, 20, r#"{"component":"control","alias":"control"}"#),
    ];
    let query = EventQuery::Matches {
        input: Box::new(projection("/component", "component")),
        field: "component".to_string(),
        text: "storage".to_string(),
    };
    let limits = EventQueryLimits::default();
    let result = evaluate(&query, &events, &limits).unwrap();
    let receipt = receipt_for(&query, &events, &result, &limits).unwrap();
    validate_receipt(&receipt, &query, &events, &result, &limits).unwrap();
    let equivalent = EventQuery::Matches {
        input: Box::new(projection("/alias", "component")),
        field: "component".to_string(),
        text: "storage".to_string(),
    };
    assert_eq!(evaluate(&equivalent, &events, &limits).unwrap(), result);
    assert_eq!(
        validate_receipt(&receipt, &equivalent, &events, &result, &limits),
        Err(EventQueryError::ReceiptDrift)
    );
    let mut changed = events.clone();
    changed[1].text = r#"{"component":"control","alias":"control","extra":true}"#.to_string();
    assert_eq!(
        validate_receipt(&receipt, &query, &changed, &result, &limits),
        Err(EventQueryError::SourceDrift)
    );
    let mut changed_result = result.clone();
    changed_result.rows[0]
        .derived
        .insert("component".to_string(), "forged".to_string());
    assert_eq!(
        validate_receipt(&receipt, &query, &events, &changed_result, &limits),
        Err(EventQueryError::ResultDrift)
    );
    let mut at_moment = projection("/component", "component");
    let EventQuery::MapText { input, .. } = &mut at_moment else {
        unreachable!()
    };
    **input = EventQuery::UpTo {
        input: Box::new(EventQuery::All),
        moment_tick: 10,
    };
    let bounded = evaluate(&at_moment, &events, &limits).unwrap();
    assert_eq!(bounded.rows, result.rows);
    assert_eq!(
        receipt_for(&at_moment, &events, &bounded, &limits)
            .unwrap()
            .moment_tick,
        Some(10)
    );
    let mixed = EventQuery::Union {
        inputs: vec![
            at_moment,
            EventQuery::UpTo {
                input: Box::new(projection("/component", "component")),
                moment_tick: 20,
            },
        ],
    };
    assert_eq!(
        evaluate(&mixed, &events, &limits),
        Err(EventQueryError::AmbiguousMoment)
    );
}
