use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, validate_receipt, EventEnvelope, EventQuery, EventQueryError,
    EventQueryLimits, EventSourceKind, EVENT_ENVELOPE_SCHEMA, MAX_QUERY_DEPTH, MAX_QUERY_NODES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn filter(predicate: Value) -> EventQuery {
    serde_json::from_value(json!({"Filter": {"input": "All", "predicate": predicate}}))
        .expect("compound predicates must be supported")
}

fn high_memory() -> Value {
    json!({"JsonNumber": {"pointer": "/used", "comparison": "GreaterThan", "value": 50}})
}

fn operational_predicate() -> Value {
    json!({"All": [
        {"Any": [{"Vm": 1}, {"Vm": 2}]},
        high_memory(),
        {"Not": {"Process": "maintenance"}}
    ]})
}

fn observations() -> Vec<EventEnvelope> {
    [
        (30, 1, Some("worker"), json!(80)),
        (10, 2, Some("maintenance"), json!(90)),
        (20, 2, Some("worker"), json!(50)),
        (40, 3, Some("worker"), json!(95)),
        (50, 2, Some("worker"), json!(70)),
        (60, 1, None, json!("100")),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (tick, vm, process, value))| EventEnvelope {
        schema: EVENT_ENVELOPE_SCHEMA.to_string(),
        source_id: [1; 32],
        source_kind: EventSourceKind::Recording,
        source_sequence: index as u64,
        tick,
        vm_index: Some(vm),
        process: process.map(str::to_string),
        thread: None,
        kind: "memory".to_string(),
        text: "memory observation".to_string(),
        payload_digest: [1; 32],
        details: Some(json!({"used": value})),
        derived: BTreeMap::new(),
    })
    .collect()
}

// r[verify chaoscontrol.compound_event_query.composition]
#[test]
fn compound_selection_preserves_canonical_observations() {
    let events = observations();
    let result = evaluate(
        &filter(operational_predicate()),
        &events,
        &EventQueryLimits::default(),
    )
    .unwrap();
    assert_eq!(result.rows, vec![events[0].clone(), events[4].clone()]);
    assert_eq!(result.matched, 2);
}

#[test]
fn identities_negation_and_nested_logic_have_explicit_truth_values() {
    let events = observations();
    let limits = EventQueryLimits::default();
    for (predicate, indexes) in [
        (json!({"All": []}), vec![1, 2, 0, 3, 4, 5]),
        (json!({"Any": []}), vec![]),
        (json!({"Not": high_memory()}), vec![2, 5]),
        (json!({"Not": {"Not": high_memory()}}), vec![1, 0, 3, 4]),
        (json!({"Not": {"Process": "worker"}}), vec![1, 5]),
        (
            json!({"All": [{"Vm": 1}, {"Any": [{"Kind": "memory"}, {"Vm": 2}]}]}),
            vec![0, 5],
        ),
    ] {
        let expected: Vec<_> = indexes.into_iter().map(|i| events[i].clone()).collect();
        assert_eq!(
            evaluate(&filter(predicate), &events, &limits).unwrap().rows,
            expected
        );
    }
    // Negation inverts the leaf's false result; it does not add a numeric type guard.
    let mut missing = events[5].clone();
    missing.details = None;
    assert_eq!(
        evaluate(
            &filter(json!({"Not": high_memory()})),
            &[missing.clone()],
            &limits
        )
        .unwrap()
        .rows,
        vec![missing]
    );
}

#[test]
fn skipped_and_empty_source_branches_still_require_valid_predicates() {
    let events = observations();
    for source in [events.as_slice(), &[]] {
        for (predicate, error) in [
            (
                json!({"Any": [{"All": []}, {"Kind": ""}]}),
                EventQueryError::InvalidPredicate,
            ),
            (
                json!({"All": [{"Any": []}, {"JsonNumber": {"pointer": "bad", "comparison": "Equal", "value": 1}}]}),
                EventQueryError::InvalidPointer,
            ),
            (
                json!({"Not": {"TickRange": {"start_tick": 2, "end_tick": 1}}}),
                EventQueryError::InvalidPredicate,
            ),
        ] {
            assert_eq!(
                evaluate(&filter(predicate), source, &EventQueryLimits::default()),
                Err(error)
            );
        }
    }
}

