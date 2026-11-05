## Decision

Extend `DashboardEvent::BugFound` with two additive fields:

- `dedup_key: u64` — the exact `BugReport::dedup_key` of the emitted failure,
  computed from the admitted assertion fingerprint and sorted fault type names.
- `replay_parent_depth: u32` — the depth already computed for the branch that
  produced the failure; depth 0 replays from the normal bootstrap snapshot, depth
  > 0 needs the saved branch context.

Both fields use `#[serde(default)]` so previously serialized events still
deserialize; serialization always includes them. The emitter clones the values it
already holds; no snapshot serialization, hashing, file I/O, or extra allocation is
added to the run loop.

Extend `CheckpointBugExportFilter` with `dedup_key: Option<u64>` and match it
against the checkpoint's stored `dedup_key`. The CLI gains
`export-bugs --dedup-key <u64>`. Existing filters compose: assertion id, minimum
parent depth, then dedup key, with `max_bugs` still capping writes after matching.
Missing `dedup_key` in a legacy checkpoint serializes as `None` and never matches a
requested key; it is not coerced to zero.

No moment/session admission, branch identity, or guest diagnostic contract is
implemented here. Those requirements exist so later phases have a reviewed owner
and explicit bounds.

## Verification

Serialize every dashboard event variant and round-trip it, asserting the new fields
appear for `BugFound`. Emit a failing assertion through `extract_bugs` and assert
the event's `dedup_key` equals the returned report's key and the depth equals the
report's depth. Export from a checkpoint whose bugs carry distinct dedup keys and
assert only the requested key is written, unmatched keys write nothing, and a
legacy checkpoint without keys rejects a keyed filter. Run the focused crate tests,
fmt, and denied-warning Clippy.

## Boundaries

A dedup key identifies a failure shape for correlation; it does not prove that two
runs, VMs, or histories are identical, that a snapshot is admissible, or that a
replay will reproduce. The event remains a dashboard signal, not replay evidence or
an accepted verdict.
