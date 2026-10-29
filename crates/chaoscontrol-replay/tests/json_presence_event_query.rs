use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_EVENT_TEXT_BYTES,
    MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const SOURCES: [&str; 2] = ["JsonPresent", "JsonTextPresent"];

fn event(sequence: u64, details: Option<Value>) -> EventEnvelope {
    let text = details.as_ref().unwrap_or(&Value::Null).to_string();
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick: sequence,
        vm_index: Some(0),
        process: Some("monitor".to_string()),
        thread: None,
        kind: "serial".to_string(),
        text,
        payload_digest: [2; 32],
        details,
        derived: BTreeMap::from([("field".to_string(), "derived-only".to_string())]),
    }
}

fn leaf(source: &str, pointer: &str) -> Value {
    json!({source: {"pointer": pointer}})
}

fn query(predicate: Value) -> EventQuery {
    serde_json::from_value(json!({"Filter": {"input": "All", "predicate": predicate}}))
        .expect("JSON presence predicates must be supported")
}

fn observations() -> Vec<EventEnvelope> {
    let mut events: Vec<_> = [
        Value::Null,
        json!(false),
        json!(0),
        json!(""),
        json!([]),
        json!({}),
        json!({"nested": true}),
        json!(15),
        json!(80),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, value)| event(i as u64, Some(json!({"field": value}))))
    .collect();
    for details in [Some(json!({})), None, Some(json!(0))] {
        events.push(event(events.len() as u64, details));
    }
    events
}

// r[verify chaoscontrol.json_field_presence.selection]
#[test]
fn presence_distinguishes_all_resolved_json_values_from_absence_without_mutation() {
    let events = observations();
    let limits = EventQueryLimits::default();
    for source in SOURCES {
        assert_eq!(
            evaluate(&query(leaf(source, "/field")), &events, &limits)
                .unwrap()
                .rows,
            events[..9]
        );
        assert_eq!(
            evaluate(
                &query(json!({"Not": leaf(source, "/field")})),
                &events,
                &limits
            )
            .unwrap()
            .rows,
            events[9..]
        );
        let numeric_source = if source == "JsonPresent" {
            "JsonNumber"
        } else {
            "JsonTextNumber"
        };
        let numeric = json!({numeric_source: {"pointer": "/field", "comparison": "GreaterThan", "value": 50}});
        let guarded = query(json!({"All": [leaf(source, "/field"), {"Not": numeric}]}));
        assert_eq!(
            evaluate(&guarded, &events, &limits).unwrap().rows,
            events[..8]
        );
    }
    let mut conflicting = event(0, Some(json!({"field": null})));
    conflicting.text = "{}".to_string();
    assert_eq!(
        evaluate(
            &query(leaf("JsonPresent", "/field")),
            std::slice::from_ref(&conflicting),
            &limits
        )
        .unwrap()
        .rows,
        vec![conflicting.clone()]
    );
    assert_eq!(
        evaluate(
            &query(leaf("JsonTextPresent", "/field")),
            std::slice::from_ref(&conflicting),
            &limits
        )
        .unwrap()
        .rows,
        Vec::<EventEnvelope>::new()
    );
    conflicting.details = None;
    conflicting.text = "{\"field\":false}".to_string();
    assert_eq!(
        evaluate(
            &query(leaf("JsonPresent", "/field")),
            std::slice::from_ref(&conflicting),
            &limits
        )
        .unwrap()
        .rows,
        Vec::<EventEnvelope>::new()
    );
    assert_eq!(
        evaluate(
            &query(leaf("JsonTextPresent", "/field")),
            std::slice::from_ref(&conflicting),
            &limits
        )
        .unwrap()
        .rows,
        vec![conflicting.clone()]
    );
}

#[test]
fn escaped_unicode_array_paths_and_pointer_byte_boundaries_are_explicit() {
    let limits = EventQueryLimits::default();
    let source = event(0, Some(json!({"指标/量": {"~key": [null]}})));
    let key = "k".repeat(MAX_FIELD_BYTES - 1);
    let boundary = event(0, Some(json!({&key: null})));
    for kind in SOURCES {
        assert_eq!(
            evaluate(
                &query(leaf(kind, "/指标~1量/~0key/0")),
                std::slice::from_ref(&source),
                &limits
            )
            .unwrap()
            .rows,
            vec![source.clone()]
        );
        for pointer in [
            "/指标~1量/~0key/1",
            "/指标~1量/~0key/0/nested",
            "/指标~1量/~0key/00",
        ] {
            assert_eq!(
                evaluate(
                    &query(leaf(kind, pointer)),
                    std::slice::from_ref(&source),
                    &limits
                )
                .unwrap()
                .rows,
                Vec::<EventEnvelope>::new()
            );
        }
        assert_eq!(
            evaluate(
                &query(leaf(kind, &format!("/{key}"))),
                std::slice::from_ref(&boundary),
                &limits
            )
            .unwrap()
            .rows,
            vec![boundary.clone()]
        );
        for pointer in [
            "".to_string(),
            "field".to_string(),
            "/bad\n".to_string(),
            format!("/{}", "k".repeat(MAX_FIELD_BYTES)),
        ] {
            let invalid = query(json!({"Any": [{"Process": "monitor"}, leaf(kind, &pointer)]}));
            assert_eq!(
                evaluate(&invalid, std::slice::from_ref(&source), &limits),
                Err(EventQueryError::InvalidPointer)
            );
            assert_eq!(
                evaluate(&invalid, &[], &limits),
                Err(EventQueryError::InvalidPointer)
            );
        }
    }
}

