# Assertion Identity Specification

## Purpose

Bind every assertion registration, runtime event, snapshot, and report record to a validated structured descriptor and reject ambiguous identity instead of silently merging properties.

## Requirements

### Requirement: Assertion identity is structured and canonical

r[chaoscontrol.assertion_identity.model] ChaosControl MUST represent an assertion with a versioned catalog namespace and logical key plus a complete canonical descriptor containing kind, message, normalized source metadata, guest, and category; its compact fingerprint MUST be domain-separated BLAKE3 over deterministic canonical descriptor bytes.

#### Scenario: Descriptor is fingerprinted

- GIVEN a valid assertion namespace, logical key, kind, message, source, guest, and category
- WHEN canonical identity is constructed
- THEN the same normalized descriptor MUST produce the same canonical bytes and fingerprint
- AND the registry MUST retain the canonical descriptor rather than relying on the fingerprint alone.

### Requirement: Complete catalogs are validated before use

r[chaoscontrol.assertion_identity.catalog_validation] A catalog MUST be accepted only after every descriptor and the catalog-complete identity validate, exact duplicate descriptors MAY be idempotent, and any logical-key, fingerprint, metadata, malformed-field, or legacy-alias conflict MUST reject the catalog.

#### Scenario: Exact descriptor is registered twice

- GIVEN two registrations have the same namespace, logical key, canonical descriptor, and fingerprint
- WHEN catalog insertion evaluates them
- THEN they MUST resolve to one assertion identity without duplicating counts or changing metadata.

#### Scenario: One logical key has conflicting metadata

r[chaoscontrol.assertion_identity.validation.conflicts]
- GIVEN two registrations share a logical key or fingerprint but differ in kind, message, source, guest, category, or canonical bytes
- WHEN catalog validation runs
- THEN it MUST return a typed conflict
- AND neither descriptor MAY silently replace or merge with the other.

#### Scenario: Test candidates force a digest collision

- GIVEN two different canonical descriptors are presented with the same injected test fingerprint
- WHEN the pure registry compares them
- THEN it MUST reject a fingerprint collision based on retained canonical bytes
- AND no test depends on finding a real BLAKE3 collision.

### Requirement: Catalog protocol preserves complete identity

r[chaoscontrol.assertion_identity.catalog_protocol] The SDK/host protocol MUST carry a versioned catalog boundary, complete descriptor fields, and a canonical catalog-complete identity without dropping source or namespace metadata.

#### Scenario: Catalog completes successfully

- GIVEN the SDK emits a valid catalog begin record, canonical descriptors, and matching complete identity
- WHEN the host decoder and validator process them
- THEN the host MUST activate exactly that catalog
- AND subsequent reports MUST retain every authoritative descriptor field.

### Requirement: Runtime events bind to an accepted catalog

r[chaoscontrol.assertion_identity.event_binding] In strict mode, every runtime assertion event MUST resolve through its accepted catalog fingerprint or token to exactly one retained descriptor; the oracle MUST NOT auto-create an authoritative record from an event ID or message.

#### Scenario: Known event is recorded

- GIVEN a catalog is active and a runtime event carries the matching accepted identity
- WHEN the oracle records the event
- THEN only that descriptor's counts and verdict state MUST change.

#### Scenario: Event is unknown or mismatched

r[chaoscontrol.assertion_identity.validation.events]
- GIVEN an event arrives before catalog completion, names an unknown fingerprint, uses a stale or mismatched token, spoofs another kind, or follows a catalog conflict
- WHEN strict event resolution runs
- THEN the event MUST be rejected or quarantined as unverified
- AND the run MUST be ineligible for accepted assertion evidence until policy-defined recovery succeeds.

### Requirement: Oracle and reports retain structured identity

r[chaoscontrol.assertion_identity.report_identity] Oracle records, oracle events, snapshots, local reports, campaign reports, and evidence-boundary summaries MUST retain identity version, namespace/logical key, complete descriptor, fingerprint, and catalog-validation status needed to verify their binding.

