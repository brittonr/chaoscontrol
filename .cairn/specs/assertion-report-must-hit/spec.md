# Assertion Report Must Hit Specification

## Purpose

Defines the `assertion-report-must-hit` capability.

## Requirements

### Requirement: Report must-hit metadata

r[chaoscontrol.assertion_reports.must_hit] Assertion report producers MUST preserve known boolean `must_hit` metadata from oracle records. Unknown metadata MUST remain absent and historical reports without this field MUST remain readable.

#### Scenario: Known and historical metadata

- GIVEN an assertion with known must-hit metadata or a historical record without it
- WHEN an exploration or strict summary is emitted and read
- THEN known metadata is retained and absence retains historical semantics.

### Requirement: Consistent must-hit verdicts

r[chaoscontrol.assertion_reports.must_hit_verdict] Rust and Nickel assertion-summary validators MUST derive zero-hit always, sometimes and reachable verdicts as failed for `must_hit = true`, passed for `false`, and unexercised when absent. Unreachable and observed-hit semantics MUST remain unchanged. Malformed metadata and contradictory verdicts MUST be rejected.

#### Scenario: Zero-hit and observed assertions

- GIVEN valid counters and optional boolean must-hit metadata
- WHEN a summary is validated
- THEN its verdict agrees with the oracle and observed failures cannot be hidden by metadata.

### Requirement: Campaign metadata agreement

r[chaoscontrol.assertion_reports.must_hit_conflict] Campaign aggregation MUST reject assertion details whose must-hit metadata differs for the same identity. Compatible details MUST retain the metadata when counters and verdicts are merged.

#### Scenario: Inconsistent seeds

- GIVEN reports with the same assertion identity but differing must-hit metadata
- WHEN a campaign is aggregated
- THEN the conflict is diagnostic and cannot become accepted assertion evidence.
