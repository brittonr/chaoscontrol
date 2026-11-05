# Design: Protocol-observation cohorts

## Context

`OracleEvent` records have a name and structured details. Explorer enrichment hashes those values into coverage slots with `DefaultHasher`. First-party Raft code records useful protocol state, but its fields and cross-node logic stay inside that fixture.

Storage recovery and projected role-protocol changes need coordinated observations, complete accounting, stable identities, and consumer-owned pure oracles. A shared mechanism can provide those facts without teaching the VMM Raft, VSR, Choregraph, Lattice, or Molten semantics.

This design adapts the protocol-aware deterministic simulation method described at:

- `https://tigerbeetle.com/blog/2026-08-20-protocol-aware-dst/`

The source is a design reference. Its protocol rules and correctness claims do not transfer to ChaosControl.

## Success Contract

A campaign can collect bounded canonical observations from its admitted participants. The pure core can assemble an exact cohort at a consumer-defined logical boundary.

A complete cohort has valid identities, required participants, continuous source sequences, admitted generations, bounded payloads, and complete loss accounting. Only a consumer-owned oracle can assign protocol meaning.

## Decisions

### Decision: The shared record is an opaque protocol envelope

**Choice:** The profile binds protocol, projection-schema, producer, participant, oracle-adapter, and bound identities. Each runtime record binds guest or process, generation, source sequence, transition class, logical-boundary ref, projection ref, optional bounded projection bytes, scheduler position, and completeness facts.

The runtime record is Rust-owned. The human-authored profile and review defaults are typed Nickel.

**Rationale:** ChaosControl needs stable transport facts, not consumer semantics.

### Decision: Canonical identities use BLAKE3

**Choice:** The pure core computes domain-separated BLAKE3 identities over canonical envelope fields and admitted opaque projection bytes. A consumer can provide an existing canonical projection ref when the payload remains external.

Coverage slots can remain compact runtime guidance. The complete BLAKE3 identity remains the durable novelty and evidence identity.

**Rationale:** Process-local hash behavior is not a stable replay or evidence contract.

### Decision: Consumers define logical boundaries

**Choice:** Cohorts group records by exact protocol cohort ref and consumer-defined logical-boundary ref. The profile declares required participant identities and the completion rule.

ChaosControl does not infer a semantic total order from host time, guest time, arrival order, or source sequence across producers.

**Rationale:** Protocol terms, indexes, cursors, labels, and checkpoints have consumer-owned meaning.

### Decision: Cohort assembly is pure and fail-closed

**Choice:** The pure core validates schema refs, bounds, participant membership, generations, source sequences, projection refs, duplicates, loss counters, and final-drain facts. It returns complete, incomplete, conflicting, or unsupported.

A missing participant, gap, overflow, malformed record, unknown schema, stale generation, conflicting projection, or failed final drain cannot become complete.

**Rationale:** Oracle results cannot exceed the available observations.

### Decision: Oracle semantics remain consumer-owned

**Choice:** A workload adapter maps an admitted cohort into its own pure oracle input. The adapter returns typed protocol results and an exact oracle identity.

The adapter cannot use the runtime under test as its only expected-result source. ChaosControl records the result but does not reinterpret it.

**Rationale:** A generic VMM cannot decide consensus, storage, choreography, or application correctness.

### Decision: Novelty uses selected stable protocol facts

**Choice:** The profile selects bounded opaque projection fields or refs for novelty. The pure core computes a BLAKE3 novelty identity.

The explorer can map this identity into coverage guidance. Evidence retains the full identity and the selected-field profile ref.

**Rationale:** Stable protocol-state guidance improves replay and cross-run comparison.

### Decision: Markers can bind protocol observations and snapshots

**Choice:** After `sut-declared-event-branching` publishes its contract, an observation can bind a declared marker identity. Marker evidence can then bind the logical boundary, projection, cohort, and restorable parent snapshot refs.

A marker does not make an incomplete cohort complete. A snapshot does not prove that all participants reached one wall-clock instant.

**Rationale:** Branching at important protocol states needs exact state and evidence linkage.

### Decision: Observation costs are bounded

**Choice:** Profiles declare finite records per producer, bytes per record, total bytes, participants, logical boundaries, cohort backlog, and oracle work. Repeated equivalent records can collapse only under the declared identity rule.

**Rationale:** Protocol visibility must not make the simulator unbounded.

### Decision: Evidence preserves non-claims

**Choice:** Receipts bind profile, producer, participants, schemas, records, cohorts, completeness, oracle adapter, results, novelty, markers, snapshots, scheduler, faults, replay, bounds, and non-claims.

Passing evidence cannot prove unobserved states, arbitrary schedules, universal protocol correctness, production readiness, or release eligibility.

**Rationale:** The mechanism reports bounded observations and consumer results only.

## Functional Core and Imperative Shell

The pure core owns profile admission, record identity, sequence validation, cohort assembly, completeness classification, novelty identity, and evidence classification.

The shell owns SDK transport, VMM collection, scheduler linkage, snapshot capture, artifact storage, oracle-adapter invocation, explorer updates, replay orchestration, and receipt persistence.

## Dependencies and Adoption

The core envelope and cohort work can start from current SDK and oracle records. Marker and snapshot linkage depends on the active `sut-declared-event-branching` contract.

The active `model-guest-storage-flush-failures` and `exercise-projected-role-protocol-faults` changes can consume this mechanism after publication. Their protocol semantics stay in their workload adapters.

Campaign adoption is not a blocker. Campaign can later rank opaque novelty identities without learning protocol semantics.

## Risks and Trade-offs

- Consumer projections can omit decisive state. Adapter review and negative fixtures remain required.
- A consumer oracle can repeat the implementation defect. Independent fixtures and outside-in histories remain necessary.
- Large projections can flood transport. Profiles enforce finite payload and aggregate bounds.
- Logical boundaries can be misdeclared. ChaosControl validates identity and completeness, not semantic correctness.
- Novelty can overfit implementation details. Profiles select reviewed stable fields and retain schema identity.
