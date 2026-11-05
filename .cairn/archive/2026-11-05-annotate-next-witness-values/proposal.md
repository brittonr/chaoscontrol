## Why

Retained errors can carry past context with WithLastValue, but not the next recovery/completion value. Add forward scalar context to the local event-query surface. [Antithesis with_next](https://antithesis.com/docs/reference/event-set-reference/#with_next) is comparison material: its matched-event output and synthetic timeouts are deliberately not claimed here.

## What Changes

Add WithNextValue and WithNextValueBy alongside existing earlier-witness forms. Retain candidates and copy a selected scalar from the nearest strictly later in-window witness, optionally requiring exact same-key matching. Reuse one direction-aware witness engine, preserving old earlier-witness results, first-projection duplicate priority, canonical evidence, admission, shared bounds and receipts.

## Consumer and Ownership

Consumer: retained error-to-recovery and request-to-completion triage. Durable capability: forward scalar correlation composed with witness JSON/tick projections and downstream selection. Adoption: query-events JSON queries with new readers; existing query/schema contracts unchanged. Maintenance owner: chaoscontrol-replay maintainers. Repeatability: permanent pairwise-oracle/boundary regressions and actual query/receipt CLI scenarios.

## Non-Goals

Not Antithesis with_next wire/representation compatibility: no matched-event replacement, synthetic timeout events, named witness sets, nested witness objects or multiverse history. No inferred causality, source/VM/history identity, tick-to-seconds conversion, complete capture, hosted service, CPU/RSS/deadline guarantee, VM execution or replay proof. Preserve the user-owned pointer regression.
