## MODIFIED Requirements

### Requirement: Tigerstyle tool and policy exposure [r[local-proof-style-inputs.tigerstyle-exposure]]
ChaosControl SHALL expose the pinned Tigerstyle tooling and checks through its root flake, and SHALL include a staged Tigerstyle consumer check over every Rust workspace library package.

#### Scenario: Full-workspace staged Tigerstyle gate passes [r[local-proof-style-inputs.tigerstyle-exposure.scenario.full-workspace-gate]]
- GIVEN the root flake has locked the local Tigerstyle input to an exact Git revision
- WHEN an operator runs `nix build .#checks.x86_64-linux.tigerstyle-chaoscontrol-focused --no-link -L`
- THEN Tigerstyle checks every Cargo workspace package library target through the pinned sibling toolchain
- AND the consumer gate reports a passing result for the staged lint profile
