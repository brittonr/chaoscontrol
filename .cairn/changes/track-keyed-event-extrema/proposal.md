## Why

RunningMin/RunningMax mix all selected owners, unlike RunningCountBy/RunningSumBy. Retained multi-process/service latency and capacity triage needs independent peak and low-watermark annotations without manually enumerating every owner. Extend the bounded local typed analogue of [Antithesis fold](https://antithesis.com/docs/reference/event-set-reference/#fold), not arbitrary reducer execution.

## What Changes

Add RunningMinBy and RunningMaxBy with input, predicate, field, value_field and key_field. Share one extrema engine with existing unkeyed operators. Preserve canonical evidence, first-projection priority, original key/operand/predicate membership under aliases, exact string grouping and existing finite-number semantics. Missing keys and pre-first-match rows remove stale output, without suppressing predicate or matching operand errors.

## Consumer and Ownership

Consumer: retained multi-process/service peak-latency and low-watermark capacity triage. Durable capability: reusable independent per-key prefix extrema composing with JSON projections and downstream numeric selection. Maintenance owner: chaoscontrol-replay maintainers. Adoption: query-events JSON query forms; new readers required for the new forms, old schemas/queries unchanged.

## Non-Goals

No inferred source/VM/history identity, automatic resets/windows, composite keys, exact arithmetic, arbitrary reducers, causal/multiverse lineage, complete capture, hosted service, CPU/RSS/deadline guarantees, VM execution or replay proof. Preserve the user's uncommitted pointer regression.
