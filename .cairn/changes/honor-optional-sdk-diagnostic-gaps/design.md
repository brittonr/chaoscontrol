## Context

`LocalDryRunReport` groups legacy IDs and ignores `must_hit`. Its reachability bucket therefore conflates reachable and unreachable declarations. The strict evidence crate is not available as an SDK dependency, and remains the promotion authority.

## Decisions

### Decision: Store optional must-hit metadata in the private catalog

Parse present metadata as a boolean; absence stays unknown. Merge a later explicit value only if it agrees with every prior explicit value. Reject conflicts in either record order. This prevents a false required-to-optional transition without requiring metadata from historical JSONL.

### Decision: Derive progress from retained counters

Use the existing success and hit counts instead of separate success-ID sets. A zero-hit optional sometimes site has no progress obligation; once executed, it still needs a true observation. Reachability sites with `must_hit=false` are forbidden paths, not required targets, so they never enter the reachable-progress list. Their false hits still increment observed violation counts. Keep all unobserved sites in the inventory: lack of execution is a fact even when it is not a failed property. No public struct shape changes.

## Risks / Trade-offs

Legacy metadata is not collision-safe and unknown optionality stays conservative. The SDK parser accepts broader historical display types than the strict validator; this change does not replace strict display/must-hit admission. Positive/negative regressions cover quiet and executed properties, missing/invalid/conflicting metadata and retained counts. A throwaway SDK consumer exercises real declarations through both diagnostic and strict CLIs.
