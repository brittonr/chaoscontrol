## Why

Antithesis event pipelines can numerically filter fields added by earlier transformations. ChaosControl Map/MapText and RunningCount store projections as strings, but existing numeric predicates inspect only canonical JSON details/text. Selecting count >= 2 currently requires enumerating string matches instead of a numeric threshold.

## What Changes

Add FieldNumber { field, comparison, value } to existing predicates. Resolve fields with existing canonical-name precedence, parse complete finite JSON-number text and reuse NumberComparison. Missing, nonnumeric and out-of-range values do not match, including NotEqual; Not retains ordinary complement semantics. Preserve canonical evidence and existing query bounds/receipts.

## Impact

Owner: chaoscontrol-replay maintainers. Current consumer: RunningCount pipelines selecting error observations after a minimum number of preceding matches. Adoption: FieldNumber after RunningCount, Map or MapText, or within existing compound predicates. Durable capability: numeric threshold filtering over selected canonical/derived strings without JavaScript execution or broad coercion. This is a bounded local analogue, not upstream Number() coercion, general arithmetic, exact arbitrary-width comparison, hosted search, capture completeness, VM execution or replay proof.
