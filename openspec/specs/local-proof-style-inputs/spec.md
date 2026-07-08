# local-proof-style-inputs Specification

## Purpose

This specification defines how ChaosControl records and validates local sibling Octet and Trellis inputs used for proof/style hardening work.
## Requirements
### Requirement: Local sibling input provenance [r[local-proof-style-inputs.provenance]]
ChaosControl SHALL record the local sibling Octet and Trellis repositories as explicit flake inputs pinned to reviewable Git revisions.

#### Scenario: Pinned sibling revisions are visible [r[local-proof-style-inputs.provenance.visible]]
- **GIVEN** the workspace flake lock
- **WHEN** a reviewer inspects the `octet` and `trellis` nodes
- **THEN** each node SHALL identify the sibling repository source and the intended Git revision.

### Requirement: Trellis verified-logic proof rail exposure [r[local-proof-style-inputs.verified-logic-proof-rail]]
ChaosControl SHALL expose the pinned Trellis `verified-logic` package and a flake check that builds the sibling Verus proof rail using Trellis' own toolchain pins.

#### Scenario: Proof rail evaluates from ChaosControl [r[local-proof-style-inputs.verified-logic-proof-rail.evaluates]]
- **GIVEN** the ChaosControl flake
- **WHEN** `nix flake check --no-build` is run
- **THEN** the Trellis `verified-logic` package and proof-rail check SHALL evaluate successfully.

#### Scenario: Proof rail builds from ChaosControl [r[local-proof-style-inputs.verified-logic-proof-rail.builds]]
- **GIVEN** the ChaosControl flake
- **WHEN** `nix build .#checks.x86_64-linux.verified-logic-verus-proofs --no-link -L` is run
- **THEN** the build SHALL complete with Verus reporting zero errors.

### Requirement: Tigerstyle tool and policy exposure [r[local-proof-style-inputs.tigerstyle-exposure]]
ChaosControl SHALL expose the pinned Octet-provided Tigerstyle tooling and checks through its root flake, and SHALL include a staged Tigerstyle consumer check over every Rust workspace library package.

#### Scenario: Full-workspace staged Tigerstyle gate passes [r[local-proof-style-inputs.tigerstyle-exposure.scenario.full-workspace-gate]]
- GIVEN the root flake has locked the local Octet input to an exact Git revision
- WHEN an operator runs `nix build .#checks.x86_64-linux.tigerstyle-chaoscontrol-focused --no-link -L`
- THEN Tigerstyle checks every Cargo workspace package library target through the pinned Octet toolchain
- AND the consumer gate reports a passing result for the staged lint profile
