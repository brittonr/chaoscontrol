# Supervisor Diagnostic Execution Specification

## Purpose

Defines the `supervisor-diagnostic-execution` capability.

## Requirements

### Requirement: The supervisor executes only identity-matching admitted requests

r[chaoscontrol.guest_diagnostic.supervisor_execution]

Execution MUST admit the request, read the supervisor-chosen executable within a
16 MiB bound, verify its BLAKE3 identity against the request before spawning, run it
with null stdin, enforce the request timeout by killing and reaping the process, and
capture at most 64 KiB per stream. The outcome MUST report the canonical request
digest, completion or timeout, the exit code when present, and output byte counts and
digests. Execution MUST NOT claim executable trust, output correctness, or receipt
completeness.

#### Scenario: Matching executable runs
- GIVEN an admitted request whose identity matches the executable bytes
- WHEN the runner executes it
- THEN it reports completion, the exit code, and bounded output facts

#### Scenario: Identity mismatch is rejected
- GIVEN an executable whose bytes differ from the requested identity
- WHEN the runner starts
- THEN it rejects the request before spawning

#### Scenario: Timeout kills the process
- GIVEN a request whose timeout expires
- WHEN the runner is waiting
- THEN it kills and reaps the process and reports timeout

### Requirement: Diagnostic outcomes bind into canonical receipts

r[chaoscontrol.guest_diagnostic.receipt_binding]

An execution outcome MUST convert into a versioned diagnostic receipt carrying the
request digest, executable identity, termination class, optional exit code,
truncation flag, and each stream's captured length and digest. Admission MUST reject
unsupported receipt versions, unknown status or presence bytes, oversized captured
lengths, and malformed or trailing encodings. A receipt identifies bounded
observations; it does not prove executable trust, output correctness, or replay
fidelity.

#### Scenario: Receipt identifies bounded output
- GIVEN a completed execution
- WHEN its receipt is encoded
- THEN it roundtrips with the request digest, executable identity, exit code, and output digests

#### Scenario: Invalid receipt is rejected
- GIVEN a receipt with an unknown status byte or oversized length
- WHEN it is decoded or encoded
- THEN the matching rejection is returned
