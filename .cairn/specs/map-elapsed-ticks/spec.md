# Map Elapsed Ticks Specification

## Purpose

Defines the `map-elapsed-ticks` capability.

## Requirements

### Requirement: Retained queries project exact elapsed ticks

r[chaoscontrol.map_elapsed_ticks.projection]

MapElapsedTicks MUST validate input and both field names even on empty sources. For every materialized child row it MUST charge one operation beyond child work, admit the original envelope, read since_field using canonical-name precedence and complete JSON u64 integer parsing, and subtract that value from the canonical tick with checked u64 arithmetic before overwriting output. Defined durations MUST be exact unsigned decimal text, including zero, adjacent ticks above 2^53 and the maximal span. Missing/malformed/signed/fractional/exponent/noninteger/out-of-range/future since values MUST retain the row while removing stale destination output. Aliases MUST read original values. Canonical evidence and other projections MUST remain unchanged. Immediate generated/intermediate capacity, existing shared structural/scan/work/derived/result/prefix/moment bounds and complete query/source/result receipt binding MUST apply without partial evaluation error artifacts. Exact duration projection MUST NOT imply tick-unit conversion, inferred history coherence or exact downstream binary64 arithmetic.

#### Scenario: Preserve exact duration boundaries

- GIVEN canonical ticks and complete unsigned JSON prior ticks across u64
- WHEN MapElapsedTicks subtracts an equal, earlier, adjacent-wide or zero prior tick
- THEN the exact nonnegative duration is retained without floating-point conversion

#### Scenario: Remove undefined stale durations safely

- GIVEN missing/invalid/out-of-range or future since values, aliases and payload shadows
- WHEN elapsed duration is recomputed
- THEN stale output is removed without dropping evidence, while defined aliases use original values and canonical tick metadata

#### Scenario: Compose bounded reproducible elapsed evidence

- GIVEN projected past-witness ticks, JSON projections, empty/name/child admission, capacity/shared limits, moments and receipts
- WHEN query composition runs or facts drift
- THEN valid exact durations compose with selection/narrowing and invalid admission/work/capacity/moments/receipts reject
