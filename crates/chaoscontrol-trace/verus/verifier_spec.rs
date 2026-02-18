// Verus specification for trace verifier pure functions
verus! {

/// Empty traces never diverge
proof fn empty_traces_no_divergence()
    ensures find_first_divergence(&[], &[]).is_none(),
{ }

/// Identical traces never diverge
proof fn identical_traces_no_divergence(events: &[TraceEvent])
    ensures find_first_divergence(events, events).is_none(),
{ }

/// Divergence index is always less than or equal to min length
proof fn divergence_index_bounded(a: &[TraceEvent], b: &[TraceEvent])
    ensures
        find_first_divergence(a, b).is_some() ==>
            find_first_divergence(a, b).unwrap().0 <= a.len().min(b.len()),
{ }

/// All events before divergence point are equal
proof fn prefix_before_divergence_equal(a: &[TraceEvent], b: &[TraceEvent])
    ensures
        find_first_divergence(a, b).is_some() ==>
            forall |i: usize| i < find_first_divergence(a, b).unwrap().0 ==>
                a[i].determinism_eq(&b[i]),
{ }

/// Divergence is symmetric (both orderings produce a result)
proof fn divergence_symmetric(a: &[TraceEvent], b: &[TraceEvent])
    ensures
        find_first_divergence(a, b).is_some() <==> find_first_divergence(b, a).is_some(),
{ }

/// Description is always non-empty
proof fn description_nonempty(a: &TraceEvent, b: &TraceEvent)
    ensures describe_divergence(a, b).len() > 0,
{ }

/// Type mismatch always mentioned in description
proof fn type_mismatch_described(a: &TraceEvent, b: &TraceEvent)
    requires a.event_type() != b.event_type(),
    ensures describe_divergence(a, b).contains("type mismatch"),
{ }

}
