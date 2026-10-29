# Noop Assertion Details Specification

## Purpose

Defines the `noop-assertion-details` capability.

## Requirements

### Requirement: Optional always assertions preserve no-op detail calls

r[chaoscontrol.noop_assertion_details.optional]

Without `full`, optional always assertions MUST accept borrowed details of arbitrary sized or unsized types without serialization, cloning, allocation or a standard-library dependency. Their condition and details expressions MUST execute exactly once, condition first, for either boolean outcome. Details remain borrowed. Disabling instrumentation MUST NOT produce local output or change the const-evaluable-message contract. Full-mode serialization, optional must-hit semantics and assertion identity MUST remain unchanged.

#### Scenario: A workload disables instrumentation

- GIVEN an optional always assertion with borrowed details and observable condition/detail side effects
- WHEN the workload builds and runs without `full`
- THEN the same detail-bearing call shape compiles, including unsized references, its inputs execute once in order and no SDK output is created

#### Scenario: Instrumentation remains enabled

- GIVEN the same serializable detail-bearing callsites with `full`
- WHEN the workload runs with local output enabled
- THEN observations retain their details, `AlwaysOrUnreachable` display and optional must-hit behavior
