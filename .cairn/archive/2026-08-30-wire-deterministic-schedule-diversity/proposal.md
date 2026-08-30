# Wire Deterministic Schedule Diversity

## Why

The scheduler has a per-branch interleaving facility but the explorer never passes a variant into branch execution. `explore_round()` builds every `BranchWork` with `schedule_variant: None` (`explorer.rs:653`), and `run_branches_sequential()` does the same (`explorer.rs:1064`). The CLI enables `schedule_diversity` for SMP, but nothing consumes it. A timing-tight race stays unreachable because the explorer searches fault schedules, not vCPU interleavings.

## What Changes

- Wire `ScheduleVariant` generation into the branch mutator path that the explorer actually calls.
- Apply the variant to every VM scheduler before each branch run through the existing `apply_schedule_variant` route.
- Bind the variant identity and build metadata into branch fingerprints, bug reports, and replay verdicts.
- Fail closed when a declared variant cannot be applied or exceeds admitted scheduler bounds.
- Validate the mechanism against a known race artifact before treating no-bug runs as meaningful.

## Impact

- **Code**: explorer branch dispatch, mutator invocation, evidence identity.
- **Testing**: positive deterministic variance, negative unwired, unsupported-strategy, and identity-drift cases.
- **Evidence**: bug and verdict artifacts record the exact interleaving policy that produced them.

## Non-Goals

- No change to the vCPU scheduler core semantics.
- No hosted or cross-machine scheduling.
- No new fault classes.
