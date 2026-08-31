# SUT-declared event branching

A guest can declare a meaningful state as an exploration branch point. The
explorer can retain that state as a replay parent and give it priority.

## Guest API

Use `cc_branch_marker!` when the declaration must remain visible even if no run
reaches the call site:

```rust
use chaoscontrol_sdk::prelude::*;

let identity = cc_branch_marker!(
    "raft",
    "leader-elected",
    "node-1",
    "leader elected",
    &serde_json::json!({"term": 7}),
    Some("b3:canonical-state"),
    Some("log:term-7:index-42"),
)?;
```

The macro registers a stable reachable assertion in the guest catalog. It also
emits a structured branch marker when execution reaches the call site. The
namespace and key determine the marker identity. Details and instance refs do
not change that identity.

Use `branch_marker(...)` only when static declaration and coverage-gap reporting
are not required.

## Admission and limits

The VMM oracle validates each marker before use. It rejects malformed schemas,
invalid identities, oversized text, excessive detail depth, and excess refs.
Exact duplicates at the same state and logical position collapse to one marker.
A run admits at most 256 distinct marker observations. Further markers produce
the typed `branch_marker_limit` protocol event. This event does not corrupt the
frontier.

## Frontier behavior

A reached marker can create a frontier entry from the current VM snapshot. The
score combines marker novelty and prior hit count. New and rare markers receive
more priority than common markers. Frontier entries retain the marker identity,
owning guest, tick, state ref, logical-position ref, and replay-parent ref.

## Evidence and replay

`validate_replay_binding(...)` checks marker identity, owner, state ref,
logical-position ref, replay-parent identity, and snapshot identity. Any drift
fails closed.

The explorer writes `branch-marker-coverage.json` with the other exploration
outputs. It lists declared marker identities, reached identities, coverage
gaps, per-marker hit counts, and any limit event. Catalog declarations make an
unreached marker visible as a gap.

## Claim boundary

A branch marker is a workload declaration. It is not an assertion result, a
proof that the state is important, or proof that all branches were explored.
A coverage gap means only that no admitted run reached the declared marker.
Snapshot and replay bindings prove identity agreement for the recorded inputs.
They do not prove workload correctness or complete state-space coverage.
