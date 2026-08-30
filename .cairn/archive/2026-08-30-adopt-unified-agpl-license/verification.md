# Verification

Date: 2026-08-30

## Identity and authority

- Last published pre-policy revision: `c169afc3d37698f816b54238c03fbc36d3ea1aa3`.
- First unified implementation revision: `e477cad82f7d8fd0644cc4b940061f218f5cfcb0`.
- The path map keeps third-party, upstream-derived, generated-upstream, output, and earlier-grant exclusions.

## License checks

- `tools/check-license-boundary.rs .`: passed 58 positive and negative rules.
- `checks.x86_64-linux.license-boundary`: passed.
- All 22 workspace packages report `AGPL-3.0-or-later` through Cargo metadata.
- `cargo package --list --locked --allow-dirty` included `Cargo.toml` and `LICENSE` for every workspace package.
- A fresh generated Rust workload scaffold contained the complete AGPL text, matching Cargo metadata, and both SPDX source notices.
- The checker rejects stale Apache package metadata, missing license text, mismatched template notices, and attempted third-party relabeling.
- `deny.toml` enumerates every repository-owned package. AGPL remains absent from the global third-party allow list.

## Workspace checks

- `cargo test --workspace --all-targets --all-features`: passed. The VMM suite passed 486 tests, with 9 declared KVM-dependent ignores. All other workspace suites passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed after correcting existing drift in `chaoscontrol-wasm-differential`.
- Cairn validation, proposal gate, and design gate passed before closeout.

## Additional broad-rail observations

The existing Crane `dependency-policy` check did not reach policy evaluation. Cargo 1.98 panicked while `cargo-deny` requested full metadata for the Radicle dependency graph.

The packaged scaffold Nix app also stopped in the existing VM Cohort vendor source. Its package omits `config/generated/profile.json`. The direct repository scaffold and package-content checks passed.

These toolchain and upstream packaging failures are not license-policy findings. They do not weaken the explicit repository-owned package checks or third-party exclusions.

## Non-claims

This change records distribution terms for authorized source. It does not relicense third-party material, revoke prior grants, or relicense unrelated workload source and output.
