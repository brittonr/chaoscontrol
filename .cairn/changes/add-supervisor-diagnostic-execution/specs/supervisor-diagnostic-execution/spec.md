# supervisor-diagnostic-execution Specification

## ADDED Requirements

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

### Requirement: The host queues admitted requests and records reported receipts

r[chaoscontrol.guest_diagnostic.host_dispatch]

The fault engine MUST bound and admit canonical diagnostic requests, serve at most
one per supervisor poll with its payload, and retain accepted canonical receipts
within fixed bounds. Requests and receipts MUST survive engine snapshot and restore,
and restore MUST reject queues that exceed bounds or contain malformed payloads.
Polls preserve order and never fabricate an empty request.

#### Scenario: Request served once
- GIVEN one admitted request
- WHEN the supervisor polls
- THEN the payload decodes to that request and the queue is empty

#### Scenario: Invalid payload rejected
- GIVEN a request or receipt outside admission
- WHEN it is queued or recorded
- THEN the queue rejects it without changing bounded state

#### Scenario: Snapshot preserves queues
- GIVEN pending requests and accepted receipts
- WHEN the engine snapshot is restored
- THEN the same pending requests and receipts remain

### Requirement: The guest client polls, executes, and reports one request

r[chaoscontrol.guest_diagnostic.guest_client]

The SDK MUST poll the host diagnostic queue, decode an admitted request or report
absence, execute it with the supervisor-supplied path, encode the outcome receipt,
and report it with the report command. Failed polls, malformed payloads, execution
errors, and rejected reports MUST surface as errors; the client MUST NOT fabricate
a receipt or claim execution success.

#### Scenario: Empty queue
- GIVEN no queued request or no VM transport
- WHEN the client polls
- THEN it returns no request and reports nothing

#### Scenario: One request completes the loop
- GIVEN one queued admitted request and a matching executable
- WHEN the client serves it
- THEN it reports the canonical receipt and returns it

### Requirement: Captured diagnostic output can be recorded as content-addressed artifacts

r[chaoscontrol.guest_diagnostic.diagnostic_artifacts]

The supervisor runner MUST offer an artifact-recording mode that writes each
non-empty captured stream under a caller-supplied directory as
`blake3-<hex>.stdout` or `.stderr`, creating the directory if missing and bounded by
the capture limits. The receipt MUST continue to report only lengths and digests.
Recording failures MUST surface as errors; no artifact identity is claimed without a
written file.

#### Scenario: Artifact recorded
- GIVEN captured output and a writable directory
- WHEN the recording mode runs
- THEN the file name matches the stream digest and its contents equal the captured bytes

#### Scenario: Empty stream writes nothing
- GIVEN a stream with no bytes
- WHEN the recording mode runs
- THEN no file is written for that stream
