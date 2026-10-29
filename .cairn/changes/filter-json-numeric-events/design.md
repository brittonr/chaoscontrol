## Context

`Filter` validates its predicate before scanning and charges one operation per row. `Map` already admits bounded JSON pointers against `EventEnvelope.details`. The current numeric scalars are converted to strings only for derived fields; adding numeric behavior to text matching would silently change existing semantics.

## Decisions

Add a separate typed `JsonNumber` predicate and `NumberComparison` enum in the existing pure event-query module. A `serde_json::Number` threshold preserves the query's equality/serialization derives and disallows NaN/infinity without special encodings. Validate the pointer with the existing confinement/byte bound and validate finite threshold conversion. Resolve `details.pointer(pointer).as_f64()` without cloning or allocating derived fields. Compare both operands as finite binary64, as in the upstream JavaScript example. Integers outside binary64's exact range may round; no exact-integer claim. Keep comparison operators exhaustive and explicit. Missing/non-numeric values do not coerce or match. Existing query budgets, ordering, source/digest binding, canonical fields and receipt validation apply without a new execution path.

## Verification

Run existing query tests before editing. Keep regression coverage for strict/inclusive/equal comparisons, negative/fractional thresholds, missing and wrong types, escaped pointers, invalid/overlong pointers, operation-bound enforcement, canonical event preservation, query round-trip and receipt threshold drift. First demonstrate the unsupported query through deserialization before adding the variant. Exercise actual `query-events query` and `validate` on structured memory observations, then reject threshold/result drift and malformed queries. No VM execution is required or claimed.
