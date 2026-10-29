## Context

Antithesis fold accepts arbitrary stateful reducers; local count/sum operators define canonical unique-event prefixes. FieldNumber and RunningSum already share finite JSON-number operand parsing.

## Decision

Add RunningMin and RunningMax with input, predicate, field and value_field, matching RunningSum's JSON shape. Reuse one evaluation arm for the two extrema, without changing existing aggregate semantics. Admit child rows before deduplication; charge every materialized child, stable-sort by (tick,source_id,source_sequence) only if needed, keep the first projection at each source position. Start with no value, not zero or an infinite sentinel. Predicate and operand read each original row before destination overwrite. Only matching operands are parsed through finite_field_number; missing/malformed/nonfinite matching operands fail InvalidNumber. Nonmatching malformed/missing values are skipped. The first match initializes state. Strict less-than/greater-than updates retain the first equal value, including signed zero. Binary64 conversion may round wide integers or underflow small values; there is no exact arithmetic claim.

Every unique row is retained. Remove stale destination fields before the first match; after a match serialize the finite selected value as JSON-number text. Reuse existing destination strings; no prefix copies or extra row collections. Other projections and canonical evidence remain unchanged. Input-field aliases and predicate/destination aliases must read before overwrite. Canonical field names retain lookup precedence.

Admit both names and full predicates even on empty input; share validation with RunningSum. Reuse predicate cache/work charging, structural/scan/derived/result limits and child moment propagation. Admit each output immediately so downstream Narrow/Filter cannot conceal overflow. Receipts bind operator choice, children, predicate, both names, canonical sources and result. Older queries and schemas remain unchanged; new query variants require upgraded readers.

## Verification

Baseline replay tests and exported EventQuery references precede implementation. Admit proposal/design/tasks, demonstrate unsupported variants, then regress full rows: empty/no-match/prefix transition, negative-only minima/maxima, ties/duplicates/extreme ticks/permutations, signed-zero and numeric boundaries, invalid matching/skipped operands, projection/filter/alias composition, both names/full predicate admission, hidden intermediate/generated overflow, shared bounds, moments and receipt drift. Exercise actual query-events query/validate and error paths without partial artifacts. Run replay tests, Clippy, workspace compilation, formatting and scope/evidence checks; synchronize accepted requirement without archiving. Exclude the user's pointer test from the commit.
