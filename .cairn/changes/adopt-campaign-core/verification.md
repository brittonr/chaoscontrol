# Verification plan for adopt-campaign-core

## Positive checks

The implementation phase will collect these checks:

- Focused legacy `frontier.rs`, `input_tree.rs`, and `explorer.rs` tests pass before core changes.
- Campaign conformance passes for the ChaosControl adapter.
- Equal snapshots and product facts map to equal opaque moment and candidate identities.
- Integer-rank conversion preserves selected corpus ordering and tie classes.
- Equal seeded entropy produces equal ranked or exploratory choices.
- Reusable candidates produce distinct successor selections with matching selection-count decay.
- Capacity pruning keeps the same selected candidates for the parity corpus.
- Exact durable selection publication occurs before mutation, worker, snapshot, or KVM effects.
- One selected expansion can report a bounded group of child moments and close one outstanding selection.
- Full parallel capacity produces a wait decision, not a false frontier or budget stop.
- Coverage, assertions, findings, corpus retention, and report meaning remain in ChaosControl.
- Existing checkpoints and multi-seed progress remain product-owned.
- The selected adaptive KVM smoke produces matching bounded decisions and product observations.
- Clippy, Octet, Cairn, and `nix flake check -L` pass.

Expected evidence includes baseline output, dependency pins, Campaign conformance, model parity, KVM smoke receipts, and current Cairn gate receipts.

## Negative checks

The test suite will reject or expose:

- moving, sibling-path, unpublished, incompatible, or mismatched Campaign and Choregraph revisions;
- ChaosControl schedules, snapshots, coverage, assertions, findings, VMM, storage, or evidence types in Campaign crates;
- unknown moments, duplicate candidates, duplicate selection ordinals, stale guidance, crossed policy, and malformed operation identities;
- floating-point values at the Campaign boundary, integer conversion overflow, and changed near-boundary ordering;
- missing, stale, crossed, or unbound epsilon entropy;
- selection of a moment without a restorable snapshot or clean-bootstrap fact;
- branch execution before exact durable selection publication;
- stale generation, stale head, duplicate selection, and concurrent planner conflicts;
- crossed or duplicate child moments and false observation binding;
- Campaign pruning that erases Choregraph history or product artifacts;
- Campaign or Choregraph receipts presented as VM, fault, assertion, finding, replay, minimization, or release evidence;
- Campaign adoption of multi-seed dispatch, progress, resume, aggregation, or report projection;
- implicit legacy policy selection after supported cutover.

## Current gaps

KVM checks require a compatible Linux host and `/dev/kvm`. If unavailable, model checks can pass, but supported cutover remains blocked.

Campaign and Choregraph history are not published. Adapter and end-to-end conformance checks remain blocked until both immutable revisions exist.

## Non-claims

Passing parity proves only the selected corpus and profiles. It does not prove exhaustive search, all entropy streams, snapshot durability, KVM correctness, finding truth, or release readiness.
