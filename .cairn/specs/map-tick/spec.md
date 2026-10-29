# Map Tick Specification

## Purpose

Defines the `map-tick` capability.

## Requirements

### Requirement: Retained queries project exact canonical event ticks

r[chaoscontrol.map_tick.projection]

MapTick MUST validate its destination name and child even on empty sources. For each materialized child row it MUST charge one operation beyond child work, admit the original envelope and annotate the destination with the exact canonical u64 tick in unsigned decimal text without floating-point conversion. Derived/details/text shadows MUST NOT replace the canonical tick. Existing output MUST be replaced; canonical evidence and all other projections MUST remain unchanged, including when the destination is a canonical lookup name. Every generated envelope MUST be admitted before downstream narrowing/filtering/deduplication. Existing structural/scan/work/derived/result-byte/prefix/moment and complete query/source/result receipt rules MUST apply without partial evaluation error artifacts. Exact tick text MUST NOT imply exact downstream binary64 subtraction or an automatic time-unit/history conversion.

#### Scenario: Retain canonical time metadata exactly

- GIVEN zero, tied, wide and maximal u64 ticks with stale or shadowing payloads
- WHEN MapTick projects a named field
- THEN every retained row has the exact canonical decimal tick while source evidence remains unchanged

#### Scenario: Compose bounded elapsed-time annotations

- GIVEN candidate and past-witness tick projections in a coherent history
- WHEN WithLastValue copies witness time and MapDifference plus numeric selection/Narrow computes elapsed observations
- THEN actual canonical ticks drive the result, missing witnesses clear stale context and existing binary64 precision limits remain explicit

#### Scenario: Preserve bounded reproducible tick projections

- GIVEN invalid names, hidden intermediate/generated overflow, shared limits, child moments and receipts
- WHEN admission/work/capacity fails, moments conflict or query/source/result facts drift
- THEN evaluation or validation rejects while valid projections remain reproducible
