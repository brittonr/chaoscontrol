# Verification: adopt-nickel-1-17-cohort

r[verify chaoscontrol.nickel_toolchain.cohort]
r[verify chaoscontrol.nickel_toolchain.lockfile]
r[verify chaoscontrol.nickel_toolchain.boundary]
r[verify chaoscontrol.nickel_toolchain.compatibility]
r[verify chaoscontrol.nickel_toolchain.validation]

## Implemented boundary

- `flake.nix` pins Nickel CLI `1.17.0` at upstream commit `1320a983e6c3d1e2fb53dd2464b084b4903b1426`.
- A Nix-generated lock entry binds that source. An overlay routes all Nix profile, evidence, fixture, and developer uses through the exact package.
- Rust evidence tools no longer fall back to ambient `nixpkgs#nickel` execution.
- Profile receipts bind `nickel-lang-cli nickel 1.17.0 (rev 1320a98)` and `blake3:bb5e202a62d399506f1eecaa8cb803108db19a0845505e927be416c0c442a09a`.
- Nickel profile acceptance remains pre-run intent. ChaosControl retains run admission and all simulation meaning.

## Positive evidence

- Baseline before core changes: `nix develop -c cargo test -p chaoscontrol-evidence profile_projection`: 12 passed.
- `nix build .#checks.x86_64-linux.nickel-cohort-exact --no-link -L --option builders ''`: passed.
- `nix develop -c cargo run -p chaoscontrol-evidence --bin check-profile-projections -- --root .`: passed.
- `nix develop -c cargo run -p chaoscontrol-evidence --bin check-contract-registry -- .`: passed with 21 families.
- `nix develop -c cargo run -p chaoscontrol-evidence --bin check-kvm-release-matrix -- --root .`: passed with seven rows and ten fixtures.
- `nix develop -c cargo test -p chaoscontrol-evidence`: 140 unit tests plus integration tests passed.
- `nix develop -c cargo clippy -p chaoscontrol-evidence --all-targets -- -D warnings`: passed.
- `nix develop -c cargo fmt --all -- --check`: passed.

## Negative evidence

- The exact cohort check rejects the former `1.15.1` identity and ambient `nixpkgs#nickel` fallback source.
- Malformed source, missing imports, contract violations, unknown fields, zero bounds, and unknown fault actions fail.
- The first cohort-check run found three remaining Rust fallback paths. Those paths now fail closed when the exact `nickel` executable is unavailable.
- An intentionally incorrect `check-kvm-release-matrix .` invocation was rejected. The documented `--root .` invocation passed.

## Non-claims

The checks prove the selected source identity and bounded profile compatibility. They do not prove Nickel correctness, simulation correctness, guest correctness, replay success, finding truth, or release readiness beyond the recorded gates.
