# Guest Diagnostic Request Contract Specification

## Purpose

Defines the `guest-diagnostic-request-contract` capability.

## Requirements

### Requirement: Diagnostic requests are versioned and bounded

r[chaoscontrol.guest_diagnostic.request_admission]

The protocol MUST define one diagnostic request command and a canonical encoding for
protocol version, a 32-byte BLAKE3 executable identity, 1-16 non-empty UTF-8
arguments, and a timeout. Admission MUST reject unsupported versions, a zero
identity, missing or too many arguments, empty or oversized arguments, an oversized
argument total, encodings larger than the hypercall payload, timeouts outside
1-30000 ms, malformed or truncated encodings, and trailing bytes.

#### Scenario: Admitted request roundtrips
- GIVEN a request within every limit
- WHEN it is encoded and decoded
- THEN the decoded request equals the admitted request

#### Scenario: Rejected requests
- GIVEN a request outside any limit or a malformed encoding
- WHEN it is admitted, encoded, or decoded
- THEN the matching rejection is returned
