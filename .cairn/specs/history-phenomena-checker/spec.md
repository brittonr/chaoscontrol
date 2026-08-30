# History Phenomena Checker Specification

## Purpose

Defines the `history-phenomena-checker` capability.

## Requirements

### Requirement: History is typed and pure

r[chaoscontrol.phenomena.history] The core MUST accept typed histories of operations with identities and dependencies, MUST reject a history with an unclassifiable record, and MUST not read files, clocks, or processes.

#### Scenario: Unclassifiable record
- GIVEN a history with an operation that lacks an identity
- WHEN the model validates the history
- THEN the core MUST reject the record with a typed error.

#### Scenario: Core purity
- GIVEN a direct call to the core
- WHEN the checker runs
- THEN it MUST perform no file, clock, or process access.

### Requirement: Phenomena are enumerated and checked

r[chaoscontrol.phenomena.checker] The checker MUST classify the named phenomena (aborted read, intermediate read, garbage read, stale read, lost write, write cycle) using dependency-graph cycle detection, and MUST attach the responsible operations to each violation.

#### Scenario: Aborted read detected
- GIVEN a history in which a committed read observes an aborted write
- WHEN the checker runs
- THEN it MUST emit an aborted-read violation with the read and write operations attached.

#### Scenario: Write cycle detected
- GIVEN a history with a dependency cycle purely over write-write edges
- WHEN the checker runs
- THEN it MUST emit a write-cycle violation with the cycle operations attached.

#### Scenario: Clean history
- GIVEN a history with no listed phenomenon
- WHEN the checker runs
- THEN it MUST emit no violations.

### Requirement: Incomplete history is bounded

r[chaoscontrol.phenomena.incomplete] When observation gaps prevent classification, the checker MUST return a bounded insufficient-data result and MUST not invent a violation.

#### Scenario: Gaps hide ordering
- GIVEN a history whose operation ordering is unknown for a pair
- WHEN the checker cannot classify
- THEN it MUST report the insufficient-data result with the affected pair.

### Requirement: Core and shell stay separated

r[chaoscontrol.phenomena.boundary] The core MUST return typed violations from typed histories. The shell MUST ingest round and log artifacts, assemble histories, and validate history identities.

#### Scenario: Core reads no artifacts
- GIVEN a direct call to the core
- WHEN the checker runs
- THEN it MUST perform no file, clock, or process access.

### Requirement: Phenomena evidence binds to histories

r[chaoscontrol.phenomena.evidence_binding] Phenomena evidence MUST bind to the history identity and the attached operation records with BLAKE3 identities, and MUST fail closed on identity drift.

#### Scenario: History identity drifts
- GIVEN evidence whose history identity differs from the assembled records
- WHEN receipt validation runs
- THEN validation MUST fail closed.

### Requirement: Phenomena validation is adversarial

r[chaoscontrol.phenomena.validation] Validation MUST pair positive fixtures for each named phenomenon with negative fixtures for clean histories and incomplete histories, and MUST verify that rejected records are typed.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to treat phenomena results as diagnosis evidence
- WHEN core, shell, receipt, and lifecycle validation runs
- THEN every positive and negative class MUST produce its expected result.
