## Why

Antithesis Logs Explorer supports preceded-by, followed-by and their negative forms with independent witness conditions and time windows. ChaosControl can select tick ranges but cannot select retained events by nearby matching observations without external joins.

## What Changes

Add Temporal { input, witness, relation, within_ticks } and TemporalRelation with PrecededBy, NotPrecededBy, FollowedBy and NotFollowedBy. Use strict earlier/later ticks with inclusive outer edges and overflow-safe bounds. Evaluate both bounded child queries, sort witnesses in place and binary-search them while retaining original candidate order and first-wins duplicate projections, without a Cartesian product or extra tick arrays. Emit only unchanged candidate rows.

## Impact

Owner: replay/query maintainers. Consumer: operators correlating errors with faults/restarts or requests with missing acknowledgements in retained evidence. Durable capability: reusable bounded temporal semi/anti-joins over the existing query language. Windows are simulation ticks, not seconds. Callers supply a coherent tick domain; no causal inference, automatic VM/process partition, branch-lineage admission, live branching, capture completeness, hosted Logs Explorer or replay claim. Existing queries/schemas remain unchanged; new variants require upgraded readers.