#[test]
fn predicate_descendants_share_query_node_and_depth_bounds() {
    let events = observations();
    let query = filter(json!({"Not": {"Not": {"Vm": 1}}}));
    let exact = EventQueryLimits {
        max_nodes: 4,
        max_depth: 2,
        ..EventQueryLimits::default()
    };
    let expected = vec![events[0].clone(), events[5].clone()];
    assert_eq!(evaluate(&query, &events, &exact).unwrap().rows, expected);
    assert_eq!(
        evaluate(
            &query,
            &events,
            &EventQueryLimits {
                max_nodes: 3,
                ..exact
            }
        ),
        Err(EventQueryError::QueryBound)
    );
    assert_eq!(
        evaluate(
            &query,
            &events,
            &EventQueryLimits {
                max_depth: 1,
                ..exact
            }
        ),
        Err(EventQueryError::QueryDepth)
    );
    let wrapped = EventQuery::UpTo {
        input: Box::new(query),
        moment_tick: 100,
    };
    assert_eq!(
        evaluate(
            &wrapped,
            &events,
            &EventQueryLimits {
                max_nodes: 5,
                max_depth: 3,
                ..exact
            }
        )
        .unwrap()
        .rows,
        expected
    );
    assert_eq!(
        evaluate(
            &wrapped,
            &events,
            &EventQueryLimits {
                max_nodes: 4,
                max_depth: 3,
                ..exact
            }
        ),
        Err(EventQueryError::QueryBound)
    );
    assert_eq!(
        evaluate(
            &wrapped,
            &events,
            &EventQueryLimits {
                max_nodes: 5,
                max_depth: 2,
                ..exact
            }
        ),
        Err(EventQueryError::QueryDepth)
    );

    let defaults = EventQueryLimits::default();
    let wide = filter(json!({"All": vec![json!({"Vm": 1}); MAX_QUERY_NODES - 2]}));
    assert_eq!(evaluate(&wide, &events, &defaults).unwrap().rows, expected);
    let over =
        filter(json!({"Any": [{"All": []}, {"All": vec![json!({"Vm": 1}); MAX_QUERY_NODES]}]}));
    assert_eq!(
        evaluate(&over, &[], &defaults),
        Err(EventQueryError::QueryBound)
    );
    let mut deep = json!({"Vm": 1});
    for _ in 0..MAX_QUERY_DEPTH {
        deep = json!({"Not": deep});
    }
    assert_eq!(
        evaluate(&filter(deep.clone()), &events, &defaults)
            .unwrap()
            .rows,
        expected
    );
    assert_eq!(
        evaluate(&filter(json!({"Not": deep})), &[], &defaults),
        Err(EventQueryError::QueryDepth)
    );
}

#[test]
fn operation_costs_preserve_flat_filters_and_charge_only_visited_children() {
    let events = observations();
    let flat_cost = events.len() * 2; // source scan plus one filter-root check per row
    let flat = EventQueryLimits {
        max_operations: flat_cost,
        ..EventQueryLimits::default()
    };
    let expected = vec![events[0].clone(), events[5].clone()];
    assert_eq!(
        evaluate(&filter(json!({"Vm": 1})), &events, &flat)
            .unwrap()
            .rows,
        expected
    );
    assert_eq!(
        evaluate(
            &filter(json!({"Vm": 1})),
            &events,
            &EventQueryLimits {
                max_operations: flat_cost - 1,
                ..flat
            }
        ),
        Err(EventQueryError::OperationBound)
    );
    let budget = EventQueryLimits {
        max_operations: flat_cost + events.len(),
        ..flat
    };
    let deep = json!({"Not": {"Not": {"Vm": 1}}});
    for (predicate, expected) in [
        (
            json!({"Any": [{"All": []}, deep.clone()]}),
            vec![1, 2, 0, 3, 4, 5],
        ),
        (json!({"All": [{"Any": []}, deep.clone()]}), vec![]),
    ] {
        let expected: Vec<_> = expected.into_iter().map(|i| events[i].clone()).collect();
        let query = filter(predicate);
        assert_eq!(evaluate(&query, &events, &budget).unwrap().rows, expected);
        assert_eq!(
            evaluate(
                &query,
                &events,
                &EventQueryLimits {
                    max_operations: budget.max_operations - 1,
                    ..budget
                }
            ),
            Err(EventQueryError::OperationBound)
        );
    }
    // Reversing Any children must evaluate the deeper first branch, not reorder it.
    assert_eq!(
        evaluate(
            &filter(json!({"Any": [deep, {"All": []}]})),
            &events,
            &budget
        ),
        Err(EventQueryError::OperationBound)
    );
    assert_eq!(
        evaluate(&filter(json!({"Not": {"Vm": 1}})), &events, &budget)
            .unwrap()
            .rows,
        vec![
            events[1].clone(),
            events[2].clone(),
            events[3].clone(),
            events[4].clone()
        ]
    );
}

#[test]
fn receipts_bind_compound_expression_even_when_another_expression_selects_the_same_rows() {
    let events = observations();
    let limits = EventQueryLimits::default();
    let query = filter(operational_predicate());
    let result = evaluate(&query, &events, &limits).unwrap();
    let receipt = receipt_for(&query, &events, &result, &limits).unwrap();
    validate_receipt(&receipt, &query, &events, &result, &limits).unwrap();
    let equivalent = filter(json!({"All": [operational_predicate()]}));
    assert_eq!(evaluate(&equivalent, &events, &limits).unwrap(), result);
    assert_eq!(
        validate_receipt(&receipt, &equivalent, &events, &result, &limits),
        Err(EventQueryError::ReceiptDrift)
    );
    let mut changed = events;
    changed[2].text = "changed non-selected source event".to_string();
    assert_eq!(
        validate_receipt(&receipt, &query, &changed, &result, &limits),
        Err(EventQueryError::SourceDrift)
    );
}
