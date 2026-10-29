## Context

`EventQuery::Filter` currently validates one atomic `EventPredicate`, charges one operation per candidate row and filters an owned vector. Query admission already bounds nodes/depth across the expression. Receipts bind query identity and re-evaluate supplied results.

## Decisions

Keep composition in the existing predicate enum and evaluator. `All(Vec<EventPredicate>)`, `Any(Vec<EventPredicate>)`, and `Not(Box<EventPredicate>)` are serde-typed expressions, not arbitrary functions. Validate all descendants using the shared query node counter and depth; a Filter represents its predicate root so existing atomic predicates retain exactly their historical structural cost. Each descendant adds a node and a depth step, including descendants beneath nested query Filters. Empty conjunction/disjunction use their standard identities. Reject invalid descendants even in an unreachable short-circuit branch or an empty source.

Make predicate matching fallible to propagate operation exhaustion. The Filter still precharges its root evaluation per row. Each visited child costs one additional operation; skipped children cost none. All/Any stop at the first false/true child; Not evaluates exactly one child. Use an owned iterator with fallible filtering to retain ordering and avoid cloning event payloads or evaluating later rows after budget failure. Leaf semantics, numeric binary64 behavior and canonical source fields remain unchanged. No new budget knobs, schema versions, aliases or alternate evaluator.

## Verification

Capture baseline query tests and a failing-before serialized compound consumer. Permanent tests cover nested boolean truth behavior, VM/process/numeric selection, empty identities, double negation, non-numeric negation, malformed skipped descendants, exact/over node and depth limits, cumulative query/predicate budgets, exact/insufficient operation limits, short-circuiting and query-bound receipts even when different expressions select identical rows. Run real query/validate CLI scenarios and reject malformed, over-budget and drifted inputs. Compile workspace callers; run replay tests, Clippy, formatting, product/evidence contracts and scoped Cairn gates. Preserve the user's unrelated pointer test in the worktree and exclude it from staging.
