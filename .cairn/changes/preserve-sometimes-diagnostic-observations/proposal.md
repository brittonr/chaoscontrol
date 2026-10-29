## Why

Antithesis's [sometimes contract](https://docs.rs/antithesis_sdk/0.3.0/antithesis_sdk/macro.assert_sometimes.html) requires at least one true observation, not every observation to be true. The SDK's diagnostic `LocalDryRunReport` nevertheless increments `failed_assertions` for each false sometimes hit, even when a later hit succeeds. The strict evidence reader already separates raw observations from property verdicts.

## What changes

Exclude false sometimes hits from the diagnostic observed-violation counter. Retain every raw hit and its success/failure counts, the never-successful progress list/gap, and existing observed safety-violation counting. Document that this diagnostic counter is not the strict report's distinct failed-property count. No change to strict evidence verdicts, campaign aggregation, guest transport or metadata admission.

## Capability and boundaries

Owner: ChaosControl SDK maintainers. Current consumer: Rust workload authors inspecting `LocalDryRunReport` before a VM campaign. Immediate outcome: unsuccessful progress attempts no longer look like observed safety violations. Durable contribution: runtime regressions for eventual success, stalled progress and genuine safety failures, plus SDK-to-strict-report consumer evidence. This remains local diagnostic/instrumentation evidence, not collision-safe SDK diagnostic authority, VM execution, replay or hosted-platform parity.
