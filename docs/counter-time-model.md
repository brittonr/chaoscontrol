# Counter-time model

Model-level evaluation for issue [#6](https://github.com/brittonr/chaoscontrol/issues/6).
The design is in
[`evaluate-progress-based-tsc-clock`](../.cairn/changes/evaluate-progress-based-tsc-clock/design.md);
runtime mode stays gated on `harden-pmu-transition-evidence`.

The model in [`tools/counter-time-model`](../tools/counter-time-model) (detached
workspace) treats a workload as ordered phases of retired instructions and VM
exits, advances a virtual TSC under one policy, and records when a TSC-based
timeout fires.

```bash
cd tools/counter-time-model
cargo test
```

| Fixture | Exit clock | Counter clock |
|---|---|---|
| 10M-instruction CPU loop, no exits, 1M-tick timeout | never fires (TSC stays 0) | fires at instruction 1,000,000 |
| 100 instructions + 2,000 exits of 10 instructions, 1M-tick timeout | fires at exit 1,000 | never fires (20,100 instructions) |
| 4 exits of 250k instructions, 1M-tick timeout, 2 ticks/instruction | — | fires at instruction 500,000, after 1 completed exit |

The model reproduces the motivating failure and the inverse risk: exit-driven time
can ignore real work, while a progress clock can fire earlier when exits dominate.

## Non-claims

This is a synthetic trace model, not the VMM clock. It does not measure PMU skid,
claim retired counts are exact or replayable, model HLT/PIT/snapshot interactions,
or authorize the runtime mode. Those remain prerequisite and design work.
