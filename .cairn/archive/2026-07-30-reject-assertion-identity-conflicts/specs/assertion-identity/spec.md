# Assertion Identity Specification

## Purpose

Bind every assertion registration, runtime event, snapshot, and report record to a validated structured descriptor and reject ambiguous identity instead of silently merging properties.

## ADDED Requirements

### Requirement: Assertion identity is structured and canonical

r[chaoscontrol.assertion_identity.model] ChaosControl MUST represent an assertion with a versioned catalog namespace and logical key plus a complete canonical descriptor containing kind, message, normalized source metadata, guest, and category; its compact fingerprint MUST be domain-separated BLAKE3 over deterministic canonical descriptor bytes.

#### Scenario: Descriptor is fingerprinted

- GIVEN a valid assertion namespace, logical key, kind, message, source, guest, and category
- WHEN canonical identity is constructed
- THEN the same normalized descriptor MUST produce the same canonical bytes and fingerprint
- AND the registry MUST retain the canonical descriptor rather than relying on the fingerprint alone.

### Requirement: Complete catalogs are validated before use

r[chaoscontrol.assertion_identity.catalog_validation] A catalog MUST be accepted only after every descriptor and the catalog-complete identity validate. Exact duplicate descriptors MAY be idempotent. Any logical-key, fingerprint, metadata, malformed-field conflict, or `LegacyU32` descriptor MUST reject strict admission.

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

r[chaoscontrol.assertion_identity.event_binding] In strict mode, every runtime assertion event MUST resolve through its accepted catalog fingerprint and token to exactly one retained descriptor. A live oracle MUST mutate counters only through this bound-event path and MUST NOT retain a mutable legacy record map.

#### Scenario: Known event is recorded

- GIVEN a catalog is active and a runtime event carries the matching accepted identity
- WHEN the oracle records the event
- THEN only that descriptor's counts and verdict state MUST change.

#### Scenario: Event is unknown or mismatched

r[chaoscontrol.assertion_identity.validation.events]
- GIVEN an event arrives without an active run, before catalog completion, names an unknown fingerprint, uses a stale or mismatched token, spoofs another kind, or follows a catalog conflict
- WHEN strict event resolution runs
- THEN the event MUST be rejected
- AND no assertion counter MAY change
- AND the run MUST be ineligible for accepted assertion evidence until policy-defined recovery succeeds.

#### Scenario: Setup completion arrives without an active run

- GIVEN no run is active
- WHEN setup completion arrives
- THEN the oracle MUST return `NoActiveRun`
- AND the engine MUST return an error status
- AND engine setup state MUST remain false.

#### Scenario: A catalog remains incomplete at a run boundary

- GIVEN a catalog builder has begun but has not completed
- WHEN the engine ends the run
- THEN it MUST clear the builder and record a fatal `CatalogIncomplete` conflict
- AND the next run MUST start without a pending builder.

### Requirement: Oracle and reports retain structured identity

r[chaoscontrol.assertion_identity.report_identity] Oracle records, oracle events, snapshots, local reports, campaign reports, and evidence-boundary summaries MUST retain identity version, namespace/logical key, complete descriptor, fingerprint, and catalog-validation status needed to verify their binding.

#### Scenario: Oracle state is snapshotted

- GIVEN a validated catalog and recorded events
- WHEN oracle state is snapshotted and restored
- THEN descriptor bindings, catalog status, counts, active-run facts, and event resolution MUST remain unchanged
- AND engine setup state MUST equal the serialized oracle current-run setup state.

#### Scenario: Diagnostic snapshot enters runtime restore

- GIVEN a bounded historical snapshot is `legacy-ambiguous` or `fatal-conflict`
- WHEN oracle, engine, VM, or controller restore preflight runs
- THEN restore MUST fail before mutation.

#### Scenario: Pending snapshot enters runtime restore

- GIVEN a snapshot has no accepted catalog
- WHEN runtime restore preflight runs
- THEN restore MAY proceed only if the state is pristine with no records, conflicts, events, completed runs, or active run.

### Requirement: Aggregation proves descriptor equality

r[chaoscontrol.assertion_identity.merge] Multi-VM aggregation MUST combine assertion counts only when namespace, logical key, canonical descriptor, and fingerprint all match. Each source MUST carry a true collision-safe claim after independent fact validation. Prepared aggregate validation MUST remain internal and non-promoting until the shell derives the final true claim.

#### Scenario: Same guest property appears in multiple VM instances

r[chaoscontrol.assertion_identity.validation.merge]
- GIVEN multiple VM instances run the same validated guest catalog descriptor
- WHEN controller reports aggregate their results
- THEN counts MAY combine under that one property while preserving per-instance provenance.

