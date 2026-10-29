## Context

Retained queries already admit canonical events, bound node/depth/work/result resources, propagate moments and bind all source/query/result facts in receipts. Window selects absolute ticks; it does not correlate two result sets.

## Decision

Temporal has independently queried candidate and witness inputs plus a typed four-way relation and u64 within_ticks. A preceding witness must satisfy c-within <= w < c; following is c < w <= c+within, using saturating endpoints. Same-tick events never witness each other; a zero window has no positive matches and negative forms retain every candidate. Any matching witness suffices, regardless of source/VM/process unless child queries restrict it. This is a relation over caller-supplied ticks, not causality or branch history.

Evaluate both inputs using existing bounds and the shared regex cache. Charge one logical temporal operation per candidate and per witness in addition to both child evaluations, then admit witness rows so invalid projected witness envelopes cannot disappear behind a semi-join. Sort only witnesses by tick in place with an allocation-free unstable sort. Binary-search the nearest strict witness for each candidate and retain candidates in their original order; this preserves the existing first-wins duplicate projection rule without extra row copies or tick arrays. For n candidates and m witnesses, correlation takes O(m log m + n log m) comparisons with no pairwise join or new limits. Logical work counts are not comparison counts or a deadline/RSS guarantee. Negative relations invert existence, including empty witness sets.

Validate both child expressions, including skipped/empty and invalid-regex witnesses. Moment propagation follows existing Union/UpTo rules: directly combined differing child moments are ambiguous; an explicit outer UpTo determines its reported moment and filters only the rows of the expression it wraps. It does not retroactively cap hidden witness inputs. Wrap the witness query itself when its history must be capped. Receipts bind relation/window/both expressions and every supplied source event, including non-emitted witnesses.

## Verification

Reproduce unknown Temporal deserialization. Regress all four relations against an independent bounded pairwise oracle over shuffled ticks, zero/exact/over windows, empty sets, equal ticks and u64 endpoints. Verify projections, regex witnesses, witness admission/errors, exact/exhausted logical work, structural/result bounds, moment/window scoping and query/source/result receipt drift. Exercise actual CLI positive/negative queries and absence of partial artifacts. Run replay tests, Clippy, workspace compilation, formatting, Nickel product-scope/evidence checks and Cairn gates. Preserve the user's pointer test and report unrelated traceability failures.
