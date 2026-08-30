# Causality Analysis Engine

## Why

When an assertion fails, the operator must manually inspect replay artifacts and apply counterfactual edits. ChaosControl minimizes fault schedules with delta debugging but does not minimize interleavings, and it does not attribute a failure to a probable cause. Antithesis pairs failure detection with causality analysis and time-travel debugging, which is what makes a reproduced race actionable in minutes instead of days.

## What Changes

- Add interleaving minimization that finds the smallest scheduling delta that still reproduces a bug.
- Add automated attribution that ranks seeds, schedules, faults, and declared events against a reproduced failure.
- Keep the analysis pure in a core crate and let shells read replay and verdict artifacts.
- Bind each attribution and minimized artifact set to the replay verdict and receipt.

## Impact

- **Code**: a pure minimization and attribution core plus a shell over replay artifacts.
- **Evidence**: attribution and minimized-delta artifacts enter the receipt flow.
- **Testing**: positive attribution on a fixture race and negative irreproducible, identically-ranked, and out-of-scope cases.

## Non-Goals

- No interactive notebook or hosted UI.
- No change to fault-schedule minimization that already exists.
- No claim of proof that a reported cause is the only cause.
