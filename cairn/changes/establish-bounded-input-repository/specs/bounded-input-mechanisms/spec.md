# Bounded Input Mechanisms Specification

## Purpose

Defines product-neutral bounded file, JSON, serialization, and decompression mechanisms for ChaosControl and other OnixResearch consumers.

## Requirements

### Requirement: Shared mechanisms have a neutral repository

r[shared.bounded_input.repository] The project MUST publish a product-neutral `bounded-input` repository under `AGPL-3.0-or-later` with independent core, JSON, and standard-library adapter crates. Publication MUST use an immutable reviewed revision and MUST NOT require a sibling path fallback.

#### Scenario: A consumer pins the shared mechanism

- GIVEN the shared repository passes its package and test checks
- WHEN ChaosControl adopts it
- THEN ChaosControl MUST pin an immutable reviewed revision
- AND normal builds MUST NOT fall back to mutable branches or workspace-relative source.

### Requirement: Resource policy is explicit and pure

r[shared.bounded_input.policy] Every operation MUST receive named limits for the resources it can consume. Limit comparison, checked arithmetic, counter transitions, and violation classification MUST be pure deterministic logic.

#### Scenario: An operation exceeds a supplied limit

- GIVEN an operation has explicit input and output limits
- WHEN the next checked transition exceeds one limit
- THEN the core MUST return a typed violation before that transition
- AND identical facts MUST return the same result without I/O or ambient state.

### Requirement: File reads use explicit authority

r[shared.bounded_input.file] The primary file API MUST read an already-open regular-file handle. Relative opening MUST require an explicit directory capability and MUST reject unsupported file kinds or unsafe traversal before content admission.

#### Scenario: A relative input names a symbolic link

- GIVEN a caller supplies a directory capability and a relative symbolic-link path
- WHEN the strict file adapter opens the input
- THEN it MUST reject the input before reading target content
- AND it MUST NOT report the target as an admitted regular file.

#### Scenario: A regular file changes during reading

- GIVEN an admitted regular file has an expected bounded extent
- WHEN its observed extent or read result changes incompatibly during the operation
- THEN the adapter MUST return a typed changed-input failure
- AND it MUST NOT publish partial bytes as a complete value.

### Requirement: JSON preflight bounds structure

r[shared.bounded_input.json] JSON preflight MUST enforce source-byte, nesting-depth, node-count, and string-byte limits with an iterative state machine before semantic deserialization.

#### Scenario: Deep JSON exceeds policy

- GIVEN syntactically valid JSON nests beyond the supplied depth limit
- WHEN preflight scans the bytes
- THEN it MUST return a depth violation in bounded work
- AND semantic deserialization MUST NOT start.

#### Scenario: Strings contain escapes

- GIVEN a JSON string contains valid escapes and multibyte UTF-8
- WHEN preflight counts its bytes and structure
- THEN it MUST handle escape state deterministically
- AND it MUST reject malformed escapes or UTF-8 with typed failures.

### Requirement: Serialization cannot exceed its output budget

r[shared.bounded_input.serialization] Bounded serialization MUST stop before retained output exceeds the caller's byte budget. An oversized result MUST return a typed failure and MUST NOT be exposed as complete serialized data.

#### Scenario: A serializer crosses the output limit

- GIVEN a valid serializable value and a bounded writer
- WHEN the next write would cross the output budget
- THEN the writer MUST reject that write
- AND the caller MUST receive no successful complete payload.

### Requirement: Decompression bounds input and expansion

r[shared.bounded_input.decompression] Streaming decompression MUST enforce separate compressed-input and expanded-output limits and MUST classify codec errors independently from resource violations.

#### Scenario: A small input expands beyond policy

- GIVEN valid compressed bytes fit the compressed-input limit
- WHEN expansion crosses the output limit
- THEN decompression MUST stop with an expanded-output violation
- AND no partial expansion MAY be published as a complete artifact.

### Requirement: Consumer authority remains local

r[shared.bounded_input.claim_boundary] The shared repository MUST NOT claim path authorization, recursive tree safety, schema correctness, artifact trust, evidence validity, or release eligibility. Consumers MUST retain those decisions.

#### Scenario: Structurally valid JSON has an invalid schema

- GIVEN JSON passes all shared structural limits
- WHEN a ChaosControl schema rejects its fields
- THEN ChaosControl MUST reject the input through its schema policy
- AND bounded-input success MUST NOT be reported as semantic acceptance.

### Requirement: Migration preserves bounded behavior

r[shared.bounded_input.migration] ChaosControl MUST compare old and shared behavior for maintained valid and invalid corpora before removing duplicate implementations. Any intentional compatibility change MUST be explicit and tested.

#### Scenario: A maintained rejection differs

- GIVEN a negative fixture is rejected by the current implementation
- WHEN the shared implementation accepts it or returns a weaker failure
- THEN migration MUST stop
- AND local code MUST remain until the difference is resolved by an explicit requirement.

### Requirement: Checks include positive and negative cases

r[shared.bounded_input.validation] The shared and consumer suites MUST cover valid bounded files, JSON, serialization, and decompression plus malformed, changing, unsupported, overflowing, and resource-exhausting inputs.

#### Scenario: The full bounded-input suite runs

- GIVEN shared unit fixtures and ChaosControl parity fixtures
- WHEN all focused checks run
- THEN valid values MUST retain their declared results
- AND every invalid or excessive input MUST fail without panic, unbounded allocation, or partial-success publication.
