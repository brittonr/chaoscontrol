use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_EVENT_TEXT_BYTES,
    MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn query(predicate: Value) -> EventQuery {
    serde_json::from_value(json!({"Filter": {"input": "All", "predicate": predicate}}))
        .expect("regex predicates must be supported")
}

fn leaf(field: &str, pattern: &str) -> Value {
    json!({"Regex": {"field": field, "pattern": pattern}})
}

fn events() -> Vec<EventEnvelope> {
    [
        (
            30,
            "prefix ERR42 timeout",
            Some("worker-1"),
            Some("store-12"),
        ),
        (10, "ERR7 retry", Some("worker-2"), Some("store-8")),
        (20, "err42 timeout", None, None),
        (40, "healthy Δοκιμή", Some("worker-A"), Some("")),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (tick, text, process, label))| {
        let mut derived = BTreeMap::from([
            ("text".to_string(), "ERR999".to_string()),
            ("process".to_string(), "worker-999".to_string()),
        ]);
        if let Some(label) = label {
            derived.insert("label".to_string(), label.to_string());
        }
        EventEnvelope {
            schema: EVENT_ENVELOPE_SCHEMA.to_string(),
            source_id: [1; 32],
            source_kind: EventSourceKind::Serial,
            source_sequence: i as u64,
            tick,
            vm_index: Some(0),
            process: process.map(str::to_string),
            thread: None,
            kind: "serial".to_string(),
            text: text.to_string(),
            payload_digest: [2; 32],
            details: Some(json!({"label": "canonical-details"})),
            derived,
        }
    })
    .collect()
}

fn selected(events: &[EventEnvelope], indexes: &[usize]) -> Vec<EventEnvelope> {
    indexes.iter().map(|i| events[*i].clone()).collect()
}

// r[verify chaoscontrol.event_regex.selection]
#[test]
fn regex_search_anchors_unicode_and_flags_preserve_canonical_rows_and_literal_selectors() {
    let events = events();
    let limits = EventQueryLimits::default();
    for (pattern, indexes) in [
        (r"ERR[0-9]+", vec![1, 0]),
        (r"^ERR\d+ retry$", vec![1]),
        (r"(?i)err[0-9]+", vec![1, 2, 0]),
        (r"\p{Greek}+", vec![3]),
        (r"^ERR|timeout$", vec![1, 2, 0]),
    ] {
        assert_eq!(
            evaluate(&query(leaf("text", pattern)), &events, &limits)
                .unwrap()
                .rows,
            selected(&events, &indexes),
            "{pattern}"
        );
    }
    assert_eq!(
        evaluate(
            &query(json!({"Contains": {"field": "text", "text": "ERR[0-9]+"}})),
            &events,
            &limits
        )
        .unwrap()
        .rows,
        Vec::<EventEnvelope>::new()
    );
    assert_eq!(
        evaluate(
            &query(json!({"Matches": {"field": "text", "text": "ERR7 retry"}})),
            &events,
            &limits
        )
        .unwrap()
        .rows,
        selected(&events, &[1])
    );
}

#[test]
fn derived_fields_missing_values_and_negation_follow_existing_resolution() {
    let events = events();
    let limits = EventQueryLimits::default();
    for (field, pattern, indexes) in [
        ("label", r"^store-\d+$", vec![1, 0]),
        ("label", r"^$", vec![3]),
        ("process", r"^worker-\d+$", vec![1, 0]),
        ("source_kind", "^serial$", vec![1, 2, 0, 3]),
    ] {
        assert_eq!(
            evaluate(&query(leaf(field, pattern)), &events, &limits)
                .unwrap()
                .rows,
            selected(&events, &indexes)
        );
    }
    assert_eq!(
        evaluate(
            &query(json!({"Not": leaf("label", "^$")})),
            &events,
            &limits
        )
        .unwrap()
        .rows,
        selected(&events, &[1, 2, 0])
    );
    let repeat = query(
        json!({"All": [leaf("text", "ERR"), {"Any": [leaf("text", "ERR"), leaf("label", "store")]}]}),
    );
    assert_eq!(
        evaluate(&repeat, &events, &limits).unwrap().rows,
        selected(&events, &[1, 0])
    );
}

