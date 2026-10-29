use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
    MAX_DERIVED_FIELDS, MAX_FIELD_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn event(source: u8, sequence: u64, tick: u64, severity: u64) -> EventEnvelope {
    EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [source; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick,
        vm_index: Some(u32::from(source)),
        process: Some("monitor".to_string()),
        thread: Some(7),
        kind: "log".to_string(),
        text: format!("{{\"severity\":{severity}}}"),
        payload_digest: [9; 32],
        details: Some(json!({"severity": severity})),
        derived: BTreeMap::from([("context".to_string(), "preserved".to_string())]),
    }
}

fn count(input: Value, predicate: Value, field: &str) -> EventQuery {
    serde_json::from_value(
        json!({"RunningCount": {"input": input, "predicate": predicate, "field": field}}),
    )
    .expect("bounded running counts must be supported")
}

fn severity(value: u64) -> Value {
    json!({"JsonNumber": {"pointer": "/severity", "comparison": "Equal", "value": value}})
}

// r[verify chaoscontrol.running_count.prefixes]
#[test]
fn canonical_prefix_counts_match_independent_oracle_across_input_permutations() {
    let events = vec![
        event(2, 0, 20, 20),
        event(1, 0, 10, 30),
        event(1, 1, 20, 20),
        event(2, 1, 20, 30),
        event(1, 2, 0, 30),
        event(2, 2, u64::MAX, 20),
    ];
    let mut ordered = events.clone();
    ordered.sort_by_key(|row| (row.tick, row.source_id, row.source_sequence));
    let expected: Vec<_> = ordered
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let total = ordered[..=index]
                .iter()
                .filter(|prior| prior.details.as_ref().unwrap()["severity"] == 20)
                .count();
            let mut row = row.clone();
            row.derived.insert("seen".to_string(), total.to_string());
            row
        })
        .collect();
    for reverse in [false, true] {
        for rotation in 0..events.len() {
            let mut supplied = events.clone();
            if reverse {
                supplied.reverse();
            }
            supplied.rotate_left(rotation);
            assert_eq!(
                evaluate(
                    &count(json!("All"), severity(20), "seen"),
                    &supplied,
                    &EventQueryLimits::default()
                )
                .unwrap()
                .rows,
                expected
            );
        }
    }
    for (predicate, totals) in [
        (json!({"All": []}), vec!["1", "2", "3", "4", "5", "6"]),
        (json!({"Any": []}), vec!["0"; 6]),
    ] {
        let mut expected = ordered.clone();
        for (row, total) in expected.iter_mut().zip(totals) {
            row.derived.insert("seen".to_string(), total.to_string());
        }
        assert_eq!(
            evaluate(
                &count(json!("All"), predicate, "seen"),
                &events,
                &EventQueryLimits::default()
            )
            .unwrap()
            .rows,
            expected
        );
    }
    assert_eq!(
        evaluate(
            &count(json!("All"), severity(20), "seen"),
            &[],
            &EventQueryLimits::default()
        )
        .unwrap()
        .rows,
        vec![]
    );
}

