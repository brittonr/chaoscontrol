# Assertion Semantics Core Specification

## Purpose

Defines reusable assertion identity, catalog, oracle, and report semantics outside ChaosControl transport and evidence policy.

## Requirements

### Requirement: Assertion semantics have a shared repository

r[shared.assertion_semantics.repository] The project MUST publish an `assertion-semantics` repository under `AGPL-3.0-or-later` with independent model, catalog, and oracle crates. ChaosControl MUST pin an immutable reviewed revision without a sibling path fallback.

#### Scenario: ChaosControl adopts a shared release

- GIVEN the shared repository has passing package and behavior checks
- WHEN ChaosControl updates its assertion dependency
- THEN it MUST pin one immutable reviewed revision
- AND protocol, SDK, and host adapters MUST use compatible semantic versions.

### Requirement: The model preserves canonical identity

r[shared.assertion_semantics.model] The model crate MUST preserve the accepted descriptor fields, normalization, canonical bytes, domain separation, BLAKE3 fingerprint, catalog token inputs, and identity versions. It MUST support `no_std` plus `alloc` for guest use.

#### Scenario: A maintained descriptor is encoded

- GIVEN a descriptor from the accepted ChaosControl identity corpus
- WHEN the shared model canonicalizes and fingerprints it
- THEN its canonical bytes and compact fingerprint MUST equal the pre-extraction values
- AND the implementation MUST retain complete canonical bytes for collision checks.

### Requirement: Catalog admission is pure and conflict-safe

r[shared.assertion_semantics.catalog] Catalog insertion, completion, token binding, event resolution, and namespace-aware merge MUST be pure deterministic operations. Exact duplicates MAY be idempotent. Conflicting descriptors, forced fingerprint collisions, stale tokens, and unknown events MUST return typed failures without state mutation.

#### Scenario: Two descriptors share a forced fingerprint

- GIVEN test inputs contain different canonical descriptors with one injected fingerprint
- WHEN catalog insertion compares them
- THEN it MUST return a fingerprint-collision failure
- AND neither descriptor MAY replace or merge with the other.

#### Scenario: A known event uses the accepted token

- GIVEN a complete accepted catalog and a matching event token
- WHEN pure event resolution runs
- THEN it MUST resolve exactly one retained descriptor
- AND it MUST return the deterministic transition input for that descriptor.

### Requirement: Oracle transitions are reusable and deterministic

r[shared.assertion_semantics.oracle] Run start, setup, hit, pass, fail, completion, snapshot, restore, and report aggregation MUST use explicit pure state transitions. Invalid order, overflow, stale catalog state, and incompatible snapshots MUST fail without partial mutation.

#### Scenario: An event arrives before run start

- GIVEN an accepted catalog and no active run
- WHEN an assertion event enters the oracle core
- THEN it MUST return a typed order failure
- AND counters and setup state MUST remain unchanged.

#### Scenario: Oracle state is restored

- GIVEN a compatible accepted oracle snapshot
- WHEN restore preflight and pure reconstruction run
- THEN the restored catalog bindings, counters, run state, and ordering MUST equal the captured state.

### Requirement: ChaosControl owns transport and runtime policy

r[shared.assertion_semantics.chaoscontrol_boundary] Hypercall command numbers, wire framing, guest macros, KVM dispatch, persistence paths, report rendering, replay admission, and readiness gates MUST remain in ChaosControl adapters.

#### Scenario: A guest event reaches the host

- GIVEN a guest sends a valid ChaosControl wire event
- WHEN host dispatch handles it
- THEN the adapter MUST decode and bind it through the shared semantic core
- AND the shared core MUST NOT perform the hypercall or KVM I/O.

### Requirement: Valence keeps evidence ownership

r[shared.assertion_semantics.valence_boundary] The shared repository MAY expose deterministic assertion facts for Valence wrapping. It MUST NOT define canonical Evidence IR, verification roles, stack provenance, evidence promotion, or release eligibility.

#### Scenario: Assertion facts enter Valence

- GIVEN an accepted assertion catalog and report
- WHEN a Valence adapter creates an evidence sidecar
- THEN the sidecar MUST preserve shared assertion identity
- AND Valence MUST remain the owner of evidence roles and canonical stack linkage.

### Requirement: Compatibility is explicit

r[shared.assertion_semantics.compatibility] Shared crate versions, ChaosControl protocol versions, report schema versions, and snapshot schema versions MUST have an explicit compatibility table. Unsupported combinations MUST fail before runtime mutation.

#### Scenario: A protocol uses an incompatible model version

- GIVEN a protocol artifact names an unsupported assertion model version
- WHEN admission runs
- THEN it MUST return a typed compatibility failure
- AND no catalog or oracle state MAY activate.

### Requirement: Migration proves parity

r[shared.assertion_semantics.migration] Before local semantic code is removed, the old and shared implementations MUST produce exact identity bytes and equal typed outcomes for maintained descriptor, catalog, event, snapshot, and report corpora.

#### Scenario: One canonical byte differs

- GIVEN a maintained descriptor corpus
- WHEN old and shared canonical outputs are compared
- THEN any byte difference MUST block migration
- AND ChaosControl MUST retain the current implementation until an explicit identity-version change is accepted.

### Requirement: Checks are adversarial

r[shared.assertion_semantics.validation] Shared and ChaosControl suites MUST include positive identity and transition cases plus malformed fields, forced collisions, unknown events, stale tokens, invalid order, overflow, merge conflicts, incompatible snapshots, and strict legacy rejection.

#### Scenario: Full assertion semantics checks run

- GIVEN shared unit cases and ChaosControl integration fixtures
- WHEN the focused suites run
- THEN accepted events MUST bind to exactly one descriptor
- AND no invalid case MAY mutate counters, merge conflicting identities, or produce accepted evidence.
