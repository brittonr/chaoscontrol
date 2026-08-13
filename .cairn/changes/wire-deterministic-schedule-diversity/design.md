# Design: Wire Deterministic Schedule Diversity

## Context

`ScheduleVariant`, `mutate_with_schedule()`, and `apply_schedule_variant()` exist and are tested at the crate boundary. The explorer does not call them. The CLI accepts `--scheduling randomized` and sets `schedule_diversity` when `num_vcpus > 1`. The branch execution path hard-codes `None`.

## Decisions

### 1. The explorer invokes the schedule mutator

Replace the `mutate()` call in `explore_round()` with `mutate_with_schedule()` when `schedule_diversity` is enabled and `num_vcpus > 1`. Each returned pair carries the mutated fault schedule and the optional `ScheduleVariant`.

### 2. Branch work carries the variant

`BranchWork.schedule_variant` receives the pair's variant. Both the parallel worker path and `run_branches_sequential()` apply it through `apply_schedule_variant()` before `begin_counterfactual_fault_run()`. A variant that cannot be applied fails the branch closed with a typed error.

### 3. Variant identity enters evidence

The branch schedule fingerprint includes the variant policy bytes. Bug reports and replay verdicts record the variant seed, strategy, and quantum so a reproduced interleaving is an exact input of the claim.

### 4. ReSeed, QuantumShift, and StrategyFlip stay as authored

The existing mutation operators are retained. Validation adds deterministic coverage for each operator and the no-op case.

### 5. Mechanism validation precedes claim use

A fixture race workload, the raft fig8 artifact for example, must reproduce with a declared variant and not reproduce without schedule search. Only then may a no-bug campaign claim interleaving coverage.

## Risks

The search space of interleavings is large. The variant mutator limits quantum and seed search but does not bound it. Campaign throughput declines with more branches. Evidence must record the exact policy so a green run cannot overclaim.
