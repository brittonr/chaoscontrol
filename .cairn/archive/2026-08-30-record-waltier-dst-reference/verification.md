# Verification

## Documentation boundary

- `docs/references/waltier-dst.md` records WalTier revision `d5dda89fb176d590d03c7812d047ced2712bba94`.
- `docs/references/antithesis-documentation.md` keeps both sources in the same comparison-only posture.
- `README.md` records the immutable source reference.
- No Cargo or Nix dependency changed.

## Checks

- `cargo test -p chaoscontrol-sim-core`: passed, 55 tests.
- `cargo clippy -p chaoscontrol-sim-core --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `nix flake check -L --option builders ''`: ran and failed at the pre-existing `checks.x86_64-linux.fmt` gate.

The formatting gate reports changes in `crates/chaoscontrol-wasm-differential/src/lib.rs`. This documentation change does not modify that file.

## Non-claims

The WalTier record creates no implementation, dependency, store-correctness, parity, equivalence, KVM-replay, or release-readiness claim.