#[test]
fn text_parse_failures_propagate_only_when_visited_and_do_not_affect_details_presence() {
    let limits = EventQueryLimits::default();
    for text in [
        "not-json".to_string(),
        "{\"field\":null} trailing".to_string(),
        format!("{}0{}", "[".repeat(200), "]".repeat(200)),
        "{\"field\":1e400}".to_string(),
    ] {
        let mut source = event(0, Some(json!({"field": null})));
        source.text = text;
        let events = std::slice::from_ref(&source);
        assert_eq!(
            evaluate(&query(leaf("JsonPresent", "/field")), events, &limits)
                .unwrap()
                .rows,
            vec![source.clone()]
        );
        for predicate in [
            leaf("JsonTextPresent", "/field"),
            json!({"Not": leaf("JsonTextPresent", "/field")}),
            json!({"All": [{"Process": "monitor"}, leaf("JsonTextPresent", "/field")]}),
            json!({"Any": [{"Process": "other"}, leaf("JsonTextPresent", "/field")]}),
        ] {
            assert_eq!(
                evaluate(&query(predicate), events, &limits),
                Err(EventQueryError::InvalidJsonText)
            );
        }
        let exact = EventQueryLimits {
            max_operations: 3,
            ..limits
        };
        let skip_all =
            query(json!({"All": [{"Process": "other"}, leaf("JsonTextPresent", "/field")]}));
        assert_eq!(
            evaluate(&skip_all, events, &exact).unwrap().rows,
            Vec::<EventEnvelope>::new()
        );
        let skip_any =
            query(json!({"Any": [{"Process": "monitor"}, leaf("JsonTextPresent", "/field")]}));
        assert_eq!(
            evaluate(&skip_any, events, &exact).unwrap().rows,
            vec![source.clone()]
        );
        assert_eq!(
            evaluate(
                &skip_any,
                events,
                &EventQueryLimits {
                    max_operations: 2,
                    ..limits
                }
            ),
            Err(EventQueryError::OperationBound)
        );
    }
}

#[test]
fn presence_queries_share_text_operation_and_structural_bounds() {
    let base = "{\"field\":null,\"pad\":\"\"}";
    let mut source = event(0, None);
    source.text = format!(
        "{{\"field\":null,\"pad\":\"{}\"}}",
        "x".repeat(MAX_EVENT_TEXT_BYTES - base.len())
    );
    let text_query = query(leaf("JsonTextPresent", "/field"));
    assert_eq!(
        evaluate(
            &text_query,
            std::slice::from_ref(&source),
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        vec![source.clone()]
    );
    source.text.push(' ');
    assert_eq!(
        evaluate(&text_query, &[source], &EventQueryLimits::default()),
        Err(EventQueryError::InvalidText)
    );
    let events = observations();
    for kind in SOURCES {
        let presence = query(leaf(kind, "/field"));
        let exact = EventQueryLimits {
            max_operations: events.len() * 2,
            ..EventQueryLimits::default()
        };
        assert_eq!(
            evaluate(&presence, &events, &exact).unwrap().rows,
            events[..9]
        );
        assert_eq!(
            evaluate(
                &presence,
                &events,
                &EventQueryLimits {
                    max_operations: exact.max_operations - 1,
                    ..exact
                }
            ),
            Err(EventQueryError::OperationBound)
        );
        let nested = query(json!({"Not": {"Not": leaf(kind, "/field")}}));
        assert_eq!(
            evaluate(
                &nested,
                &events,
                &EventQueryLimits {
                    max_nodes: 2,
                    ..exact
                }
            ),
            Err(EventQueryError::QueryBound)
        );
        assert_eq!(
            evaluate(
                &nested,
                &events,
                &EventQueryLimits {
                    max_depth: 1,
                    ..exact
                }
            ),
            Err(EventQueryError::QueryDepth)
        );
    }
}

#[test]
fn presence_receipts_bind_pointer_source_kind_source_content_and_result_under_moments() {
    let events = [
        event(0, Some(json!({"field":null,"alias":false}))),
        event(1, Some(json!({}))),
    ];
    let limits = EventQueryLimits::default();
    for kind in SOURCES {
        let original = EventQuery::UpTo {
            input: Box::new(query(leaf(kind, "/field"))),
            moment_tick: 1,
        };
        let result = evaluate(&original, &events, &limits).unwrap();
        assert_eq!(result.rows, vec![events[0].clone()]);
        let receipt = receipt_for(&original, &events, &result, &limits).unwrap();
        assert_eq!(receipt.moment_tick, Some(1));
        validate_receipt(&receipt, &original, &events, &result, &limits).unwrap();
        let other = if kind == "JsonPresent" {
            "JsonTextPresent"
        } else {
            "JsonPresent"
        };
        for (source_kind, pointer) in [(kind, "/alias"), (other, "/field")] {
            let changed = EventQuery::UpTo {
                input: Box::new(query(leaf(source_kind, pointer))),
                moment_tick: 1,
            };
            assert_eq!(
                validate_receipt(&receipt, &changed, &events, &result, &limits),
                Err(EventQueryError::ReceiptDrift)
            );
        }
        let mut changed = events.clone();
        changed[1].text = "{\"unrelated\":true}".to_string();
        assert_eq!(
            validate_receipt(&receipt, &original, &changed, &result, &limits),
            Err(EventQueryError::SourceDrift)
        );
        let mut forged = result.clone();
        forged.rows[0].details = Some(json!({"field":false,"alias":false}));
        assert_eq!(
            validate_receipt(&receipt, &original, &events, &forged, &limits),
            Err(EventQueryError::ResultDrift)
        );
    }
}
