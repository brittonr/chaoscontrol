# Proposal: Adopt Nickel 1.17 for simulation configuration

## Why

ChaosControl configuration checks currently resolve to Nickel `1.15.1`. This is older than the stack Nickel `1.17.0` cohort.

Simulation profiles and evidence configuration must remain valid under the reviewed stack evaluator.

## What Changes

- Pin Nickel `1.17.0` at upstream commit `1320a983e6c3d1e2fb53dd2464b084b4903b1426`.
- Use the pin for profiles, fixtures, evidence configuration, and Nix checks.
- Add positive and negative compatibility fixtures for profile contracts, imports, merges, and diagnostics.
- Record the exact evaluator identity in bounded readiness evidence where applicable.

## Impact

ChaosControl configuration will use the stack Nickel cohort. ChaosControl retains VMM, schedule, fault, replay, and evidence authority.

## Dependencies

This change uses the upstream Nickel `1.17.0` release.

## Non-goals

- Do not change VMM or simulation semantics.
- Do not update unrelated dependencies only to obtain Nickel.
- Do not claim workload correctness from configuration acceptance.
