## Why

The Antithesis map/add_fields utilization example annotates each retained event with used/total before downstream filtering. FieldRatio supplies local selection but does not expose the quotient for display, Narrow or later FieldNumber filters.

## What Changes

Add EventQuery::MapRatio { input, field, numerator, denominator }. Reuse the existing finite binary64 quotient semantics and canonical/derived lookup. Compute before overwriting an operand alias; remove the destination on undefined division rather than preserve a stale prior ratio. Preserve canonical evidence and enforce existing field, intermediate envelope, structural, operation, result and receipt bounds.

## Capability and Ownership

Consumer: retained utilization triage through query-events. Outcome: inspect and reuse a computed utilization projection without general code execution. Owner: chaoscontrol-replay maintainers. Adoption: existing JSON queries can wrap Map/MapText/RunningCount inputs in MapRatio; old variants and schemas stay unchanged, while new queries require upgraded readers. Repeatability: behavioral regressions and actual CLI query/validate evidence.

## Non-Goals

No general arithmetic interpreter, JavaScript coercions, exact rational arithmetic, hosted service, complete capture, CPU/RSS/deadline guarantee, VM execution or replay proof. No changes to the user's uncommitted pointer test.
