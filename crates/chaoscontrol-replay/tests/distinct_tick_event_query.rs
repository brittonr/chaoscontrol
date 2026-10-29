use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_DERIVED_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn events() -> Vec<EventEnvelope> {
    [
        (1, 0, 20, "preferred"),
        (2, 0, 10, "raw"),
        (1, 1, 10, "preferred"),
        (2, 1, 20, "raw"),
        (1, 2, 0, "raw"),
        (2, 2, u64::MAX, "raw"),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (source, sequence, tick, kind))| EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [source; 32],
        source_kind: EventSourceKind::Serial,
        source_sequence: sequence,
        tick,
        vm_index: Some(u32::from(source - 1)),
        process: Some("monitor".to_string()),
        thread: None,
        kind: kind.to_string(),
        text: json!({"label":format!("L{i}")}).to_string(),
        payload_digest: [3; 32],
        details: Some(json!({"label":format!("L{i}")})),
        derived: BTreeMap::from([("kept".to_string(), "original".to_string())]),
    })
    .collect()
}

fn branch(kind: &str) -> Value {
    if kind == "all" {
        json!("All")
    } else {
        json!({"Filter":{"input":"All","predicate":{"Kind":kind}}})
    }
}

fn query(inputs: Vec<Value>) -> EventQuery {
    serde_json::from_value(json!({"DistinctByTick":{"inputs":inputs}}))
        .expect("distinct tick queries must be supported")
}

fn at(input: Value, tick: u64) -> Value {
    json!({"UpTo":{"input":input,"moment_tick":tick}})
}

// r[verify chaoscontrol.distinct_tick_query.precedence]
#[test]
fn first_encountered_tick_winners_match_independent_linear_oracle() {
    let mut events = events();
    for _ in 0..2 {
        for kinds in [
            vec!["all"],
            vec!["absent"],
            vec!["preferred", "all"],
            vec!["raw", "preferred"],
            vec!["all", "preferred"],
            vec!["all", "all"],
        ] {
            let mut expected: Vec<EventEnvelope> = Vec::new();
            for kind in &kinds {
                for row in events
                    .iter()
                    .filter(|row| *kind == "all" || row.kind == *kind)
                {
                    if !expected.iter().any(|kept| kept.tick == row.tick) {
                        expected.push(row.clone());
                    }
                }
            }
            expected.sort_by_key(|row| (row.tick, row.source_id, row.source_sequence));
            let original = query(kinds.iter().map(|kind| branch(kind)).collect());
            assert_eq!(
                evaluate(&original, &events, &EventQueryLimits::default())
                    .unwrap()
                    .rows,
                expected,
                "{kinds:?}"
            );
        }
        events.reverse();
    }
}

