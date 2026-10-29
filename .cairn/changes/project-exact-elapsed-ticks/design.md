## Context

MapTick preserves exact decimal u64 metadata; MapDifference is deliberately binary64, so adjacent large ticks can produce a zero delta. A typed duration operator closes this gap without changing existing numeric semantics or generalizing the map evaluator.

## Decision

Add MapElapsedTicks with input, destination field and since_field. Admit both names and the child even on empty sources. Evaluate the child, charge one operation per materialized child row and admit each original envelope. Read since_field using existing canonical-name precedence, parse serde_json::from_str::<u64> (complete unsigned JSON integer text, surrounding JSON whitespace permitted), then event.tick.checked_sub(since). Reject signed, fractional, exponent, quoted, malformed, missing and out-of-range operands as undefined; a future since is also undefined. No float or intermediate Value. Resolve and calculate before destination overwrite; aliasing since_field is safe. An undefined duration keeps the row but removes stale destination projection, rather than raising a query error or saturating to zero.

Serialize defined u64 durations directly as decimal text, reusing existing destination strings. Zero/equal ticks, adjacent ticks above 2^53 and u64::MAX minus zero remain exact. Canonical tick is the minuend regardless of payload shadows. Preserve canonical evidence and other projections; canonical selector precedence remains unchanged for destination aliases. Admit generated rows immediately so Narrow/Filter/deduplication cannot hide capacity errors; even a malformed intermediate row that overwrite could repair must fail. Integrate existing structural/scan/work/derived/result-byte/prefix/moment/receipt rules. Receipts bind input, both names and all canonical source/result facts. No new schema/error/lookup fallback; old queries remain unchanged and new readers are required.

Use MapTick on the witness branch of WithLastValue, copy that tick into candidates and apply MapElapsedTicks directly to candidates. Candidate MapTick is unnecessary because the operator reads canonical metadata. Exact output does not make FieldNumber thresholds or other downstream floating-point arithmetic exact across u64. Ticks remain in the caller-supplied domain; no causal or cross-history coherence is inferred.

## Verification

Capture baseline and exported references, admit gates, reproduce unsupported query. Compare full rows for integer grammar and all numeric boundaries, future/missing/invalid stale clearing, aliases/canonical precedence, JSON and near-max past-witness composition, names/empty children, intermediate/generated capacity including repairable overflow, shared limits/moments and query/source/result drift. Exercise actual CLI query/validate success and error artifacts. Run replay tests, Clippy, workspace compilation, formatting and scope/evidence checks. Synchronize the accepted requirement without archiving; stage only owned source hunks.
