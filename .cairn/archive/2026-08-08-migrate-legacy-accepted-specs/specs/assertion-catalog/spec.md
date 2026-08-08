# Assertion Catalog Specification

## Purpose

Defines compile-time assertion registry and runtime catalog transmission to enable coverage analysis of unexercised property assertions in ChaosControl guests.

## Requirements
### Requirement: Compile-time Assertion Registry

The SDK MUST maintain a compile-time catalog of all assertion declarations using distributed slice collection.

#### Scenario: Assertion macro registration

- GIVEN an assertion macro is expanded in guest code
- WHEN the guest binary is compiled
- THEN a static catalog entry MUST be generated containing assertion ID, message, type, file, and line number

### Requirement: Catalog Transmission

The SDK MUST transmit the complete assertion catalog to the VMM during guest initialization.

#### Scenario: Guest startup catalog send

- GIVEN a guest binary contains assertion catalog entries
- WHEN the guest reaches setup_complete phase
- THEN the catalog MUST be serialized and sent to VMM via CMD_SEND_CATALOG hypercall

#### Scenario: Empty catalog handling

- GIVEN a guest binary contains no assertions
- WHEN the guest reaches setup_complete phase  
- THEN an empty catalog MUST be transmitted to maintain protocol consistency

### Requirement: Oracle Pre-population

The PropertyOracle MUST pre-populate assertion records from the received catalog before guest execution begins.

#### Scenario: Catalog-based oracle initialization

- GIVEN the VMM receives a guest assertion catalog
- WHEN the PropertyOracle is initialized
- THEN assertion records MUST be created for all catalog entries marked as unexercised

### Requirement: Coverage Tracking

The PropertyOracle MUST distinguish between exercised and unexercised assertions in coverage reports, and the generated assertion-readiness surface MUST preserve gap evidence before any workload is promoted beyond bounded replay proof.

#### Scenario: Exercised assertion tracking

- GIVEN an assertion is registered in the catalog and fires during execution
- WHEN a coverage report is generated
- THEN the assertion MUST be marked as exercised with execution details

#### Scenario: Unexercised assertion reporting

- GIVEN an assertion is registered in the catalog but never fires
- WHEN a coverage report is generated
- THEN the assertion MUST be reported as unexercised with catalog metadata

#### Scenario: Assertion readiness gaps remain promotion blockers

- GIVEN an accepted workload proof has unhit, uncategorized, or non-passing assertion gaps
- WHEN assertion-readiness status or promotion checks are generated
- THEN the system MUST report those gaps as promotion blockers unless explicit workload-specific rationale is present
- AND the workload MUST NOT be described as richer-than-bounded replay support solely because its accepted proof exercised cataloged assertions

#### Scenario: Assertion readiness gap details identify remediation targets

- GIVEN an accepted workload proof has unhit or non-passing assertions
- WHEN assertion-readiness status is generated
- THEN the system MUST include deterministic gap details that identify the workload, gap class, assertion ID or message, kind, category, verdict, and hit count when those fields are present
- AND the details MUST be derived from committed accepted-proof assertion artifacts rather than fresh VM execution

#### Scenario: Accepted assertion category inference

- GIVEN a committed accepted-proof assertion artifact lacks category metadata for a known workload assertion
- WHEN assertion-readiness status is generated
- THEN the system MUST render a deterministic effective category for that assertion without modifying the committed artifact
- AND the gap detail MUST distinguish inferred categories from categories present in the artifact

#### Scenario: Unknown accepted assertion remains uncategorized

- GIVEN a committed accepted-proof assertion artifact lacks category metadata and has no deterministic category mapping
- WHEN assertion-readiness status and promotion checks are generated
- THEN the system MUST keep that assertion uncategorized so promotion remains fail-closed until metadata or explicit rationale exists

#### Scenario: Replay probes are checked proof signals, not instrumentation blockers [r[assertion-readiness.replay-probes-not-blockers]]

- GIVEN an accepted workload proof includes a non-passing assertion categorized as `replay-probe`
- WHEN assertion-readiness status and promotion checks are generated
- THEN the system MUST report that assertion as a replay-proof signal outside the ordinary non-passing instrumentation gap count
- AND the promotion checker MUST fail closed if the replay-probe signal count is omitted or weakened
- AND the report MUST preserve anti-claim text that replay-probe visibility is not product parity by itself

### Requirement: Assertion-readiness promotion gate

The static readiness surface MUST fail closed when assertion-readiness evidence is weakened, hidden, or promoted beyond the accepted workload's documented instrumentation state.

#### Scenario: Generated report preserves anti-claims

- GIVEN accepted workload proofs and their committed assertion artifacts
- WHEN the assertion-readiness report is generated or checked
- THEN it MUST preserve anti-claim text stating that assertion density is not replay proof or product parity by itself

#### Scenario: Gap removal fails closed

- GIVEN a workload has nonzero unhit, uncategorized, or non-passing assertion gaps
- WHEN a generated or checked assertion-readiness surface omits those gaps without explicit rationale
- THEN the promotion gate MUST exit nonzero and identify the workload and hidden gap class

#### Scenario: Promotion rationale is explicit

- GIVEN a workload is proposed for an instrumentation-readiness claim stronger than bounded replay proof
- WHEN assertion-readiness promotion is evaluated
- THEN the gate MUST require either zero relevant gaps or a checked workload-specific rationale for each remaining gap class

### Requirement: Backward Compatibility

The system MUST support guests compiled without assertion catalog capabilities.

#### Scenario: Legacy guest execution

- GIVEN a guest binary without catalog support
- WHEN the guest executes in the VMM
- THEN assertion tracking MUST function normally but without unexercised assertion reporting

### Requirement: No-std Compatibility

The assertion catalog implementation MUST work in no_std guest environments.

#### Scenario: No-std guest compilation

- GIVEN a guest binary compiled in no_std environment
- WHEN assertion macros are expanded
- THEN catalog entries MUST be generated without standard library dependencies
