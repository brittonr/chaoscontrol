## Context

The upstream event schema exposes moment.vtime to arbitrary expressions. Local envelopes already retain canonical tick; existing field_value deliberately returns borrowed strings and cannot synthesize numeric metadata without changing every lookup consumer. A dedicated typed projection avoids that cutover and keeps existing selector semantics unchanged.

## Decision

Add MapTick with input and destination field. Validate the existing nonempty/control-free/256-byte UTF-8 name even for empty sources, validate the child, evaluate it, charge one operation per materialized child row, and admit each original row before mutation. Copy only event.tick, ignoring same-named derived/details/text payloads. Format the u64 directly as decimal text, without a float conversion or intermediate JSON value. Reuse an existing destination String; allocate a key/value only when inserting a new field. Canonical tick and all other evidence/projections remain immutable. A destination named kind/text/process/source_kind changes only its derived projection; canonical lookup remains authoritative.

Admit generated rows immediately so Narrow/Filter/final duplicate collapse cannot hide overflow. Integrate child moment traversal and existing structural/scan/work/derived/result-byte/prefix/receipt rules. Receipts bind input, destination and all canonical source/result facts. No new schema/error/lookup variant or compatibility shim; new query readers are required.

MapTick's text remains exact through u64::MAX. FieldNumber, MapDifference and other numeric consumers still use binary64 and may round ticks above 2^53 or lose small differences near large values. Document and regress that boundary rather than claiming exact elapsed arithmetic. Ticks are the supplied envelope's domain, not automatically seconds, global causal time or a guarantee that combined histories are coherent.

## Verification

Capture replay baseline and exported query references; admit gates before implementation and demonstrate unsupported MapTick. Verify exact full rows for zero/ties/2^53 boundaries/u64::MAX/permutations/duplicates, payload shadows and destination reuse/canonical aliases, elapsed-time composition through both branches of WithLastValue and numeric selection/Narrow, all name bounds, intermediate/generated capacity, shared limits/moments and query/source/result receipt drift. Exercise actual CLI query/validate and failures without partial artifacts. Run replay tests, Clippy, workspace compilation, formatting and scope/evidence checks. Synchronize accepted requirement without archiving; stage only owned source hunks.
