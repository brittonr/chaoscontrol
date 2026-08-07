## Why

`FaultEngine::poll_faults` increments `faults_injected` as soon as faults become due, and `SimulationController::step_round` clones the same values into `faults_fired` before calling `apply_fault`. Application then silently ignores many invalid targets and missing devices. Several variants only write controller fields that no execution or I/O path reads, including current disk error/full flags and memory-pressure, vCPU-stall, clock-freeze, and clock-jitter state. Reports can therefore claim that a fault fired even when it was inapplicable, unsupported, failed, or could never affect guest behavior.

Fault selection is useful schedule evidence, but it is not proof of application or observation.

## What Changes

- Introduce explicit selected, applicable, applied, rejected, and observed fault stages with a stable attempt identity and typed reason/effect data.
- Plan applicability in a pure core from normalized fault and VM/device capability facts before mutation.
- Require every schedulable fault variant to reach a real enforcement path or return an explicit unsupported/rejected outcome.
- Make application adapters return outcomes; invalid targets, missing devices, invalid ranges, invalid vCPUs/bits, and backend errors can no longer succeed as no-ops.
- Replace ambiguous `faults_injected`/`faults_fired` accounting with stage-specific counters and round records.
- Emit observations only from the execution or data path where the declared effect actually occurs.
- Add complete positive and negative variant matrices plus replay, snapshot, partial-failure, and report-consistency tests.

## Impact

- **Files**: `chaoscontrol-fault` engine and schedule interfaces, VMM controller fault planning/application, block/network/clock/scheduler effect hooks, round results, reports, snapshots, and evidence-boundary validation.
- **Compatibility**: consumers of `faults_injected` and `faults_fired` must migrate to stage-specific outcomes; legacy fields may remain only as explicitly defined aliases and cannot imply observation.
- **Behavior**: unsupported or inapplicable faults become visible rejections instead of silent successes; configured campaigns can choose whether rejection is fatal by explicit policy.
- **Ownership**: runtime attempt and outcome records remain Rust-owned. Nickel may validate compact review summaries, but does not own high-volume fault traces.
- **Scope boundary**: this package does not own replay artifact DTO extraction, snapshot-reference confinement, or whole-VM snapshot completeness.
- **Claims**: an applied record proves a bounded control-plane mutation or synchronous action succeeded; only an observed record proves the declared data/execution path encountered that effect.
