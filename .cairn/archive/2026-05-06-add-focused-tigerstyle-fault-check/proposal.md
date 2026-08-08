## Why

ChaosControl now pins the local Tigerstyle toolchain, but the root flake only proves that the Tigerstyle repository itself evaluates. A focused consumer check gives ChaosControl a repeatable first gate over owned production Rust code without taking on a workspace-wide lint-drain in one change.

## What Changes

- Add a staged `dylint.toml` profile for the initial ChaosControl Tigerstyle rollout.
- Add a root flake check that runs Tigerstyle against the `chaoscontrol-fault` library crate.
- Keep the check narrow and reproducible by using the pinned sibling Tigerstyle input and the repository Cargo lockfile.

## Capabilities

### Modified Capabilities
- `local-proof-style-inputs`: Extends Tigerstyle exposure from tool availability to a consumer check over ChaosControl source.

## Impact

- **Files**: `flake.nix`, `dylint.toml`, OpenSpec change/archive/spec files.
- **APIs**: Adds `checks.x86_64-linux.tigerstyle-chaoscontrol-fault`.
- **Dependencies**: Uses the already-pinned `tigerstyle` flake input.
- **Testing**: `nix build .#checks.x86_64-linux.tigerstyle-chaoscontrol-fault --no-link -L`, `nix flake check --no-build`, and strict OpenSpec validation.