#### Scenario: Different guest catalogs reuse a compact transport alias

- GIVEN different catalog namespaces contain strict descriptors with the same non-authoritative compact alias
- WHEN reports are merged
- THEN the descriptors MUST remain separate
- AND a same-namespace metadata conflict MUST fail rather than aggregate.

#### Scenario: Compatibility alias selects a record

- GIVEN a report is accepted, collision-safe, conflict-free, legacy-empty, and independently valid
- WHEN a minimizer or CLI selects a unique compatibility alias
- THEN it MAY return only the matching structured record
- AND a legacy-only, mixed, active-run, demoted, malformed, or ambiguous-alias report MUST return an error.

#### Scenario: Final counters refer to an unfinished run

- GIVEN a final report has observations with zero completed runs, zero run-hit counters for nonzero hits, zero satisfied-run counters for nonzero true counts, or `first_failure_run >= total_runs`
- WHEN strict report validation runs
- THEN validation MUST reject the report
- AND only a validated active-run snapshot MAY use the current run index before finalization.

### Requirement: Local reports use the same conflict rules

r[chaoscontrol.assertion_identity.local_reports] Local JSONL report generation MUST apply the same canonical descriptor validation, namespace separation, event binding, and conflict rejection as VMM oracle reporting.

#### Scenario: Local stream changes metadata under one identity

r[chaoscontrol.assertion_identity.validation.local]
- GIVEN a local stream repeats an identity with different message, kind, source, guest, or category
- WHEN the report is generated
- THEN generation MUST fail with a deterministic conflict diagnostic
- AND first-seen metadata MUST NOT hide the conflict.

### Requirement: Replay artifacts bind exact assertion identity

r[chaoscontrol.assertion_identity.replay_artifacts] Exported bugs and schema-v2 replay verdicts MUST carry the failed assertion fingerprint, complete descriptor, canonical descriptor bytes, and catalog token. Before reproduction or minimization mutates runtime state, the carrier MUST resolve through a reconstructed accepted catalog or validated restored report. The numeric assertion alias is redundant metadata: it MUST equal a present descriptor compatibility ID, or it MUST be zero when the descriptor has no compatibility ID.

#### Scenario: Exact bug identity enters replay

- GIVEN a bug carries a strict descriptor, fingerprint, canonical bytes, catalog token, and redundant alias
- WHEN reproduction restores the parent report
- THEN replay MUST resolve the exact fingerprint through the restored accepted catalog before applying the schedule
- AND the replay verdict MUST retain the same exact identity.

#### Scenario: Replay carrier substitutes identity data

- GIVEN a bug or verdict changes its descriptor, canonical bytes, fingerprint, catalog token, or redundant alias
- WHEN replay, minimization, checkpoint resume, campaign aggregation, export, or evidence promotion validates it
- THEN the complete carrier MUST be rejected before mutation or publication
- AND alias collisions MUST resolve only by exact fingerprint and descriptor equality.

#### Scenario: One bug in a collection is invalid

- GIVEN a checkpoint or campaign contains one valid bug and one missing, legacy, or malformed bug identity
- WHEN resume, aggregation, auto-minimization preparation, or export validates the collection
- THEN the whole untrusted collection MUST fail or become explicitly fatal and non-promoting
- AND the invalid bug MUST NOT be silently omitted.

#### Scenario: Historical bug has only an integer ID

- GIVEN a historical bug or schema-v1 replay verdict has only an assertion integer
- WHEN a bounded diagnostic reader opens it
- THEN the reader MAY preserve it as non-authoritative diagnostic data
- BUT reproduction, minimization, export, and promotion MUST reject it.

### Requirement: Legacy identity is explicitly unsupported

r[chaoscontrol.assertion_identity.compatibility] Public `u32` assertion APIs, explicit-ID macros, compatibility commands, and unbound guidance MUST NOT exist. Historical `LegacyU32` serialized input MUST be rejected in strict mode or labeled `legacy-ambiguous` by bounded diagnostic readers.

#### Scenario: Legacy report enters strict evidence validation

r[chaoscontrol.assertion_identity.validation.legacy]
- GIVEN a report carries only unnamespaced integer assertion IDs and lacks a validated structured catalog
- WHEN strict evidence validation runs
- THEN the report MUST NOT satisfy collision-safe assertion evidence
- AND diagnostic parsing, if enabled, MUST preserve a visible legacy-ambiguous classification.

#### Scenario: Removed source and wire forms are used

- GIVEN source code uses an old explicit-ID assertion API, or a guest sends removed command `0x05` or `0x07`
- WHEN the source builds or the host dispatches the command
- THEN the old source API MUST be absent
- AND the removed wire command MUST return the unknown-command error without changing assertion state.

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
