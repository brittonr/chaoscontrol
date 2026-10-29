## Context

Canonical and derived field resolution and compound predicate admission already exist. The workspace lockfile already contains regex 1.13.1 through existing consumers. Regex must not compile per row or mutate global state.

## Decision

Add Regex { field, pattern } and InvalidRegex. Reuse nonempty/control-free 256-byte field and 8192-byte pattern admission. Compile with Rust regex::RegexBuilder using 256 KiB program size, 256 KiB lazy-DFA cache and nesting depth 64; explicit named constants cap expansion and parser work rather than accepting engine defaults. These engine bounds are not total process RSS or a wall-clock deadline. Use a query-local BTreeMap borrowing pattern strings, populated by existing recursive validation; identical patterns share a compiled value across fields and branches. Thread the immutable cache through query/predicate evaluation. Old queries create an empty nonallocating map. Receipt construction independently admits patterns because it is a separate public entry point. No persistent cache, new public limits or regex-specific serialized state.

Default matching is case-sensitive Unicode substring search. Anchors and Rust inline flags are supported; backreferences/look-around are rejected by the non-backtracking engine. Missing fields are false and Not follows existing negation, including missing fields. Canonical names retain precedence over derived names. Admission visits every descendant before evaluation, irrespective of short-circuiting or source emptiness; compilation failure/expansion/nesting excess returns InvalidRegex. Logical per-row/visited-child operation charges remain unchanged and are not engine instruction counts.

## Verification

Reproduce unsupported query deserialization. Regress regex versus literal/exact selection, anchors, alternation/classes, Unicode, canonical precedence, derived projection, missing/empty fields, negation/compound branches, invalid syntax/unsupported constructs/expansion/nesting/field-pattern byte bounds even in skipped/empty queries, exact/exhausted operation and structural bounds, moments, repeated patterns and receipt drift. Exercise actual CLI query/validate positive and negative cases, including no partial artifacts. Run replay tests, Clippy, workspace compilation, formatting, Nickel product-scope/evidence checks and Cairn gates. Preserve the pre-existing user pointer test; report unrelated global traceability failures.
