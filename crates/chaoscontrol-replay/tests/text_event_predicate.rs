use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_EVENT_TEXT_BYTES,
    MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn filter(predicate: Value) -> EventQuery {
    serde_json::from_value(json!({"Filter": {"input": "All", "predicate": predicate}}))
        .expect("text predicates must compose")
}

fn text_predicate(operator: &str, field: &str, text: &str) -> Value {
    json!({operator: {"field": field, "text": text}})
}

fn observations() -> Vec<EventEnvelope> {
    [
        (30, "[WARN] retry disk", Some("worker/a"), Some("storage")),
        (10, "[ERROR] heartbeat", Some("monitor"), Some("control")),
        (20, "[WARN] disk down", Some("worker/b"), Some("storage")),
        (40, "[warn] retry disk", None, None),
        (50, "警告: 重试磁盘", Some("worker/c"), Some("storage")),
        (60, "[ERROR] permanent", Some("worker/a"), Some("storage")),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (tick, text, process, component))| EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Recording,
        source_sequence: index as u64,
        tick,
        vm_index: Some(0),
        process: process.map(str::to_string),
        thread: None,
        kind: "serial".to_string(),
        text: text.to_string(),
        payload_digest: [1; 32],
        details: component.map(|component| json!({"component": component})),
        derived: BTreeMap::from([
            ("text".to_string(), "shadow".to_string()),
            ("process".to_string(), "shadow".to_string()),
            ("kind".to_string(), "shadow".to_string()),
            ("source_kind".to_string(), "shadow".to_string()),
        ]),
    })
    .collect()
}

