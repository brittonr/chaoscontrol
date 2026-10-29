## Why

RunningSum mixes observations from all selected owners; RunningCountBy already allows explicit per-key counting. Multi-process/service transferred-byte and cumulative-metric triage needs independent numeric totals without enumerating an owner-specific query for each process. Antithesis fold is the comparison source, not a requirement for an arbitrary reducer.

## What Changes

Add RunningSumBy { input, predicate, field, value_field, key_field }. Maintain positive-zero-seeded finite binary64 totals per exact string key, preserving canonical evidence and first-projection duplicate priority. Missing keys remove stale output but do not hide predicate or matching operand errors. Reuse one sum engine for keyed and unkeyed queries. Consumer: retained multi-owner cumulative metric analysis; maintenance owner: chaoscontrol-replay maintainers.

## Non-Goals

No automatic VM/source/history identity, resets/windows, composite keys, exact arithmetic, arbitrary reducers, causal/multiverse lineage, complete capture, hosted service, CPU/RSS/deadline guarantees, VM execution or replay proof. Preserve the user's uncommitted pointer regression.
