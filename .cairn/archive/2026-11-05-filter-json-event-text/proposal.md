## Why

Antithesis's event-set memory-monitor example parses JSON stdout, selects a process and applies a numeric threshold. MapText projects scalars as strings, while JsonNumber reads only canonical details. Operators cannot apply a numeric threshold to retained JSON stdout without external preprocessing.

## What Changes

Add a JsonTextNumber predicate with the existing pointer/comparison/number shape. It parses only visited candidate text, uses existing finite binary64 comparisons without coercion, and preserves canonical rows. Invalid JSON fails explicitly, matching MapText. Reuse all existing admission, compound short-circuit, operation, moment and receipt rules.

## Impact

Owner and maintenance: replay/query maintainers. Consumer: local retained-log analysis of JSON memory-monitor stdout. Current query variants and schemas remain unchanged; new predicates require upgraded readers. This closes bounded numeric stdout filtering, not arbitrary JavaScript transforms, reactive branches, hosted Notebook compatibility, VM correctness or replay readiness.
