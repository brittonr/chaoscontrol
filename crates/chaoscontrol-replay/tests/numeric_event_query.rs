use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn query(comparison: &str, value: Value) -> EventQuery {
    serde_json::from_value(json!({"Filter": {
        "input": "All",
        "predicate": {"JsonNumber": {
            "pointer": "/memory/used", "comparison": comparison, "value": value
        }}
    }}))
    .expect("numeric JSON predicates must be supported")
}

fn observations() -> Vec<EventEnvelope> {
    let values = [
        json!(-3),
        json!(-2.5),
        json!(0),
        json!(50),
        json!(50.25),
        json!(100),
        json!("100"),
        json!(true),
        Value::Null,
        json!([]),
        json!({}),
    ];
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| EventEnvelope {
            schema: EVENT_ENVELOPE_SCHEMA.to_string(),
            source_id: [1; 32],
            source_kind: EventSourceKind::Recording,
            source_sequence: index as u64,
            tick: index as u64,
            vm_index: Some(0),
            process: Some("memory-monitor".to_string()),
            thread: None,
            kind: "memory".to_string(),
            text: "memory observation".to_string(),
            payload_digest: [1; 32],
            details: Some(json!({"memory": {"used": value}})),
            derived: BTreeMap::new(),
        })
        .collect()
}

// r[verify chaoscontrol.numeric_event_query.comparison]
#[test]
fn numeric_thresholds_compare_numbers_without_coercing_other_json() {
    let events = observations();
    let limits = EventQueryLimits::default();
    for (comparison, indexes) in [
        ("LessThan", vec![0]),
        ("LessThanOrEqual", vec![0, 1]),
        ("Equal", vec![1]),
        ("GreaterThanOrEqual", vec![1, 2, 3, 4, 5]),
        ("GreaterThan", vec![2, 3, 4, 5]),
    ] {
        let result = evaluate(&query(comparison, json!(-2.5)), &events, &limits).unwrap();
        let expected: Vec<_> = indexes.into_iter().map(|i| events[i].clone()).collect();
        assert_eq!(result.rows, expected, "{comparison}");
    }
    let mut events = events;
    for details in [None, Some(json!({})), Some(json!({"memory": null}))] {
        let mut event = events[0].clone();
        event.source_sequence = events.len() as u64;
        event.tick = event.source_sequence;
        event.details = details;
        events.push(event);
    }
    let result = evaluate(&query("GreaterThan", json!(50)), &events, &limits).unwrap();
    assert_eq!(result.rows, vec![events[4].clone(), events[5].clone()]);
}

#[test]
fn escaped_pointers_and_binary64_boundaries_are_explicit() {
    let mut events = observations();
    events.truncate(1);
    events[0].details = Some(json!({"memory/usage": {"~used": -0.0}}));
    let mut query = query("Equal", json!(0));
    let EventQuery::Filter { predicate, .. } = &mut query else {
        unreachable!()
    };
    let mut encoded = serde_json::to_value(&*predicate).unwrap();
    encoded["JsonNumber"]["pointer"] = json!("/memory~1usage/~0used");
    *predicate = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        evaluate(&query, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        events
    );

    events[0].details = Some(json!({"memory/usage": {"~used": 9_007_199_254_740_993_u64}}));
    let mut encoded = serde_json::to_value(&query).unwrap();
    encoded["Filter"]["predicate"]["JsonNumber"]["value"] = json!(9_007_199_254_740_992_u64);
    let rounded_query = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        evaluate(&rounded_query, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        events
    );
}

#[test]
fn numeric_queries_reject_invalid_pointers_types_and_excess_operations() {
    let events = observations();
    let query = query("GreaterThan", json!(50));
    for pointer in [
        String::new(),
        "memory/used".to_string(),
        "/bad\n".to_string(),
        format!("/{}", "x".repeat(MAX_FIELD_BYTES)),
    ] {
        let mut encoded = serde_json::to_value(&query).unwrap();
        encoded["Filter"]["predicate"]["JsonNumber"]["pointer"] = json!(pointer);
        let invalid = serde_json::from_value(encoded).unwrap();
        assert_eq!(
            evaluate(&invalid, &events, &EventQueryLimits::default()),
            Err(EventQueryError::InvalidPointer)
        );
    }
    for value in [Value::Null, json!("50"), json!(true), json!([])] {
        let mut encoded = serde_json::to_value(&query).unwrap();
        encoded["Filter"]["predicate"]["JsonNumber"]["value"] = value;
        assert!(serde_json::from_value::<EventQuery>(encoded).is_err());
    }
    let mut encoded = serde_json::to_value(&query).unwrap();
    encoded["Filter"]["predicate"]["JsonNumber"]["comparison"] = json!("Approximately");
    assert!(serde_json::from_value::<EventQuery>(encoded).is_err());
    let exact = EventQueryLimits {
        max_operations: events.len() * 2,
        ..EventQueryLimits::default()
    };
    assert_eq!(
        evaluate(&query, &events, &exact).unwrap().rows,
        vec![events[4].clone(), events[5].clone()]
    );
    let insufficient = EventQueryLimits {
        max_operations: exact.max_operations - 1,
        ..exact
    };
    assert_eq!(
        evaluate(&query, &events, &insufficient),
        Err(EventQueryError::OperationBound)
    );
}

#[test]
fn numeric_receipts_bind_threshold_and_original_observations() {
    let events = observations();
    let limits = EventQueryLimits::default();
    let original = query("GreaterThan", json!(50));
    let result = evaluate(&original, &events, &limits).unwrap();
    let receipt = receipt_for(&original, &events, &result, &limits).unwrap();
    validate_receipt(&receipt, &original, &events, &result, &limits).unwrap();
    // This different threshold selects the same rows; query identity still matters.
    let changed = query("GreaterThan", json!(50.1));
    assert_eq!(
        validate_receipt(&receipt, &changed, &events, &result, &limits),
        Err(EventQueryError::ReceiptDrift)
    );
    let mut changed_result = result.clone();
    changed_result.rows[0].details = Some(json!({"memory": {"used": 99}}));
    assert_eq!(
        validate_receipt(&receipt, &original, &events, &changed_result, &limits),
        Err(EventQueryError::ResultDrift)
    );
}
