## Why

Retained multi-process triage needs independent match counts for each owner. RunningCount currently counts all selected sources together. The Antithesis fold comparison motivates a bounded keyed counter, not arbitrary reducer execution or inferred timelines.

## What Changes

Add RunningCountBy { input, predicate, field, key_field }. Count matching unique canonical prefixes separately for exact selected string keys. Missing-key rows retain evidence but lose stale count output; explicit empty keys form a valid group. Owner and consumer: chaoscontrol-replay maintainers, multi-process/service error-frequency triage. Existing RunningCount remains unchanged.

## Non-Goals

No automatic VM/source/history partitioning, resets, sliding windows, composite keys, arbitrary reducers, causal/multiverse lineage, hosted search, complete capture, CPU/RSS/deadline guarantees, VM execution or replay proof. Preserve the user's uncommitted pointer regression.
