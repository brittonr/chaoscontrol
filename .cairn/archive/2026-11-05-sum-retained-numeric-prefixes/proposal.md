## Why

Antithesis fold can attach running aggregates to retained events. ChaosControl supports RunningCount but cannot compute cumulative numeric metrics such as bytes transferred without external processing.

## What Changes

Add RunningSum { input, predicate, field, value_field }. Annotate every canonical unique-position prefix with an inclusive finite binary64 total of matching selected fields, seeded at 0.0. Preserve canonical evidence, predicate-before-overwrite behavior and existing resource/receipt rules. Reject invalid matching operands or nonfinite intermediate totals with InvalidNumber.

## Capability and Ownership

Consumer: local retained cumulative byte/latency triage in query-events. Outcome: bounded reusable numeric aggregation without a generic reducer. Owner: chaoscontrol-replay maintainers. Adoption: Map/MapText/WithLastValue can produce operands; downstream FieldNumber and Narrow can consume totals. Repeatability: permanent behavioral regressions and actual CLI receipts.

## Non-Goals

No JavaScript fold interpreter, exact rational/integer summation, implicit per-source partitioning, multiverse lineage, causality, complete capture, hosted search, CPU/RSS/deadline guarantee, VM execution or replay proof. Preserve the user's uncommitted pointer regression.
