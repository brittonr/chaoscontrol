# Fallback Assertion Transport Specification

## Purpose

Defines the `fallback-assertion-transport` capability.

## Requirements

### Requirement: Fallback records are language-agnostic

r[chaoscontrol.fallback_assertion_transport.record_format] A fallback record MUST be a versioned, self-contained line that carries a type, a stable logical key, a condition result where applicable, a message, and a mandatory process identity.

#### Scenario: Valid fallback record
- GIVEN a well-formed fallback record from an uninstrumented process
- WHEN the oracle ingests it
- THEN the record MUST enter the assertion catalog under its derived identity.

#### Scenario: Malformed record
- GIVEN a fallback record with a missing process identity or field
- WHEN the oracle ingests it
- THEN ingestion MUST reject it with a typed diagnostic naming the record and process.

### Requirement: Ingestion order is deterministic

r[chaoscontrol.fallback_assertion_transport.deterministic_ingestion] Fallback records MUST be ingested in declared sink order, and record order MUST be part of replay identity.

#### Scenario: Replayed sink order
- GIVEN two identical runs with identical record sequences
- WHEN replay validates the sinks
- THEN the record order MUST match.

#### Scenario: Reordered sink
- GIVEN evidence whose record order differs from the executed sink
- WHEN replay validation runs
- THEN replay MUST fail closed.

### Requirement: Sinks are bounded

r[chaoscontrol.fallback_assertion_transport.bounded_sink] A fallback sink MUST enforce an admitted record bound and MUST emit a typed overflow event instead of dropping records silently.

#### Scenario: Sink overflows
- GIVEN a process that emits more records than the admitted bound
- WHEN the bound is reached
- THEN an overflow event MUST be recorded and the sink MUST stay valid.

### Requirement: Identity conflicts fail the catalog

r[chaoscontrol.fallback_assertion_transport.identity_conflict] A fallback record whose derived identity conflicts with an existing catalog entry MUST produce a typed catalog event and MUST NOT be silently accepted.

#### Scenario: Conflicting key
- GIVEN a fallback record whose stable key conflicts with an SDK-sourced entry
- WHEN the catalog validates
- THEN the conflict MUST be recorded with both identities.

### Requirement: Fallback evidence is process-scoped

r[chaoscontrol.fallback_assertion_transport.evidence_scope] Bug reports and replay verdicts that include fallback records MUST record the owning process identity and MUST NOT promote a process-local fact into a whole-guest claim.

#### Scenario: Failure attribution
- GIVEN an assertion failure from a fallback record
- WHEN the bug report is produced
- THEN the report MUST name the owning process and the record identity.

### Requirement: Fallback validation is adversarial

r[chaoscontrol.fallback_assertion_transport.validation] Validation MUST pair a positive ingestion fixture with negative fixtures for malformed records, identity conflicts, sink overflow, reordering, and process-scope overclaims.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to admit the fallback path
- WHEN protocol, oracle, replay, and lifecycle validation runs
- THEN every positive and negative class MUST produce its expected result.
