## Why

Antithesis's storage-utilization example divides two event fields before filtering. Local Map/MapText can expose both fields, and FieldNumber compares each separately, but no retained predicate can compare used/total against a threshold.

## What Changes

Add FieldRatio { numerator, denominator, comparison, value } over existing canonical/derived string lookup. Parse finite JSON-number text using the FieldNumber convention, divide in binary64, and reuse all six comparisons. Missing/nonnumeric operands, zero denominators and nonfinite quotients never match, including NotEqual; Not remains ordinary complement. No source mutation or general expression engine.

## Impact

Owner: chaoscontrol-replay maintainers. Consumer: retained memory/storage utilization triage using projected used/total observations. Adoption: compose Map/MapText with FieldRatio through EventQuery or query-events JSON. Durable capability: bounded two-field fractional threshold filtering. This is a local analogue of the upstream example, not JavaScript Number coercion, ratio projection, general arithmetic, exact rational comparison, hosted search, complete capture, VM execution or replay proof.
