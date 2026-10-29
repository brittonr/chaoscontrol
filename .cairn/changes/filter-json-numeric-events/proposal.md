## Why

The Antithesis event-set guide demonstrates filtering structured memory observations with `mem_usage > 50`. ChaosControl can map JSON scalars into string fields and compare text but cannot filter a JSON number numerically. Lexical string comparisons would produce incorrect threshold results.

## What Changes

Add `EventPredicate::JsonNumber` with a bounded JSON pointer, a typed comparison (`LessThan`, `LessThanOrEqual`, `Equal`, `GreaterThanOrEqual`, `GreaterThan`) and a JSON-number threshold. Select actual JSON numbers only; missing paths, strings, booleans, null, arrays and objects do not match. Compare as finite IEEE-754 binary64 numbers, matching the guide's JavaScript Number model rather than claiming exact arbitrary-width integer comparison. Preserve canonical event data, existing operation/resource limits and receipt re-evaluation.

## Impact

Owner: ChaosControl replay/query maintainers. Current consumer: operators diagnosing structured workload or resource observations with `query-events`. Immediate outcome: local numeric-threshold queries. Durable capability: a reusable typed numeric predicate over retained JSON evidence. Existing query forms and receipt schemas remain unchanged; readers must upgrade to accept queries using the new variant. This is retained-input query evidence, not live branching, hosted Notebook compatibility, complete capture, execution/replay proof or release readiness.
