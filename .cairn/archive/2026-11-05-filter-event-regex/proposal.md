## Why

Antithesis Logs Explorer supports case-sensitive regex and negated regex search over event fields. ChaosControl has literal Contains and exact Matches selectors but cannot express bounded pattern searches over canonical or projected strings.

## What Changes

Add EventPredicate::Regex { field, pattern } with substring search by default, explicit anchors for whole-field matching and Not for negation. Use the already-locked Rust regex engine, compiling distinct patterns once during each query admission and reusing compiled values across all rows and branches. Preserve current field resolution, literal selectors, compound semantics and receipts.

## Impact

Owner: replay/query maintainers. Consumer: local retained-log triage selecting error codes, process families or projected labels by pattern. Durable capability: bounded, composable non-backtracking string pattern search without arbitrary code. Reject invalid/over-resource patterns even in skipped descendants and empty sources. New variants require upgraded readers; existing schemas and queries remain unchanged. Not upstream regex syntax/coercion parity, hosted Logs Explorer, complete capture, temporal search, VM proof or replay readiness.