// r[verify chaoscontrol.text_event_predicate.selection]
#[test]
fn warning_and_error_selection_can_exclude_heartbeat_text() {
    let events = observations();
    let query = filter(json!({"All": [
        {"Any": [text_predicate("Contains", "text", "[WARN]"), text_predicate("Contains", "text", "[ERROR]")]},
        {"Not": text_predicate("Contains", "text", "heartbeat")}
    ]}));
    assert_eq!(
        evaluate(&query, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        vec![events[2].clone(), events[0].clone(), events[5].clone()]
    );
}

#[test]
fn text_comparison_is_literal_case_sensitive_and_preserves_canonical_precedence() {
    let events = observations();
    let limits = EventQueryLimits::default();
    for (operator, field, text, indexes) in [
        ("Matches", "text", "[WARN] retry disk", vec![0]),
        ("Matches", "text", "retry", vec![]),
        ("Contains", "text", "[warn]", vec![3]),
        ("Contains", "text", "重试", vec![4]),
        ("Matches", "text", "警告: 重试磁盘", vec![4]),
        ("Matches", "text", "shadow", vec![]),
        ("Matches", "process", "shadow", vec![]),
        ("Contains", "kind", "shadow", vec![]),
        (
            "Matches",
            "source_kind",
            "recording",
            vec![1, 2, 0, 3, 4, 5],
        ),
        ("Contains", "absent", "anything", vec![]),
    ] {
        let expected: Vec<_> = indexes.into_iter().map(|i| events[i].clone()).collect();
        assert_eq!(
            evaluate(
                &filter(text_predicate(operator, field, text)),
                &events,
                &limits
            )
            .unwrap()
            .rows,
            expected
        );
    }
    assert_eq!(
        evaluate(
            &filter(json!({"Not": text_predicate("Contains", "process", "worker")})),
            &events,
            &limits
        )
        .unwrap()
        .rows,
        vec![events[1].clone(), events[3].clone()]
    );
}

#[test]
fn mapped_strings_are_selectable_without_overriding_canonical_text() {
    let events = observations();
    for (field, operator, needle, indexes) in [
        ("component", "Matches", "storage", vec![2, 0, 4, 5]),
        ("text", "Contains", "[ERROR]", vec![1, 5]),
    ] {
        let mut query = filter(text_predicate(operator, field, needle));
        let EventQuery::Filter { input, .. } = &mut query else {
            unreachable!()
        };
        **input = EventQuery::Map {
            input: Box::new(EventQuery::All),
            field: field.to_string(),
            pointer: "/component".to_string(),
        };
        let expected: Vec<_> = indexes
            .into_iter()
            .map(|i| {
                let mut event = events[i].clone();
                let component = event.details.as_ref().unwrap()["component"]
                    .as_str()
                    .unwrap();
                event
                    .derived
                    .insert(field.to_string(), component.to_string());
                event
            })
            .collect();
        assert_eq!(
            evaluate(&query, &events, &EventQueryLimits::default())
                .unwrap()
                .rows,
            expected
        );
    }
}

#[test]
fn field_and_needle_bounds_apply_even_to_skipped_text_leaves() {
    let mut events = observations();
    events.truncate(1);
    let field = "f".repeat(MAX_FIELD_BYTES);
    let needle = "é".repeat(MAX_EVENT_TEXT_BYTES / 2);
    events[0].text = needle.clone();
    events[0].derived.insert(field.clone(), needle.clone());
    let limits = EventQueryLimits::default();
    for operator in ["Contains", "Matches"] {
        assert_eq!(
            evaluate(
                &filter(text_predicate(operator, &field, &needle)),
                &events,
                &limits
            )
            .unwrap()
            .rows,
            events
        );
        for source in [events.as_slice(), &[]] {
            for (bad_field, bad_needle, error) in [
                (
                    String::new(),
                    "x".to_string(),
                    EventQueryError::InvalidField,
                ),
                (
                    "bad\n".to_string(),
                    "x".to_string(),
                    EventQueryError::InvalidField,
                ),
                (
                    format!("{field}x"),
                    "x".to_string(),
                    EventQueryError::InvalidField,
                ),
                (
                    "text".to_string(),
                    String::new(),
                    EventQueryError::InvalidText,
                ),
                (
                    "text".to_string(),
                    "bad\n".to_string(),
                    EventQueryError::InvalidText,
                ),
                (
                    "text".to_string(),
                    format!("{needle}x"),
                    EventQueryError::InvalidText,
                ),
            ] {
                let query = filter(
                    json!({"Any": [{"All": []}, text_predicate(operator, &bad_field, &bad_needle)]}),
                );
                assert_eq!(evaluate(&query, source, &limits), Err(error));
            }
        }
    }
}

#[test]
fn text_leaves_use_existing_flat_and_composite_operation_costs() {
    let events = observations();
    let flat = EventQueryLimits {
        max_operations: 12,
        ..EventQueryLimits::default()
    };
    let query = filter(text_predicate("Contains", "text", "[WARN]"));
    assert_eq!(
        evaluate(&query, &events, &flat).unwrap().rows,
        vec![events[2].clone(), events[0].clone()]
    );
    assert_eq!(
        evaluate(
            &query,
            &events,
            &EventQueryLimits {
                max_operations: 11,
                ..flat
            }
        ),
        Err(EventQueryError::OperationBound)
    );
    let compound = filter(
        json!({"All": [text_predicate("Contains", "text", "[WARN]"), {"Not": text_predicate("Matches", "process", "worker/b")}]}),
    );
    let exact = EventQueryLimits {
        max_operations: 22,
        ..flat
    };
    assert_eq!(
        evaluate(&compound, &events, &exact).unwrap().rows,
        vec![events[0].clone()]
    );
    assert_eq!(
        evaluate(
            &compound,
            &events,
            &EventQueryLimits {
                max_operations: 21,
                ..exact
            }
        ),
        Err(EventQueryError::OperationBound)
    );
}

#[test]
fn receipts_bind_text_even_when_a_different_needle_selects_identical_rows() {
    let events = observations();
    let limits = EventQueryLimits::default();
    let query = filter(text_predicate("Contains", "text", "[WARN]"));
    let result = evaluate(&query, &events, &limits).unwrap();
    let receipt = receipt_for(&query, &events, &result, &limits).unwrap();
    validate_receipt(&receipt, &query, &events, &result, &limits).unwrap();
    let changed = filter(text_predicate("Contains", "text", "[WAR"));
    assert_eq!(evaluate(&changed, &events, &limits).unwrap(), result);
    assert_eq!(
        validate_receipt(&receipt, &changed, &events, &result, &limits),
        Err(EventQueryError::ReceiptDrift)
    );
}
