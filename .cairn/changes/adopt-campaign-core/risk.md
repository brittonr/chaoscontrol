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

The Choregraph history implementation and Campaign Rust implementation are not published. Product adapter implementation cannot start safely.

## Residual risks

- Integer conversion can differ from floating-point order at selected boundaries.
- A bounded corpus cannot prove all entropy streams or frontier shapes equivalent.
- A structurally valid history can reference an unavailable snapshot.
- Host timing and kernel behavior can differ across KVM runs.
- Shared conformance does not prove useful exploration or correct product findings.

ChaosControl maintainers own adapter mapping, KVM behavior, product policy, and evidence claims. Campaign owns shared frontier policy. Choregraph owns structural history.
