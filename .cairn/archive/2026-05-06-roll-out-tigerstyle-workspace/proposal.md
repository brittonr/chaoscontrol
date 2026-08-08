## Why

ChaosControl already gates a focused set of owned crates with the pinned local Tigerstyle toolchain. The next rollout should make that staged lint profile apply to every Rust workspace package so new style-boundary regressions cannot land outside the focused slice.

## What Changes

- Expand the `tigerstyle-chaoscontrol-focused` check from a positive three-crate package list to the full Cargo workspace package set.
- Keep the current staged lint profile and `--lib` target scope so rollout remains actionable instead of enabling the full Tigerstyle catalog at once.
- Repair any findings exposed by the currently denied staged lint set.

## Capabilities

### Modified Capabilities
- `local-proof-style-inputs`: The pinned Tigerstyle consumer check covers the full ChaosControl Rust workspace package set.

## Impact

- **Files**: `Cargo.toml`, `flake.nix`, any Rust sources needed to satisfy the staged Tigerstyle profile, and `openspec/specs/local-proof-style-inputs/spec.md` after archive.
- **APIs**: No public API changes intended.
- **Dependencies**: No new dependencies intended.
- **Testing**: `./scripts/openspec validate roll-out-tigerstyle-workspace --strict`, `nix build .#checks.x86_64-linux.tigerstyle-chaoscontrol-focused --no-link -L`, `nix flake check --no-build`, and whitespace checks.
