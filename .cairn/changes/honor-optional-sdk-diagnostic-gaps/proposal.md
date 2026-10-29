## Why

Antithesis distinguishes required reachable paths from optional forbidden paths through `must_hit`. The SDK diagnostic currently puts a cataloged, never-executed `Unreachable` site in `reachable_without_hit`, even though avoiding that path is success. Optional sometimes sites similarly acquire progress gaps without execution. The strict report already handles these cases correctly.

## What Changes

Honor explicit `must_hit=false` when deriving diagnostic progress gaps. Keep actual observation rows, raw counts and the complete unobserved-site inventory. Required and legacy-unknown sites retain progress gaps; executed optional violations remain visible. Reject malformed or contradictory must-hit metadata instead of allowing optionality to erase a required gap.

## Impact

Owner: ChaosControl SDK maintainers. Consumer: Rust workload authors using `LocalDryRunReport` before strict quality checks. Immediate outcome: no false missing-reachability warning for a quiet forbidden path. Durable capability: diagnostic optionality agrees with SDK declarations and strict report semantics, with behavioral regressions and a real SDK consumer smoke. No public layout or emitted schema change; diagnostic parsing remains non-collision-safe and does not authorize VM promotion, replay or release. Test default/sparse SDK paths, strict CLI scenarios, compilation, formatting and evidence/scope contracts.