#[test]
fn regex_filters_text_projection_and_inherits_moments_and_receipt_binding() {
    let mut events = events();
    for (event, label) in events.iter_mut().zip(["db-12", "db-8", "other", "db-A"]) {
        event.text = json!({"label": label}).to_string();
    }
    let mapped = EventQuery::MapText {
        input: Box::new(EventQuery::All),
        field: "label".to_string(),
        pointer: "/label".to_string(),
    };
    let EventQuery::Filter { predicate, .. } = query(leaf("label", r"^db-\d+$")) else {
        unreachable!()
    };
    let original = EventQuery::UpTo {
        input: Box::new(EventQuery::Filter {
            input: Box::new(mapped),
            predicate,
        }),
        moment_tick: 30,
    };
    let limits = EventQueryLimits::default();
    let result = evaluate(&original, &events, &limits).unwrap();
    let mut expected = selected(&events, &[1, 0]);
    for event in &mut expected {
        event.derived.insert(
            "label".to_string(),
            if event.source_sequence == 1 {
                "db-8"
            } else {
                "db-12"
            }
            .to_string(),
        );
    }
    assert_eq!(result.rows, expected);
    let receipt = receipt_for(&original, &events, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(30));
    validate_receipt(&receipt, &original, &events, &result, &limits).unwrap();
    let mut encoded = serde_json::to_value(&original).unwrap();
    encoded["UpTo"]["input"]["Filter"]["predicate"]["Regex"]["pattern"] = json!(r"^db-[0-9]+$");
    let changed = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        validate_receipt(&receipt, &changed, &events, &result, &limits),
        Err(EventQueryError::ReceiptDrift)
    );
    let mut changed = events.clone();
    changed[2].text = "{\"label\":\"unselected\"}".to_string();
    assert_eq!(
        validate_receipt(&receipt, &original, &changed, &result, &limits),
        Err(EventQueryError::SourceDrift)
    );
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("label".to_string(), "db-99".to_string());
    assert_eq!(
        validate_receipt(&receipt, &original, &events, &forged, &limits),
        Err(EventQueryError::ResultDrift)
    );
}

#[test]
fn invalid_syntax_unsupported_constructs_expansion_and_nesting_are_admitted_even_when_skipped() {
    let events = events();
    let limits = EventQueryLimits::default();
    for pattern in [
        "[".to_string(),
        r"(a)\1".to_string(),
        r"(?=a)".to_string(),
        "(?:a{1000}){1000}".to_string(),
        format!("{}a{}", "(".repeat(100), ")".repeat(100)),
    ] {
        let invalid = query(json!({"Any": [{"Process": "worker-1"}, leaf("text", &pattern)]}));
        assert_eq!(
            evaluate(&invalid, &events, &limits),
            Err(EventQueryError::InvalidRegex),
            "{pattern}"
        );
        assert_eq!(
            evaluate(&invalid, &[], &limits),
            Err(EventQueryError::InvalidRegex),
            "{pattern}"
        );
    }
}

#[test]
fn regex_field_and_pattern_bytes_keep_existing_admission_rules() {
    let mut events = events();
    let field = "f".repeat(MAX_FIELD_BYTES);
    events[0].derived.insert(field.clone(), "a".to_string());
    let pattern = format!("(?x){}a", " ".repeat(MAX_EVENT_TEXT_BYTES - 5));
    let limits = EventQueryLimits::default();
    assert_eq!(
        evaluate(&query(leaf(&field, &pattern)), &events, &limits)
            .unwrap()
            .rows,
        selected(&events, &[0])
    );
    for (field, pattern, error) in [
        (
            String::new(),
            "a".to_string(),
            EventQueryError::InvalidField,
        ),
        (
            format!("{field}x"),
            "a".to_string(),
            EventQueryError::InvalidField,
        ),
        (
            "bad\n".to_string(),
            "a".to_string(),
            EventQueryError::InvalidField,
        ),
        (
            "text".to_string(),
            String::new(),
            EventQueryError::InvalidText,
        ),
        (
            "text".to_string(),
            "a\n".to_string(),
            EventQueryError::InvalidText,
        ),
        (
            "text".to_string(),
            format!("{pattern} "),
            EventQueryError::InvalidText,
        ),
    ] {
        let invalid = query(json!({"All": [{"Process": "absent"}, leaf(&field, &pattern)]}));
        assert_eq!(evaluate(&invalid, &events, &limits), Err(error));
        assert_eq!(evaluate(&invalid, &[], &limits), Err(error));
    }
}

#[test]
fn regex_compounds_share_operation_and_structural_limits_across_query_branches() {
    let events = events();
    let original = query(json!({"All": [leaf("text", "ERR"), leaf("process", r"^worker-\d+$")]}));
    let exact = EventQueryLimits {
        max_operations: 14,
        ..EventQueryLimits::default()
    };
    assert_eq!(
        evaluate(&original, &events, &exact).unwrap().rows,
        selected(&events, &[1, 0])
    );
    assert_eq!(
        evaluate(
            &original,
            &events,
            &EventQueryLimits {
                max_operations: 13,
                ..exact
            }
        ),
        Err(EventQueryError::OperationBound)
    );
    assert_eq!(
        evaluate(
            &original,
            &events,
            &EventQueryLimits {
                max_nodes: 2,
                ..exact
            }
        ),
        Err(EventQueryError::QueryBound)
    );
    let nested = query(json!({"Not": {"Not": leaf("text", "ERR")}}));
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
    let union = EventQuery::Union {
        inputs: vec![query(leaf("label", r"\d+")), query(leaf("process", r"\d+"))],
    };
    assert_eq!(
        evaluate(&union, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        selected(&events, &[1, 0])
    );
}
