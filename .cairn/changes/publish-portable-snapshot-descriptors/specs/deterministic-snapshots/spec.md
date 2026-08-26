# Deterministic Snapshots Specification Delta

## Purpose

Publish stable exact-snapshot descriptors and restore observations for external consumers without exposing host locators or widening portability claims.

## ADDED Requirements

### Requirement: Public snapshot descriptors have canonical producer identity

r[chaoscontrol.snapshot_descriptor.contract] ChaosControl MUST define a versioned Rust-owned public snapshot descriptor. It MUST compute descriptor identity with domain-separated BLAKE3 framing over complete versioned canonical hash material and deterministic ordered inventories.

#### Scenario: Equivalent descriptors use different JSON formatting

- GIVEN two JSON projections decode to the same valid descriptor fields
- WHEN canonical descriptor identity is computed
- THEN both projections MUST produce the same descriptor identity

#### Scenario: Required cohort field changes

- GIVEN one behavior-relevant descriptor field changes
- WHEN identity is recomputed
- THEN the descriptor identity MUST change

### Requirement: Descriptors bind the complete exact cohort

r[chaoscontrol.snapshot_descriptor.complete_cohort] A descriptor MUST bind the exact completeness profile, state schema, architecture, KVM operation cohort, sorted MSR inventory, CPU and memory topology, stable device identities, backend kinds, deterministic state owners, guest artifacts, scheduler, time, entropy, and payload closure.

#### Scenario: Complete exact descriptor is admitted

- GIVEN every required field and component is present once under its stable identity
- WHEN descriptor validation runs
- THEN the descriptor MAY proceed to destination preflight

#### Scenario: Required backend state owner is absent

- GIVEN one mutable deterministic backend is missing from the descriptor inventory
- WHEN validation runs
- THEN ChaosControl MUST reject the descriptor as incomplete

### Requirement: Descriptor identity is independent of locators

r[chaoscontrol.snapshot_descriptor.locator_boundary] Canonical descriptors MUST contain tagged content identities, lengths, codecs, order, and closure roles. Paths, store names, Redb keys, tickets, URLs, mirrors, and provider handles MUST remain detached locator observations.

#### Scenario: Snapshot moves to another store

- GIVEN identical verified payload bytes and descriptor facts receive a new locator
- WHEN descriptor identity is recomputed
- THEN the descriptor identity MUST remain unchanged

#### Scenario: Locator is supplied without content identity

- GIVEN a confined path exists but no valid payload identity and length are known
- WHEN descriptor admission runs
- THEN ChaosControl MUST reject the reference as incomplete

### Requirement: Payload closure supports monolithic and chunked artifacts

r[chaoscontrol.snapshot_descriptor.closure] The descriptor MUST bind either one exact monolithic payload or one ordered complete chunk manifest. Every member MUST have a tagged digest algorithm, digest, length, role, and order where applicable. Unknown algorithms, gaps, overlaps, truncation, or digest mismatch MUST fail closed.

#### Scenario: Chunked closure verifies

- GIVEN every ordered chunk matches its declared identity and total logical payload facts
- WHEN closure validation runs
- THEN ChaosControl MUST admit the closure without requiring a host path in canonical identity

#### Scenario: One chunk is reordered

- GIVEN all chunk bytes exist but their order differs from the descriptor
- WHEN closure validation runs
- THEN ChaosControl MUST reject the payload

### Requirement: Destination preflight remains pure and exact

r[chaoscontrol.snapshot_descriptor.preflight] Preflight MUST compare a valid descriptor with supplied destination architecture, KVM, MSR, topology, device, backend, and resource observations without I/O. Unsupported or mismatching facts MUST fail before VM mutation.

#### Scenario: Destination MSR inventory differs

- GIVEN a valid descriptor and a destination with a different required MSR inventory
- WHEN preflight runs
- THEN it MUST reject restore before any KVM or device mutation

### Requirement: Restore receipts preserve mutation and poison state

r[chaoscontrol.snapshot_descriptor.restore_receipt] Detached restore receipts MUST bind descriptor, destination cohort, preflight, materialization, mutation-start, ordered phase results, poison state, completion, and bounded continuation observations.

#### Scenario: Restore fails after mutation starts

- GIVEN preflight passed and one required restore phase fails after destination mutation
- WHEN the receipt is emitted
- THEN it MUST report the destination as poisoned
- AND it MUST NOT report restore success

### Requirement: Public projections remain Rust-owned and reviewable

r[chaoscontrol.snapshot_descriptor.projection] Rust DTOs MUST own emitted descriptor and restore-receipt facts. JSON schemas and Nickel review contracts MUST be generated or checked from that owner with fixture parity and content-bound freshness.

#### Scenario: Rust shape changes without contract update

- GIVEN a descriptor field changes while its schema, contract, or fixtures remain stale
- WHEN projection freshness runs
- THEN validation MUST fail before publication

### Requirement: Consumer fixtures preserve ChaosControl ownership

r[chaoscontrol.snapshot_descriptor.consumer_contract] ChaosControl MUST publish a refs-only external-consumer fixture for identity, closure, and exact compatibility. The fixture MUST NOT encode Molten branch, merge, authority, promotion, or release semantics.

#### Scenario: Consumer treats descriptor as restore authority

- GIVEN a consumer fixture claims descriptor validity grants runtime activation
- WHEN conformance validation runs
- THEN ChaosControl MUST reject the claim as an authority overreach

### Requirement: Descriptor verification covers hostile compatibility inputs

r[chaoscontrol.snapshot_descriptor.verification] ChaosControl MUST test valid descriptors, stable identities, complete closures, exact preflight, restore observations, incomplete inventories, locator substitution, payload tamper, cohort mismatch, poison, and portability overclaims.

#### Scenario: Focused descriptor rail runs

- GIVEN positive and negative fixtures use the reviewed exact snapshot cohort
- WHEN descriptor verification runs
- THEN it MUST report the exact supported profile and all bounded non-claims