#[test]
fn winning_projections_remain_visible_to_downstream_queries() {
    let events = events();
    let mapped = json!({"Map":{"input":branch("preferred"),"field":"chosen","pointer":"/label"}});
    let original = query(vec![mapped, json!("All")]);
    let mut expected = vec![
        events[4].clone(),
        events[2].clone(),
        events[0].clone(),
        events[5].clone(),
    ];
    expected[1]
        .derived
        .insert("chosen".to_string(), "L2".to_string());
    expected[2]
        .derived
        .insert("chosen".to_string(), "L0".to_string());
    assert_eq!(
        evaluate(&original, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        expected
    );
    let selected = EventQuery::Matches {
        input: Box::new(original),
        field: "chosen".to_string(),
        text: "L2".to_string(),
    };
    assert_eq!(
        evaluate(&selected, &events, &EventQueryLimits::default())
            .unwrap()
            .rows,
        vec![expected[1].clone()]
    );
}

#[test]
fn discarded_branches_still_parse_and_admit_projected_rows() {
    let limits = EventQueryLimits::default();
    assert_eq!(
        evaluate(&query(vec![]), &[], &limits),
        Err(EventQueryError::QueryBound)
    );
    let invalid =
        json!({"Filter":{"input":"All","predicate":{"Regex":{"field":"kind","pattern":"["}}}});
    assert_eq!(
        evaluate(&query(vec![json!("All"), invalid]), &[], &limits),
        Err(EventQueryError::InvalidRegex)
    );
    let mut malformed = events();
    malformed[5].text = "plain log".to_string();
    let parsed = json!({"MapText":{"input":"All","field":"label","pointer":"/label"}});
    assert_eq!(
        evaluate(&query(vec![json!("All"), parsed]), &malformed, &limits),
        Err(EventQueryError::InvalidJsonText)
    );
    let mut large = events();
    large[5].details = Some(json!({"big":"x".repeat(MAX_DERIVED_BYTES/2)}));
    let first = json!({"Map":{"input":"All","field":"first","pointer":"/big"}});
    let second = json!({"Map":{"input":first,"field":"second","pointer":"/big"}});
    assert_eq!(
        evaluate(&query(vec![json!("All"), second]), &large, &limits),
        Err(EventQueryError::InvalidEvent)
    );
}

#[test]
fn distinct_rows_preserve_shared_work_structural_and_result_limits() {
    let events = events();
    let original = query(vec![json!("All"), json!("All")]);
    let exact = EventQueryLimits {
        max_nodes: 3,
        max_depth: 1,
        max_operations: events.len() * 4,
        ..EventQueryLimits::default()
    };
    let expected = vec![
        events[4].clone(),
        events[1].clone(),
        events[0].clone(),
        events[5].clone(),
    ];
    assert_eq!(evaluate(&original, &events, &exact).unwrap().rows, expected);
    for (limits, error) in [
        (
            EventQueryLimits {
                max_nodes: 2,
                ..exact
            },
            EventQueryError::QueryBound,
        ),
        (
            EventQueryLimits {
                max_operations: exact.max_operations - 1,
                ..exact
            },
            EventQueryError::OperationBound,
        ),
        (
            EventQueryLimits {
                max_result_bytes: 1,
                ..exact
            },
            EventQueryError::ResultBound,
        ),
    ] {
        assert_eq!(evaluate(&original, &events, &limits), Err(error));
    }
    let nested = query(vec![json!("All"), at(json!("All"), u64::MAX)]);
    assert_eq!(
        evaluate(&nested, &events, &exact),
        Err(EventQueryError::QueryDepth)
    );
    let capped = EventQueryLimits {
        max_result_rows: 1,
        ..exact
    };
    let result = evaluate(&original, &events, &capped).unwrap();
    assert_eq!(result.rows, vec![events[4].clone()]);
    assert_eq!(result.matched, 4);
    assert!(result.truncated);
    let receipt = receipt_for(&original, &events, &result, &capped).unwrap();
    validate_receipt(&receipt, &original, &events, &result, &capped).unwrap();
}

#[test]
fn tick_receipts_bind_priority_expressions_discarded_sources_and_moments() {
    let events = events();
    let limits = EventQueryLimits::default();
    let original = query(vec![at(branch("preferred"), 20), at(json!("All"), 20)]);
    let result = evaluate(&original, &events, &limits).unwrap();
    assert_eq!(
        result.rows,
        vec![events[4].clone(), events[2].clone(), events[0].clone()]
    );
    let receipt = receipt_for(&original, &events, &result, &limits).unwrap();
    assert_eq!(receipt.moment_tick, Some(20));
    validate_receipt(&receipt, &original, &events, &result, &limits).unwrap();
    let reordered = query(vec![at(json!("All"), 20), at(branch("preferred"), 20)]);
    assert_eq!(
        validate_receipt(&receipt, &reordered, &events, &result, &limits),
        Err(EventQueryError::ReceiptDrift)
    );
    let equivalent = json!({"Filter":{"input":"All","predicate":{"Matches":{"field":"kind","text":"preferred"}}}});
    let changed = query(vec![at(equivalent, 20), at(json!("All"), 20)]);
    assert_eq!(
        validate_receipt(&receipt, &changed, &events, &result, &limits),
        Err(EventQueryError::ReceiptDrift)
    );
    let mut changed = events.clone();
    changed[3].text = "changed discarded source".to_string();
    assert_eq!(
        validate_receipt(&receipt, &original, &changed, &result, &limits),
        Err(EventQueryError::SourceDrift)
    );
    let mut forged = result.clone();
    forged.rows[0]
        .derived
        .insert("kept".to_string(), "forged".to_string());
    assert_eq!(
        validate_receipt(&receipt, &original, &events, &forged, &limits),
        Err(EventQueryError::ResultDrift)
    );
    let mixed = query(vec![
        at(branch("preferred"), 20),
        at(json!("All"), u64::MAX),
    ]);
    assert_eq!(
        evaluate(&mixed, &events, &limits),
        Err(EventQueryError::AmbiguousMoment)
    );
    let outer: EventQuery =
        serde_json::from_value(at(serde_json::to_value(mixed).unwrap(), 10)).unwrap();
    let result = evaluate(&outer, &events, &limits).unwrap();
    assert_eq!(result.rows, vec![events[4].clone(), events[2].clone()]);
    assert_eq!(
        receipt_for(&outer, &events, &result, &limits)
            .unwrap()
            .moment_tick,
        Some(10)
    );
}
