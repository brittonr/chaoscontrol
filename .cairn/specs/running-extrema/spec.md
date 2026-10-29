# Running Extrema Specification

## Purpose

Defines the `running-extrema` capability.

## Requirements

### Requirement: Retained queries annotate finite numeric prefix extrema

r[chaoscontrol.running_extrema.prefixes]

RunningMin and RunningMax MUST admit both field names, the entire predicate and child query even on empty inputs. They MUST admit all intermediate rows, order by (tick,source_id,source_sequence), retain the first projection at each source position and retain every unique row. For each inclusive prefix they MUST annotate the finite binary64 minimum/maximum of matching value_field operands, using canonical-name lookup precedence before destination overwrite. Before the first match they MUST remove stale destination projections; no zero or infinite seed is permitted. Equal numeric values MUST retain the first canonical value, including signed zero. Missing/malformed/nonfinite matching operands MUST fail InvalidNumber; nonmatching operands MUST NOT be parsed. Other projections and canonical evidence MUST remain unchanged. Existing predicate semantics/work charging, structural/scan/derived/result bounds, immediate output admission, prefix truncation, moment reconciliation and receipt binding MUST apply without partial error artifacts.

#### Scenario: Retain meaningful extrema across prefixes

- GIVEN nonmatching prefixes followed by positive, negative or zero metric observations
- WHEN RunningMin or RunningMax annotates canonical unique-event prefixes
- THEN the destination is absent before a match and contains the inclusive extremum afterward, with first-position projections and stable equal-value priority

#### Scenario: Reject undefined matching values without parsing skipped values

- GIVEN malformed, missing or nonfinite operands and canonical-shadowed names
- WHEN matching observations contribute
- THEN undefined matching values fail, skipped values do not parse and input/destination aliases read before overwrite

#### Scenario: Preserve bounded reproducible query facts

- GIVEN query composition, intermediate/generated capacity, shared limits, moments and receipts
- WHEN admission/capacity/work bounds fail, moments conflict or operator/query/source/result facts drift
- THEN evaluation or validation rejects; valid canonical projections and receipts remain reproducible and evaluation errors publish no partial artifacts
