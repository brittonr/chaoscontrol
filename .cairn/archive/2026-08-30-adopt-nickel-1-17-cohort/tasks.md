# Tasks: Adopt Nickel 1.17 for simulation configuration

## Pin and profile boundary

- [x] [serial] Add an exact Nickel `1.17.0` source input at commit `1320a983e6c3d1e2fb53dd2464b084b4903b1426`. r[chaoscontrol.nickel_toolchain.cohort]
- [x] [serial] Replace profile, evidence, fixture, and developer-tool uses of Nickel `1.15.1`. r[chaoscontrol.nickel_toolchain.cohort]
- [x] [serial] Regenerate `flake.lock` only with a Nix lock command. r[chaoscontrol.nickel_toolchain.lockfile]
- [x] [parallel] Add a guard for older, floating, ambient, or mixed evaluators. r[chaoscontrol.nickel_toolchain.cohort]
- [x] [parallel] Prove profile acceptance does not replace ChaosControl run admission. r[chaoscontrol.nickel_toolchain.boundary]

## Compatibility and evidence

- [x] [parallel] Run valid deterministic profile, schedule, fault, guest, and evidence fixtures. r[chaoscontrol.nickel_toolchain.compatibility]
- [x] [parallel] Add malformed, missing-import, contract, bound, unknown-field, and cohort negative fixtures. r[chaoscontrol.nickel_toolchain.compatibility]
- [x] [serial] Record the exact evaluator identity in applicable readiness evidence. r[chaoscontrol.nickel_toolchain.validation]

## Validation

- [x] [serial] Run profile fixtures, evidence checks, formatting, Clippy, lifecycle gates, and relevant Nix checks. r[chaoscontrol.nickel_toolchain.validation]
