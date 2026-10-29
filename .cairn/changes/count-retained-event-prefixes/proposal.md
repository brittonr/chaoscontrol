## Why

Antithesis fold can annotate each observation with a running count of matching earlier observations, then filter downstream events using that context. ChaosControl retained queries currently select, project and combine events but cannot calculate a prefix count. Add a bounded counting operation rather than arbitrary reducer execution.

## What Changes

RunningCount { input, predicate, field } orders unique source positions by (tick, source_id, source_sequence) and annotates every row with an inclusive decimal u64 count of matches so far, starting from zero. Existing predicates, limits, moments and receipts apply. Preserve canonical evidence, admit intermediate and generated rows, and make ordering and source-position deduplication explicit.

## Impact

Owner: chaoscontrol-replay maintainers. Consumer: retained fault/log triage that needs to know how many matching observations occurred before a selected error. Adoption: compose RunningCount with Map/MapText, Filter, Matches and Narrow through EventQuery or query-events JSON. Durable capability: deterministic bounded local prefix aggregation. This is a local analogue of the counting example in Antithesis fold, not a generic fold, JavaScript interpreter, per-VM partitioner, branch-lineage reconstruction, capture-completeness guarantee, VM execution, replay proof or hosted event-search service.
