## Why

Antithesis event-set filters accept arbitrary functions, including compound boolean conditions. ChaosControl has typed atomic predicates and can combine some positive selections with nested filters/unions, but cannot negate an arbitrary predicate or express boolean expressions in one filter. Operators inspecting high-memory observations need to combine numeric, VM and process conditions without duplicating query branches.

## What Changes

Add `EventPredicate::All`, `Any`, and `Not`. Evaluate children left-to-right with short-circuiting. Empty All is true and empty Any is false. Validate every branch before evaluation, even when short-circuiting would skip it. Share the existing query node/depth limits and operation budget; the existing Filter node/root predicate charge is unchanged, and descendant predicate nodes/evaluations consume additional budget. Preserve flat-query admission, canonical rows and receipt semantics.

## Impact

Owner: replay/query maintainers. Current consumer: `query-events` operators analyzing structured memory observations across selected VMs while excluding maintenance processes. Immediate outcome: bounded negation and compound selection. Durable capability: reusable pure predicate composition over retained evidence without arbitrary-code execution. Existing atomic predicates, limits fields and receipt schemas stay unchanged. New variants require upgraded readers. This is not full Antithesis Notebook compatibility, reactive branching, complete capture, VM/replay proof or release readiness.
