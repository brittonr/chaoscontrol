## Why

Antithesis with_last annotates an event with its nearest earlier matching event. Local Temporal currently supports only existence/absence selection, so retained error triage cannot carry the last restart message or leader value into the selected row.

## What Changes

Add WithLastValue with input/witness queries, destination/witness field names and a finite tick window. Copy one scalar string from the nearest strictly earlier witness; preserve candidate rows and canonical evidence. Resolve equal-tick witness ties deterministically, preserve first duplicate-position projections, clear stale destination values on absence and retain all existing admission, work, moment and receipt bounds.

## Capability and Ownership

Consumer: local query-events error/restart and leader-transition triage. Outcome: inspect and filter nearest past context without a Cartesian join or arbitrary code. Owner: chaoscontrol-replay maintainers. Adoption: wrap existing candidate/witness expressions; Map/MapText can produce witness fields and existing text/numeric predicates can read the copied value. Behavioral regressions and actual CLI receipts provide repeatability.

## Non-Goals

No full nested-event with_last representation, multiple named witness sets, inferred causality, multiverse lineage, implicit VM/process partitioning, with_next/timeouts, hosted search, complete capture, CPU/RSS/deadline guarantee, VM execution or replay proof. Preserve and exclude the user's uncommitted pointer regression.
