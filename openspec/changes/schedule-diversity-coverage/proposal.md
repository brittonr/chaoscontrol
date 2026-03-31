## Why

The explorer treats thread interleaving as a fixed configuration choice — you pick RoundRobin or Randomized at startup and every branch runs with the same scheduling strategy. For SMP guests, two branches with different fault schedules but identical vCPU interleavings exercise the same concurrency paths. The coverage bitmap has no signal for "this branch ran a different interleaving," so the explorer can't distinguish schedule-sensitive states from schedule-insensitive ones. Bugs that only manifest under specific interleaving patterns (e.g., a write on vCPU 0 racing with a read on vCPU 1 within a narrow window) are found only by luck.

## What Changes

- **Schedule fingerprinting**: Hash the sequence of (vCPU, quantum) transitions during a branch into coverage edges, so two runs with different interleavings look different to the coverage collector.
- **Per-branch schedule variation**: The explorer mutates scheduling parameters (strategy, quantum range, seed) across branches within a round, not just across runs. Each branch gets a different interleaving to maximize diversity.
- **Schedule-aware mutation**: The fault schedule mutator gains schedule mutations — changing quantum, switching strategies, re-seeding the scheduler RNG — alongside its existing fault timing/type mutations.

## Capabilities

### New Capabilities
- `schedule-diversity`: Per-branch vCPU schedule variation and schedule fingerprinting into the coverage bitmap.

### Modified Capabilities
_(none — no existing spec-level requirements change)_

## Impact

- **chaoscontrol-vmm**: `VcpuScheduler` needs to emit a fingerprint of the interleaving trace. `SimulationController` needs per-branch scheduler config override.
- **chaoscontrol-explore**: `ExplorerConfig` gains schedule diversity settings. `ScheduleMutator` gains schedule mutation operators. `run_branch` injects per-branch scheduler configs. Coverage enrichment merges schedule fingerprints alongside assertion-state edges.
- **CLI**: New flags for schedule diversity on `chaoscontrol-explore run` and `campaign`.
- **Checkpoint**: Schedule diversity config serialized for resume.
