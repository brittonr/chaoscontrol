## Context

Existing Union supplies n-ary query admission, source-position first-wins ordering and moment combination. Antithesis distinct_by_moment instead keeps one event per vtime with event-set priority. Canonical sorting before choosing winners would incorrectly override branch/input priority.

## Decision

Use DistinctByTick { inputs: Vec<EventQuery> }, rejecting an empty list like Union. Evaluate branches left-to-right with existing bounded query evaluation and the shared regex cache. Charge each materialized branch row once beyond child evaluation, admit it even if its tick already has a winner, and move the first row for each u64 tick into the output. A HashSet<u64> tracks seen ticks using the existing Union pattern; no new key type, row clone or whole-source preprocessing. The key set is bounded by admitted branch rows and shared logical work. Preserve original branch row order until existing final canonical sorting/truncation.

Share Union's recursive node/depth and moment handling. All branches are evaluated even when earlier branches cover every tick; malformed JSON and invalid projected collisions cannot masquerade as harmless duplicates. Direct differing moments are ambiguous; an explicit outer UpTo retains existing expression scoping. Receipts bind branch order and all branch expressions, limits, supplied source events and winners, including discarded observations.

## Verification

Capture passing replay baseline, inspect exported enum references and admit native gates before implementation. Reproduce unsupported DistinctByTick. Regress first encountered tick winners across source orders/branch priorities, multiple sources/VMs, extreme ticks, enriched projections/downstream filtering, empty input/source, hidden duplicate admission/parsing, exact/exhausted work, shared structural bounds, canonical prefix/bytes, moments and receipt drift. Exercise actual CLI positive/negative queries and no partial failure artifacts. Run replay tests, denied-warning Clippy, workspace compilation, formatting, Nickel scope/evidence checks and scoped gates, then synchronize the requirement while keeping the change active. Preserve/exclude the user's pointer test and report existing unrelated traceability failures.
