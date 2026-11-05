## Context

MapText already parses canonical event text under MAX_EVENT_TEXT_BYTES and serde_json's default recursion bound. JsonNumber already validates confined pointers and finite thresholds and compares finite binary64 values without string coercion. Compound predicates return Result and charge visited children left-to-right.

## Decision

Add JsonTextNumber { pointer, comparison, value } to EventPredicate. Reuse pointer/threshold admission and a private shared numeric comparison implementation. Parse text per visited leaf only; malformed/trailing/deep JSON propagates InvalidJsonText. Missing or nonnumeric selected values are false, including under normal Not semantics. All/Any short-circuiting may skip parsing but never skips structural admission of descendants. Input rows remain unmodified. A numeric text leaf costs the same logical operation as JsonNumber; the existing text-byte cap bounds each parse. No derived-string coercion or persistent parsed cache.

## Verification

Reproduce unsupported deserialization before implementation. Keep positive/negative regressions for all comparisons, source separation, missing/non-number values, pointer escaping, binary64 rounding, parsing failures and text bounds, short-circuit error propagation, skipped-invalid admission, exact/exhausted work and structural bounds, and query/source/result receipt drift. Exercise actual query/validate CLI commands, including the upstream-style process projection plus numeric threshold and UpTo. Run replay tests, Clippy, workspace compilation, formatting, product-scope/evidence checks and scoped Cairn gates. Report unrelated traceability failures rather than altering unrelated requirements.