#### Scenario: Oracle state is snapshotted

- GIVEN a validated catalog and recorded events
- WHEN oracle state is snapshotted and restored
- THEN descriptor bindings, catalog status, counts, and event resolution MUST remain unchanged.

### Requirement: Aggregation proves descriptor equality

r[chaoscontrol.assertion_identity.merge] Multi-VM aggregation MUST combine assertion counts only when namespace, logical key, canonical descriptor, and fingerprint all match; VM instance remains an explicit dimension and distinct namespaces MUST remain separate.

#### Scenario: Same guest property appears in multiple VM instances

r[chaoscontrol.assertion_identity.validation.merge]
- GIVEN multiple VM instances run the same validated guest catalog descriptor
- WHEN controller reports aggregate their results
- THEN counts MAY combine under that one property while preserving per-instance provenance.

#### Scenario: Different guest catalogs reuse a compatibility ID

- GIVEN different catalog namespaces contain descriptors with the same legacy integer alias
- WHEN reports are merged
- THEN the descriptors MUST remain separate
- AND a same-namespace metadata conflict MUST fail rather than aggregate.

### Requirement: Local reports use the same conflict rules

r[chaoscontrol.assertion_identity.local_reports] Local JSONL report generation MUST apply the same canonical descriptor validation, namespace separation, event binding, and conflict rejection as VMM oracle reporting.

#### Scenario: Local stream changes metadata under one identity

r[chaoscontrol.assertion_identity.validation.local]
- GIVEN a local stream repeats an identity with different message, kind, source, guest, or category
- WHEN the report is generated
- THEN generation MUST fail with a deterministic conflict diagnostic
- AND first-seen metadata MUST NOT hide the conflict.

### Requirement: Legacy identity is explicitly non-authoritative

r[chaoscontrol.assertion_identity.compatibility] Existing `u32` assertion APIs MUST map into an explicit namespaced legacy-key form, while `u32`-only protocol, snapshot, or report inputs MUST be rejected in strict mode or labeled `legacy-ambiguous` in diagnostic mode.

#### Scenario: Legacy report enters strict evidence validation

r[chaoscontrol.assertion_identity.validation.legacy]
- GIVEN a report carries only unnamespaced integer assertion IDs and lacks a validated structured catalog
- WHEN strict evidence validation runs
- THEN the report MUST NOT satisfy collision-safe assertion evidence
- AND diagnostic parsing, if enabled, MUST preserve a visible legacy-ambiguous classification.

### Requirement: Identity logic has a functional core

r[chaoscontrol.assertion_identity.boundary] Canonicalization, fingerprint-input construction, catalog insertion/completion, conflict classification, event resolution, and report merge MUST be pure deterministic logic, while SDK transport, KVM hypercalls, file reads, persistence, logging, and rendering remain in imperative shells.

#### Scenario: Identity core is replayed

- GIVEN identical descriptor candidates, catalog boundaries, events, and merge inputs
- WHEN the identity core evaluates them
- THEN it MUST return identical accepted identities or conflicts without filesystem, environment, clock, process, KVM, network, output, or ambient mutable-state access.

### Requirement: Assertion identity validation is adversarial

r[chaoscontrol.assertion_identity.validation] The change MUST test idempotent duplicates, metadata conflicts, forced fingerprint collisions, runtime binding failures, namespace-aware multi-VM merge, local-report conflicts, snapshot continuity, and strict legacy rejection.

#### Scenario: Full identity regression suite runs

- GIVEN positive and negative identity fixtures cover SDK, protocol, oracle, controller merge, local report, snapshot, and review-boundary contract paths
- WHEN the focused and workspace suites run
- THEN no conflicting canonical descriptors MAY merge
- AND all accepted event counts MUST resolve to one validated descriptor.
