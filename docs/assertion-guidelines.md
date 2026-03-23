# Assertion Guidelines for Guest Programs

How to place SDK assertions in ChaosControl guest programs. Derived from
Antithesis's assertion density patterns and our own exploration results.

## Assertion Types and When to Use Them

| Type | Purpose | Placement |
|------|---------|-----------|
| `always` | Invariant that must hold on every execution | Safety checks, data integrity, protocol rules |
| `sometimes` | Path that should be reached in at least one run | Branch coverage validation, liveness evidence |
| `reachable` | Code point that should be hit | Feature coverage, error handler exercise |
| `unreachable` | Code point that should never be hit | Dead code, impossible states, defensive checks |

## Density Targets

The raft guest currently has 6 `assert::` calls covering 3 safety invariants
and 3 liveness properties. That covers the big correctness properties but
leaves the explorer blind to whether it's actually exercising interesting
code paths inside the guest.

Target: assertions at every significant branch in the guest, not just at
top-level invariant checks.

## The `Sometimes` Pair Pattern

For any boolean condition the explorer should cover both sides of, assert
both the true and false cases:

```rust
let have_quorum = acks >= majority;
assert::sometimes(have_quorum, "quorum reached", &json!({}));
assert::sometimes(!have_quorum, "quorum not reached", &json!({}));
```

If the explorer never makes the `false` case true across thousands of runs,
the exploration strategy has a blind spot. This turns assertion failures into
exploration quality signals — a `sometimes` that never fires means the
explorer isn't reaching that part of the state space.

Use this at:
- Majority/quorum checks
- Timeout vs success paths
- Leader vs follower code paths
- Message delivery vs drop
- Any `if/else` that represents meaningfully different system behavior

## Placement Guide

### Every handler entry point

```rust
fn handle_append_entries(&mut self, msg: &AppendEntries) {
    assert::always(true, "append_entries handler called", &json!({"term": msg.term}));
    // ...
}
```

This confirms the explorer is generating traffic that reaches each handler.
A handler with zero `always(true)` hits means the workload never triggered it.

### Error paths

```rust
if entries_conflict {
    assert::reachable("log conflict detected", &json!({"index": conflict_idx}));
    self.truncate_log(conflict_idx);
} else {
    assert::reachable("log consistent", &json!({}));
}
```

### State transitions

```rust
match (old_role, new_role) {
    (Follower, Candidate) => {
        assert::reachable("follower started election", &json!({"term": self.term}));
    }
    (Candidate, Leader) => {
        assert::reachable("candidate won election", &json!({"term": self.term}));
    }
    (_, Follower) => {
        assert::reachable("stepped down to follower", &json!({"term": self.term}));
    }
    _ => {
        assert::unreachable("unexpected transition", &json!({
            "from": format!("{:?}", old_role),
            "to": format!("{:?}", new_role),
        }));
    }
}
```

### Data invariants inline

Don't just check invariants in a post-tick sweep. Assert them at the point
where the data changes:

```rust
// Right after modifying commit_index, not in a separate check function
self.commit_index = new_commit;
assert::always(
    self.commit_index <= self.log.len(),
    "commit_index within log bounds",
    &json!({"commit": self.commit_index, "log_len": self.log.len()}),
);
```

## What Not to Assert

- Don't assert on timing or performance (`always(latency < 10ms, ...)` —
  virtual time doesn't map to real time)
- Don't assert on values that depend on exploration seed
  (`always(leader_id == 0, ...)`)
- Don't use `always(true, ...)` as a logging mechanism — use
  `lifecycle::send_event` for pure telemetry

## Coverage Interaction

Assertions and `coverage::record_edge()` serve different purposes:

- **Coverage edges** guide the explorer toward new code paths (AFL-style
  bitmap feedback)
- **Assertions** check correctness properties once you're on a path

Both matter. A guest with good coverage instrumentation but no assertions
will explore widely but never find bugs. A guest with good assertions but
no coverage will check correctness but only on paths the explorer stumbles
into.

## Applying to Existing Guests

For the raft guest, the gaps are:
1. No handler-level reachability (`handle_request_vote` called, etc.)
2. No `sometimes` pairs on quorum/timeout branches
3. No state transition reachability
4. No inline data invariants at mutation sites
5. Safety invariants only checked in the post-tick sweep, not at the point
   of state change