#[test]
fn projected_predicates_and_downstream_selection_count_before_overwrite() {
    let events = [
        event(1, 0, 30, 30),
        event(1, 1, 10, 20),
        event(1, 2, 20, 20),
    ];
    let projected = json!({"MapText": {"input": "All", "field": "level", "pointer": "/severity"}});
    let query = count(
        projected,
        json!({"All": [{"Regex": {"field": "level", "pattern": "^20$"}}, {"Vm": 1}]}),
        "seen",
    );
    let selected = EventQuery::Filter {
        input: Box::new(query),
        predicate: serde_json::from_value(severity(30)).unwrap(),
    };
    let mut expected = events[0].clone();
    expected
        .derived
        .insert("level".to_string(), "30".to_string());
    expected.derived.insert("seen".to_string(), "2".to_string());
    assert_eq!(
        evaluate(&selected, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        vec![expected]
    );
    let mut supplied = events.to_vec();
    for row in &mut supplied {
        row.derived.insert(
            "seen".to_string(),
            if row.tick == 20 { "no" } else { "yes" }.to_string(),
        );
    }
    let query = count(
        json!("All"),
        json!({"Matches": {"field": "seen", "text": "yes"}}),
        "seen",
    );
    let actual = evaluate(&query, &supplied, &EventQueryLimits::default()).unwrap();
    assert_eq!(
        actual
            .rows
            .iter()
            .map(|row| row.derived["seen"].as_str())
            .collect::<Vec<_>>(),
        vec!["1", "1", "2"]
    );
    let narrowed = EventQuery::Narrow {
        input: Box::new(query),
        fields: vec!["seen".to_string()],
    };
    let mut expected = actual.rows;
    for row in &mut expected {
        row.derived.remove("context");
    }
    assert_eq!(
        evaluate(&narrowed, &supplied, &EventQueryLimits::default())
            .unwrap()
            .rows,
        expected
    );
}

#[test]
fn duplicate_positions_count_once_using_first_projection() {
    let mut first = event(1, 0, 10, 20);
    first.derived.insert("tag".to_string(), "skip".to_string());
    let mut duplicate = first.clone();
    duplicate
        .derived
        .insert("tag".to_string(), "match".to_string());
    let mut next = event(1, 1, 20, 20);
    next.derived.insert("tag".to_string(), "match".to_string());
    let query = count(
        json!("All"),
        json!({"Matches": {"field": "tag", "text": "match"}}),
        "seen",
    );
    let mut expected = vec![first.clone(), next.clone()];
    expected[0]
        .derived
        .insert("seen".to_string(), "0".to_string());
    expected[1]
        .derived
        .insert("seen".to_string(), "1".to_string());
    let result = evaluate(
        &query,
        &[first, next, duplicate],
        &EventQueryLimits::default(),
    )
    .unwrap();
    assert_eq!(result.rows, expected);
    assert_eq!((result.scanned, result.matched), (3, 2));
}

#[test]
fn count_admission_and_generated_bounds_cannot_be_hidden_by_outer_queries() {
    let limits = EventQueryLimits::default();
    for field in [
        String::new(),
        "a\nb".to_string(),
        "é".repeat(MAX_FIELD_BYTES / 2) + "x",
    ] {
        assert_eq!(
            evaluate(&count(json!("All"), severity(20), &field), &[], &limits),
            Err(EventQueryError::InvalidField)
        );
    }
    for (predicate, error) in [
        (
            json!({"Regex": {"field": "kind", "pattern": "["}}),
            EventQueryError::InvalidRegex,
        ),
        (
            json!({"Any": [{"All": []}, {"JsonTextPresent": {"pointer": "bad"}}]}),
            EventQueryError::InvalidPointer,
        ),
    ] {
        assert_eq!(
            evaluate(&count(json!("All"), predicate, "seen"), &[], &limits),
            Err(error)
        );
    }
    let source = event(1, 0, 10, 20);
    let field = "é".repeat(MAX_FIELD_BYTES / 2);
    let mut expected = source.clone();
    expected.derived.insert(field.clone(), "1".to_string());
    assert_eq!(
        evaluate(
            &count(json!("All"), severity(20), &field),
            std::slice::from_ref(&source),
            &limits
        )
        .unwrap()
        .rows,
        vec![expected]
    );
    let mut full = source.clone();
    full.derived = (0..MAX_DERIVED_FIELDS)
        .map(|i| (format!("f{i}"), String::new()))
        .collect();
    let mut expected = full.clone();
    expected.derived.insert("f0".to_string(), "1".to_string());
    assert_eq!(
        evaluate(
            &count(json!("All"), severity(20), "f0"),
            &[full.clone()],
            &limits
        )
        .unwrap()
        .rows,
        vec![expected]
    );
    let hidden = |query| EventQuery::Narrow {
        input: Box::new(query),
        fields: vec![],
    };
    assert_eq!(
        evaluate(
            &hidden(count(json!("All"), severity(20), "new")),
            &[full],
            &limits
        ),
        Err(EventQueryError::InvalidEvent)
    );
    let mut large = source.clone();
    large.derived = BTreeMap::from([(
        "large".to_string(),
        "x".repeat(MAX_DERIVED_BYTES - "large".len()),
    )]);
    assert_eq!(
        evaluate(
            &hidden(count(json!("All"), severity(20), "new")),
            &[large.clone()],
            &limits
        ),
        Err(EventQueryError::InvalidEvent)
    );
    let overflow = json!({"Map": {"input": "All", "field": "extra", "pointer": "/severity"}});
    assert_eq!(
        evaluate(&count(overflow, severity(20), "large"), &[large], &limits),
        Err(EventQueryError::InvalidEvent)
    );
    let mut malformed = source;
    malformed.text = "not JSON".to_string();
    let text = json!({"JsonTextPresent": {"pointer": "/severity"}});
    assert_eq!(
        evaluate(
            &count(json!("All"), text.clone(), "seen"),
            &[malformed.clone()],
            &limits
        ),
        Err(EventQueryError::InvalidJsonText)
    );
    let skipped = count(json!("All"), json!({"Any": [{"All": []}, text]}), "seen");
    let mut expected = malformed.clone();
    expected.derived.insert("seen".to_string(), "1".to_string());
    assert_eq!(
        evaluate(&skipped, &[malformed], &limits).unwrap().rows,
        vec![expected]
    );
}

#[test]
fn running_counts_share_predicate_work_structure_and_result_limits() {
    let events = [event(1, 0, 20, 20), event(1, 1, 10, 30)];
    let predicate =
        json!({"All": [{"SourceKind": "serial"}, {"JsonPresent": {"pointer": "/severity"}}]});
    let query = count(json!("All"), predicate, "seen");
    let exact = EventQueryLimits {
        max_nodes: 4,
        max_depth: 1,
        max_operations: 8,
        ..EventQueryLimits::default()
    };
    let result = evaluate(&query, &events, &exact).unwrap();
    assert_eq!(
        result
            .rows
            .iter()
            .map(|row| row.derived["seen"].as_str())
            .collect::<Vec<_>>(),
        vec!["1", "2"]
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
    for (limits, error) in [
        (
            EventQueryLimits {
                max_operations: 7,
                ..exact
            },
            EventQueryError::OperationBound,
        ),
        (
            EventQueryLimits {
                max_nodes: 3,
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
    let wrapped = EventQuery::UpTo {
        input: Box::new(query.clone()),
        moment_tick: 20,
    };
    assert_eq!(
        evaluate(
            &wrapped,
            &events,
            &EventQueryLimits {
                max_nodes: 5,
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
    assert_eq!(prefix.rows, result.rows[..1]);
    assert_eq!((prefix.matched, prefix.truncated), (2, true));
}

#[test]
fn running_count_moments_and_receipts_bind_complete_expressions_and_sources() {
    let events = [
        event(1, 0, 10, 20),
        event(1, 1, 20, 30),
        event(1, 2, 30, 20),
    ];
    let limits = EventQueryLimits::default();
    let input = json!({"UpTo": {"input": "All", "moment_tick": 20}});
    let query = count(input.clone(), severity(20), "seen");
    let result = evaluate(&query, &events, &limits).unwrap();
    assert_eq!(
        result
            .rows
            .iter()
            .map(|row| row.derived["seen"].as_str())
            .collect::<Vec<_>>(),
        vec!["1", "1"]
    );
    let receipt = receipt_for(&query, &events, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(20));
    validate_receipt(&receipt, &query, &events, &result, &limits).unwrap();
    for changed in [
        count(input.clone(), json!({"All": [severity(20)]}), "seen"),
        count(input.clone(), severity(20), "other"),
        count(input, severity(30), "seen"),
    ] {
        assert!(validate_receipt(&receipt, &changed, &events, &result, &limits).is_err());
    }
    let mut changed = events.clone();
    changed[2].text = "changed outside moment".to_string();
    assert_eq!(evaluate(&query, &changed, &limits).unwrap(), result);
    assert!(validate_receipt(&receipt, &query, &changed, &result, &limits).is_err());
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("seen".to_string(), "7".to_string());
    assert!(validate_receipt(&receipt, &query, &events, &forged, &limits).is_err());
    let mixed: EventQuery = serde_json::from_value(
        json!({"Union": {"inputs": [query, {"UpTo": {"input": "All", "moment_tick": 30}}]}}),
    )
    .unwrap();
    assert_eq!(
        evaluate(&mixed, &events, &limits),
        Err(EventQueryError::AmbiguousMoment)
    );
    let outer = EventQuery::UpTo {
        input: Box::new(mixed),
        moment_tick: 20,
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
        Some(20)
    );
}
