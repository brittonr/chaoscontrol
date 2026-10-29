## Why

Antithesis fold permits running aggregate annotations. Local RunningCount and RunningSum cannot directly identify peak latency or low-watermark capacity without external reduction.

## What Changes

Add typed RunningMin and RunningMax over finite numeric fields, annotating inclusive canonical unique-event prefixes. Before any match, remove stale destination projections; do not invent a zero/infinite seed. Preserve canonical evidence, existing bounds, moments and receipt binding. Consumer: retained metric triage; owner: chaoscontrol-replay maintainers.

## Non-Goals

No arbitrary reducer, JavaScript coercion, exact arithmetic, implicit VM/process/source partitioning, multiverse lineage or causality, complete capture, hosted search, CPU/RSS/deadline guarantee, VM execution or replay proof. Preserve the user's uncommitted pointer regression.
