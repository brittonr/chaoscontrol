# Diagnostic Optional Progress Specification

## Purpose

Defines the `diagnostic-optional-progress` capability.

## Requirements

### Requirement: Diagnostic progress respects optional execution

r[chaoscontrol.diagnostic_optional.progress]

The SDK diagnostic MUST NOT require a hit for explicit optional reachability declarations or a success for never-executed optional sometimes declarations. It MUST retain all sites and observation counts, required or unknown-metadata progress gaps, and observed optional violations. Missing must-hit metadata MUST stay conservative; malformed and conflicting explicit metadata MUST be rejected. This diagnostic MUST remain non-collision-safe and MUST NOT authorize promotion.

#### Scenario: Quiet forbidden path

- GIVEN a setup-complete local output with an unexecuted unreachable declaration and `must_hit=false`
- WHEN the SDK diagnostic is parsed
- THEN no reachable-progress gap is reported, and the site remains unobserved with zero counts

#### Scenario: Required progress and optional failures

- GIVEN required or unknown-metadata quiet progress sites, or executed optional sites without a successful observation
- WHEN the diagnostic is parsed
- THEN required progress gaps and actual optional violations or unsuccessful sometimes progress remain visible

#### Scenario: Metadata cannot erase required progress

- GIVEN malformed must-hit metadata or contradictory explicit values for a site
- WHEN the diagnostic is parsed
- THEN parsing rejects the document in either record order
