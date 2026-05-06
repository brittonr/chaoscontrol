## MODIFIED Requirements

### Requirement: Tigerstyle tool and policy exposure [r[local-proof-style-inputs.tigerstyle-exposure]]
ChaosControl SHALL expose the pinned Tigerstyle cargo and standards tools plus a policy-registry check using Tigerstyle's own flake toolchain pins, and SHALL include a focused Tigerstyle consumer check over owned ChaosControl Rust source that covers the fault, protocol, and SDK library crates.

#### Scenario: Tigerstyle tools evaluate from ChaosControl [r[local-proof-style-inputs.tigerstyle-exposure.evaluates]]
- **GIVEN** the ChaosControl flake
- **WHEN** `nix flake check --no-build` is run
- **THEN** the Tigerstyle cargo tool, standards tool, policy-registry check, and focused ChaosControl consumer check SHALL evaluate successfully.

#### Scenario: Focused fault, protocol, and SDK crate consumer check passes [r[local-proof-style-inputs.tigerstyle-exposure.focused-consumer-check]]
- **GIVEN** the ChaosControl flake and staged Tigerstyle profile
- **WHEN** `nix build .#checks.x86_64-linux.tigerstyle-chaoscontrol-focused --no-link -L` is run
- **THEN** Tigerstyle SHALL check the `chaoscontrol-fault`, `chaoscontrol-protocol`, and `chaoscontrol-sdk` library crates through the pinned sibling toolchain and report a passing consumer gate.
