## Why

Antithesis's raw assertion API separates `condition`, `hit` and `must_hit`. Reachable declarations can have `condition=false` and `hit=false` while still requiring a hit. ChaosControl currently uses the condition to select its concrete reachability identity. Such declarations are registered as unreachable; later true hits cannot resolve that identity and disappear.

## What Changes

Resolve `AssertType::Reachability` from `must_hit`, not the observation condition. Keep that kind stable across declaration and later observations. Update the existing classification helper and its documentation; migrate its runtime caller. Preserve raw IDs, bounded serialization, source metadata, sparse sampling and rejection of undeclared late sites.

## Impact

Owner: ChaosControl SDK maintainers. Current consumer: third-party assertion framework adapters using `assert_raw`. Immediate outcome: valid raw reachable declarations and hits retain one accepted identity. Durable capability: declaration/observation-independent raw classification, with regression and real SDK-to-strict-CLI evidence. This is local Rust instrumentation parity, not VM execution, replay, hosted equivalence or release readiness.
