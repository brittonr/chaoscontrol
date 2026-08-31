# ChaosControl Nickel Toolchain Delta

## ADDED Requirements

### Requirement: ChaosControl pins Nickel 1.17 exactly

r[chaoscontrol.nickel_toolchain.cohort] ChaosControl MUST use Nickel `1.17.0` from upstream commit `1320a983e6c3d1e2fb53dd2464b084b4903b1426` for simulation profiles and evidence configuration.

#### Scenario: Profile tools resolve
r[chaoscontrol.nickel_toolchain.cohort.scenario.exact]
- GIVEN ChaosControl builds profile and evidence checks
- WHEN cohort admission runs
- THEN every Nickel command MUST use the exact reviewed evaluator

#### Scenario: An older or floating evaluator resolves
r[chaoscontrol.nickel_toolchain.cohort.scenario.rejected]
- GIVEN an ambient package, branch, or older pin supplies Nickel
- WHEN admission runs
- THEN admission MUST fail

### Requirement: Simulation meaning remains ChaosControl-owned

r[chaoscontrol.nickel_toolchain.boundary] Nickel MUST validate configuration shape only. ChaosControl MUST retain schedule, fault, guest, replay, and evidence meaning.

#### Scenario: A profile passes
r[chaoscontrol.nickel_toolchain.boundary.scenario.product-admission]
- GIVEN Nickel accepts a simulation profile
- WHEN ChaosControl admits a run
- THEN ChaosControl MUST still apply its own run and authority checks

### Requirement: Compatibility includes negative profiles

r[chaoscontrol.nickel_toolchain.compatibility] Tests MUST cover valid deterministic profiles and rejected malformed, missing-import, contract, bound, unknown-field, and cohort cases.

#### Scenario: The profile matrix runs
r[chaoscontrol.nickel_toolchain.compatibility.scenario.matrix]
- GIVEN representative valid and invalid profiles
- WHEN Nickel `1.17.0` evaluates them
- THEN valid profiles MUST retain supported outcomes
- AND invalid profiles MUST fail closed

### Requirement: Lockfiles are tool-generated

r[chaoscontrol.nickel_toolchain.lockfile] ChaosControl MUST change source inputs in `flake.nix`. Nix MUST generate any `flake.lock` change.

#### Scenario: The pin changes
r[chaoscontrol.nickel_toolchain.lockfile.scenario.generated]
- GIVEN the exact Nickel source input is declared
- WHEN the lockfile updates
- THEN a Nix lock command MUST produce the change

### Requirement: The repository validation rail passes

r[chaoscontrol.nickel_toolchain.validation] The change MUST pass profile fixtures, evidence checks, formatting, Clippy, lifecycle gates, and relevant Nix checks.

#### Scenario: Validation completes
r[chaoscontrol.nickel_toolchain.validation.scenario.passed]
- GIVEN the pin, fixtures, and evidence are current
- WHEN validation runs
- THEN every required check MUST pass or report one exact blocker
