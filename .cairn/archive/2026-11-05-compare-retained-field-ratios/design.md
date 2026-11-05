## Context

The upstream map/filter example computes Number(used)/Number(total). Local derived values are strings; canonical kind/text/process/source_kind names take precedence. FieldNumber already establishes full JSON-number grammar and finite binary64 thresholds.

## Decision

Add EventPredicate::FieldRatio { numerator: String, denominator: String, comparison: NumberComparison, value: serde_json::Number }. Validate both names using existing nonempty/control-free/256-byte rules and the finite numeric threshold before evaluation, including empty input and skipped branches. Reuse field_value, serde_json::Number parsing and NumberComparison::matches; no changes to older predicates or receipt schemas. Upgraded readers are required for the variant.

Use an arm-local numeric lookup closure with the same complete JSON-number parser as FieldNumber, finite as_f64 conversion and no string clone/object parsing. Resolve the denominator first; missing/malformed/nonfinite or positive/negative zero is a nonmatch without unnecessary numerator parsing. Resolve the numerator and divide only if both are finite and the denominator is nonzero. Number::from_f64 rejects overflowing quotients. Signed-zero quotients and underflow remain finite and follow the shared comparisons; negative operands are valid. No relation, including NotEqual, matches an undefined ratio. Ordinary Not complements nonmatches. Do not cross-multiply: doing so would change rounding and overflow behavior.

The predicate neither writes projections nor retains stale results. Existing All/Any/Not short-circuiting, regex cache, node/depth accounting and row/child logical-operation charges apply; no new cache or work charge. Numeric text is bounded by admitted canonical text/aggregate derived limits. Source admission, canonical sorting/deduplication, prefix truncation, serialized result bounds, moments and complete-expression/canonical-source/result receipts remain unchanged. A quotient is a bounded local observation, not evidence of causal history or upstream coercion equivalence.

## Verification

Capture replay baseline and EventPredicate references; admit proposal/design/tasks before implementation. Reproduce unsupported FieldRatio with full-envelope six-relation regressions. Verify operand syntax/missing/type errors, zero/nonfinite denominator and overflowing quotient, negative/zero/underflow/rounding cases, Not versus NotEqual, canonical-name precedence, Map/MapText and RunningCount composition, both-name/threshold admission including skipped branches, UTF-8/derived-byte boundaries, shared operation/structural/scan/result bounds, moments and expression/source/result drift. Exercise the real CLI for complete expected rows, receipts and failure artifact absence. Run replay tests, Clippy, workspace compilation, formatting, scope/evidence checks and scoped gates; sync the requirement while keeping the change active. Preserve/exclude the user's pointer regression. No KVM/replay claim.
