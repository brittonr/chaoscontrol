## Context

The upstream with_last operator returns input events with nearest past context. ChaosControl's event model has immutable canonical fields and bounded derived strings; Temporal already uses strict tick ordering, independent child queries and caller-supplied coherent tick domains.

## Decision

Add EventQuery::WithLastValue { input, witness, field, witness_field, within_ticks }. Both names use existing nonempty/control-free/256-byte UTF-8 admission, even on empty input. Independently evaluate and admit both children, even for zero window or no candidates. Charge each child normally plus one operation per candidate and per witness before deduplication.

Stable-sort witnesses by (tick, source_id, source_sequence), then deduplicate source positions retaining the first projection. For each candidate binary-search the last witness with strictly smaller tick; that is the nearest prior tick, and ties choose the greatest canonical source/sequence key at that tick. This deterministic tie-break is not causal ordering. Accept it only when candidate.tick - witness.tick <= within_ticks; strict ordering makes subtraction safe at u64 extremes. A zero window yields no match. Do not allocate a tick array, clone candidate vectors or perform a Cartesian join.

Copy the chosen witness field through existing field_value canonical-name precedence. An absent field on the nearest witness does not fall back to an older witness; an existing empty string is copied. Reuse an existing destination String through clear/push_str, allocating only for a new projection. Absence removes only the destination projection to prevent stale context. Candidate canonical fields remain untouched, even if destination names collide. Admit candidates before and after writes and all witnesses before deduplication so intermediate/generated capacity errors cannot be hidden by later narrowing, duplicate removal or empty/zero-window selection.

Integrate child traversal and moment reconciliation like Temporal. Preserve existing final canonical sort/first-position deduplication, prefix truncation, structural/scan/result bounds and complete-expression/source/result receipt binding. No schema change; new variants require upgraded readers. Both query branches must be restricted explicitly for same-VM/process/source correlation. This is a bounded scalar analogue of with_last, not full nested-event or multiverse parity.

## Verification

Capture baseline and exported query references; admit lifecycle gates before implementation. Reproduce unsupported WithLastValue with an independent pairwise nearest-witness oracle. Check tick extremes, exact window edges, zero/empty cases, permutation/tie behavior, duplicate projection priority, nearest missing versus older present, canonical and empty-field lookup, independent witness projections and downstream selectors, name admission, intermediate/generated capacity, hidden invalid witness errors, shared bounds, moments and receipt drift. Exercise real CLI successes and failures without partial artifacts. Run replay tests, Clippy, workspace compilation, formatting, scope/evidence checks and scoped Cairn gates; synchronize the requirement without archiving or promoting unrelated global traceability failures.
