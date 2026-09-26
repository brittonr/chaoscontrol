# Risk analysis for adopt-campaign-core

## Assets and hazards

### Exploration policy parity

The migration must preserve frontier ranking, selection-count decay, epsilon choice, pruning, and stop behavior for selected profiles.

Hazards include numeric-conversion drift, changed tie breaks, crossed entropy, stale guidance, duplicate ordinals, and lost outstanding selections.

### VM and snapshot authority

A Choregraph moment can identify history without proving that a restorable VM snapshot exists.

Hazards include selecting an expired snapshot, confusing structural ancestry with restorable state, and storing snapshot bytes in shared crates.

### Publication order

A selected expansion can produce many KVM effects.

Hazards include execution before durable selection publication, stale branch updates, concurrent planners, and a planned selection treated as execution evidence.

### Product observations

ChaosControl uses coverage, assertion state, schedules, findings, and protocol events to classify interesting results.

Hazards include false adapter projections, loss of finding identity, shared interpretation of product scores, and structural receipts promoted into evidence.

### Dependency and rollback

Campaign and Choregraph have separate release boundaries. Migration temporarily creates two policy paths.

Hazards include sibling-path dependencies, mismatched Cargo and Nix revisions, unsupported fallback, and long-lived semantic drift.

## Mitigations

- Baseline tests bind the exact legacy frontier and exploration sources.
- Cargo and Nix select exact compatible Campaign and Choregraph revisions.
- Static checks reject ChaosControl product and evidence types from shared crates.
- Integer-rank conversion uses a versioned adapter identity and checked arithmetic.
- Equal frontier, policy, and explicit entropy inputs must produce equal selection decisions.
- Snapshot eligibility requires an exact current restorable identity or clean-bootstrap operation.
- The shell durably publishes selection before KVM expansion.
- Stale control generations cause replan without branch execution.
- ChaosControl computes coverage, score meaning, observations, findings, and stop semantics.
- Campaign pruning cannot erase Choregraph history or product artifacts.
- Model parity compares ranked and exploratory choices, score decay, pruning, budgets, and stop classes.
- KVM smoke evidence compares bounded structural and product observations.
- Legacy selection becomes diagnostic-only after cutover and cannot satisfy release gates.

## Current blockers

Choregraph history and Campaign are published. The product adapter, durable journal, publication fence, parity evidence, cutover, product-type leakage fixtures, and parity receipt classification are implemented. These obligations remain:

- Cargo and the Nix `campaign-src` input both pin Campaign development revision `a75b5e3a` (commit `97788af`), which closes the mismatched-revision hazard for this revision. It is not the Campaign release revision. The forge answered HTTP 530 at the re-pin, so both locks were resolved from local caches, and fresh acquisition of `a75b5e3a` from the live endpoint has not been observed. No release claim holds until the release re-pin.
- The full `nix flake check -L` gate has not passed on the re-pinned tree.

## Residual risks

- The leakage record-shape check cannot tell a small integer-only product value from a counter or tick. Closed record schemas (`deny_unknown_fields`, canonical re-encoding) and identity re-derivation, not the shape check, keep such values out of Campaign records.
- The host control capacity stop (`control_capacity_exhausted`) is unreachable with the derived bounds and is exercised only through test-only tighter projection and graph bounds. The genesis record stores the whole history bound, and replay uses it as both the graph and the projection bound, so a journal written under a tighter bound replays under the derived one. That is harmless for a stopped epoch, but only the derived bounds are a supported configuration.
- The Explorer's whole-round reserve assumes `ROUND_CONTROL_EVENTS` and the largest candidate batch bound every round's control events, and that `max_child_moments` (the branch factor) bounds its child moments. The pre-round retirement sweep is not part of that reserve: a sweep that no longer fits is refused by publication, none of it is staged, and the round becomes the capacity stop rather than an error.
- Campaign's graph reserve (D54) holds only while no other writer consumes it. The Explorer is the only writer of its epoch graph, and its own child moments are reserved per round. If the reserve were consumed anyway, the planner and the stop publication would both refuse with `GraphCapacityExhausted`. The epoch could then record no stop, and the round would fail with that typed error instead of a stop.
- Host-capacity waits and stops have no pre-publication plan receipt (Campaign D57 residual). ChaosControl projects no Campaign receipts, so nothing depends on one.
- Integer conversion can differ from floating-point order at selected boundaries.
- A bounded corpus cannot prove all entropy streams or frontier shapes equivalent.
- A structurally valid history can reference an unavailable snapshot.
- Host timing and kernel behavior can differ across KVM runs.
- Shared conformance does not prove useful exploration or correct product findings.

ChaosControl maintainers own adapter mapping, KVM behavior, product policy, and evidence claims. Campaign owns shared frontier policy. Choregraph owns structural history.
