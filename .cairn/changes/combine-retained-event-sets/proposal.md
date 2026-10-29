## Why

Antithesis event sets expose intersect and difference in addition to union. ChaosControl can combine predicates on one row but cannot intersect or subtract independently transformed retained query branches.

## What Changes

Add binary Intersect { input, other } and Difference { input, other } expressions. Reuse existing (source_id, source_sequence) identity and preserve left-hand rows/projections. Evaluate both bounded branches, admit hidden right-hand rows, sort the right vector in place and binary-search membership while retaining original left order. Reuse structural/work/result bounds, moments and receipts.

## Impact

Owner: replay/query maintainers. Consumer: operators combining independently projected evidence selections or excluding retained known-noise sets. Durable capability: composable bounded event-set algebra. Identity intentionally follows existing ChaosControl Union, not upstream full-field synthetic-event equality. No arbitrary transforms, capture completeness, multiverse lineage, hosted event-search service, VM execution or replay-proof claim. Existing queries/schemas remain unchanged; new variants require upgraded readers.
