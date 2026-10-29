## Why

The Antithesis event-set guide demonstrates filtering text with `output_text.includes("[WARN]")` and documents case-sensitive contains/exact matches. ChaosControl's standalone Contains/Matches query operators already implement bounded text selection, but boolean predicates cannot use them. Operators cannot directly negate text or combine warnings/errors with exclusions in one filter.

## What Changes

Add Contains and Matches variants to EventPredicate, borrowing the existing field lookup and using existing field/text admission. They compose under All/Any/Not without new limits or an alternate evaluator. Missing fields do not match. Canonical text/kind/process/source_kind take precedence over similarly named derived fields. Other names address derived string fields, including Map projections. Preserve existing query-level selectors and costs.

## Impact

Owner: replay/query maintainers. Consumer: local `query-events` operators selecting warnings/errors while excluding heartbeat or maintenance messages. Immediate outcome: composable text selection and exclusion. Durable capability: typed bounded text leaves in the existing boolean predicate core. New variants require upgraded readers; existing queries, receipt schemas, canonical row identity and resource controls remain unchanged. Retained-input query behavior only, not arbitrary JavaScript, full Notebook compatibility, reactive branching, capture completeness, VM/replay proof or release readiness.
