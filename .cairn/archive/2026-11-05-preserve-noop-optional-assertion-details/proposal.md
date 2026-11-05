## Why

Antithesis Rust SDK 0.3.0 evaluates optional always-assertion conditions and details even without `full`, without requiring JSON serialization. ChaosControl's no-op `emit_registered_always_or_unreachable` uniquely requires `&()` while the corresponding full-mode API accepts borrowed serialized details. A workload using `cc_assert_always_or_unreachable!(condition, message, &details)` therefore fails to compile when instrumentation is disabled.

## Change

Make only the no-op stub generic over `S: ?Sized`, matching the existing ordinary assertion convention. Preserve condition-before-details evaluation, one evaluation each, const messages, borrowed ownership and disabled output. No macro rewrite, dependencies, alias migration, transport change or allocation.

## Consumer and ownership

Consumer: Rust workloads compiling optional always assertions in both instrumented and production/no-std configurations. Owner: ChaosControl SDK maintainers. Immediate outcome: detail-bearing optional assertions compile without `full`. Durable contribution: feature-independent borrowed-detail contract protected by evaluation-order and unsized-value regressions plus an actual consumer smoke. No VM/replay, hosted-platform or release-readiness claim.
