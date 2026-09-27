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

## Current state

Choregraph history and Campaign are published. The product adapter, durable journal, publication fence, parity evidence, cutover, product-type leakage fixtures, and parity receipt classification are implemented. Every lifecycle task is complete, including the final re-pin (task 41) and the final gate (task 42). One obligation remains outside the lifecycle tasks:

- Cargo and the Nix `campaign-src` input both pin the final Campaign revision `e7757a51` (R5, commit `351b59c`), which closes the mismatched-revision hazard for this revision. The forge answered HTTP 530 at the re-pin, so both locks were resolved from local caches, and fresh acquisition of `e7757a51` from the live endpoint has not been observed. No release claim holds until it is.
- The full `nix flake check -L` gate passed at the final head `ba3e8d3` (code `351b59c`), using the cached Campaign source while the forge answered HTTP 530. See "Final gate" in `verification.md`.

## Residual risks

- `e7757a51` is a development pin, not a published Campaign release revision: Campaign has designated no release revision, and fresh acquisition of `e7757a51` from the live endpoint has not been observed, so this change makes no release claim (owner: ChaosControl maintainers).
- No check enforces `r[chaoscontrol.campaign_policy.source_pins.invalid]` for Campaign: `flake.nix` compares `campaign-src` only with its own constant, and nothing compares it with the Campaign revisions in `Cargo.toml` and `Cargo.lock` or rejects a branch or tag Campaign dependency, so Cargo and Nix disagreed from `fb9a221` to `ec1625e` unchecked; agreement at `e7757a51` rests on the re-pin record (owner: ChaosControl maintainers).
- The leakage record-shape check cannot tell a small integer-only product value from a counter or tick. Closed record schemas (`deny_unknown_fields`, canonical re-encoding) and identity re-derivation, not the shape check, keep such values out of Campaign records.
- The host control capacity stop (`control_capacity_exhausted`) is unreachable with the derived bounds and is exercised only through test-only tighter projection and graph bounds. The genesis record stores the whole history bound, and replay uses it as both the graph and the projection bound, so a journal written under a tighter bound replays under the derived one. That is harmless for a stopped epoch, but only the derived bounds are a supported configuration.
- The Explorer's whole-round reserve assumes `ROUND_CONTROL_EVENTS` and the largest candidate batch bound every round's control events, and that `max_child_moments` (the branch factor) bounds its child moments. The pre-round retirement sweep is not part of that reserve: a sweep that no longer fits is refused by publication, none of it is staged, and the round becomes the capacity stop rather than an error.
- Campaign's graph reserve (D54, exposed as `graph_room()` since D75) holds only while no other writer consumes it. Since D82 Campaign refuses a second campaign under the same envelope on one graph (`SharedControlGraph`), but campaigns under other envelopes and non-Campaign appends still count as other writers against `graph_room()`. The Explorer is the only writer of its epoch graph, and its whole-round check keeps every append of a round, control events and child moments alike, within the exposed `graph_room()`. If the reserve were consumed anyway, the planner and the stop publication would both refuse with `GraphCapacityExhausted`. The epoch could then record no stop, and the round would fail with that typed error instead of a stop.
- The Explorer never leaves a pending control draft (D63, D75): each publication's graph admission and branch update commit in one journal segment and replay together. A journal edited outside the Explorer, a lowered graph or projection bound, or a foreign Choregraph writer could still leave a draft the Explorer cannot republish, and Campaign has no abandonment API, so that head would stay blocked with `PendingControlDraft` (Campaign's D75 residual; owners: Campaign adapter maintainers for the API, ChaosControl maintainers for the journal and its single-writer rule). The Explorer has no recovery path that republishes a pending draft through `pending_control_draft`, because its own histories cannot contain one.
- Campaign's recovery guarantees cover only hosts that plan and publish on durably committed branch values (D91, `campaign.publication_fence.durable_base`); a pipelined chain of drafts can be refused at a bound and block its head (Campaign's pipelined-chain residual; owners: Campaign adapter maintainers and consumer hosts). The Explorer never pipelines in Campaign's sense because its graph and branch are one in-process store: each publication moves the branch in the same call that admits its draft, and a journal segment makes a round's drafts and branch moves durable together or not at all. Later publications of one segment therefore plan on branch values that are durable only when the segment commits; the Explorer relies on the segment's atomicity and on halting after a failed commit, not on a per-publication durable compare-and-set. A future host that persisted the graph and the branch separately, or kept publishing after a failed segment, would need to meet the per-publication rule instead.
- The capacity check counts every descending control draft as its own event plus a settlement, as Campaign's reserve does (D83), including racing siblings of which at most one can commit. For a pending result, closure, guidance, or stop, or a losing sibling, this is more than needed, so the capacity stop can come earlier than necessary; it never comes later. The Explorer's own histories hold no drafts, so this applies only to foreign or edited histories.
- Campaign's `StaleGraph` check (D84) records non-draft graph events only as a count, so a graph of equal or greater length that lacks some foreign non-draft event is not detected (Campaign residual; such an event does not affect the reserve). The Explorer always advances over its own growing graph, so it never presents such a graph; owners: Campaign adapter maintainers for the check, ChaosControl maintainers for the single-writer journal.
- Host-capacity waits and stops have no pre-publication plan receipt (Campaign D57 residual). ChaosControl projects no Campaign receipts, so nothing depends on one.
- Integer conversion can differ from floating-point order at selected boundaries.
- A bounded corpus cannot prove all entropy streams or frontier shapes equivalent.
- A structurally valid history can reference an unavailable snapshot.
- Host timing and kernel behavior can differ across KVM runs.
- Shared conformance does not prove useful exploration or correct product findings.
- `history.state_coverage` covers a bounded 4-way combination of declared state features over 176 table classes with 3 candidates, and since Campaign `e7757a51` (D99) pairs each 3-way held-feature combination with presented forks at depths 0 to 3 and the deepest admitted depth and with 20 fork head sub-kinds. Passing it does not prove every reachable state, longer lineages, or dimensions the suite does not declare (such as execution-gate or cache timing against the held state) are handled; the adapter's own history, cache, and fence tests remain the evidence for those.

ChaosControl maintainers own adapter mapping, KVM behavior, product policy, and evidence claims. Campaign owns shared frontier policy. Choregraph owns structural history.
