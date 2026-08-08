# Sdk Parity And Guidance Specification

## Purpose

Defines the `sdk-parity-and-guidance` capability.

## Requirements

### Requirement: Parity mapping is tracked

r[chaoscontrol.sdk_parity.mapping] ChaosControl MUST keep a versioned parity mapping that records, for each `chaoscontrol-sdk` surface, the matching `antithesis_sdk` surface and a status (equivalent, superset, subset, divergent, or absent), and MUST include the compared SDK versions and the review date.

#### Scenario: New SDK surface is added
- GIVEN a new assertion, random, lifecycle, coverage, or transport surface is added to `chaoscontrol-sdk`
- WHEN the parity mapping is reviewed
- THEN the mapping MUST be updated with the matching Antithesis surface and status.

#### Scenario: Parity entry is reviewed
- GIVEN the parity document
- WHEN a reader checks one entry
- THEN the entry MUST name both SDK symbols, give one status code, and sit under a clear surface heading.

### Requirement: Parity is reference-only

r[chaoscontrol.sdk_parity.reference] The parity mapping MUST NOT create a requirement that ChaosControl match any Antithesis surface, and MUST mark surfaces that exist in only one SDK without implying a parity obligation.

#### Scenario: Antithesis-only surface
- GIVEN a feature exists in `antithesis_sdk` but not in `chaoscontrol-sdk`
- WHEN the mapping is read
- THEN the entry MUST be marked Antithesis-only and MUST NOT state a ChaosControl obligation.

### Requirement: Guidance watermarks have a recorded decision

r[chaoscontrol.sdk_parity.guidance] ChaosControl MUST record a decision on Antithesis guidance watermarks (numeric and boolean watermark reporting) that states whether they are a current requirement, with rationale, and that marks them MAY-level for future explorer work unless reopened.

#### Scenario: Decision is reviewed
- GIVEN the guidance entry in the parity document
- WHEN the decision is read
- THEN it MUST give the status (not a current requirement) and the rationale (reference-only boundary, existing failure details, and `record_state` coverage).

### Requirement: Local output schema relationship is documented

r[chaoscontrol.sdk_parity.local_schema] The parity mapping MUST state that ChaosControl local output is a superset of the Antithesis fallback schema and MUST name the added `chaoscontrol_*` records and identity fields.

#### Scenario: Caller validates local output
- GIVEN a caller reads the local output section
- WHEN they check schema compatibility
- THEN they MUST be told the output has extra fields beyond the Antithesis fallback schema.
