# Validation evidence

Date: 2026-08-08

## Exact cohort

- Mantle commit: `a141fcbaafe41f9a413a81275a33fe915bfca370`
- SpaceWasm commit: `e24cf09355a90497148eb5029fdb8e3400bd63e3`
- Mantle bundle identity: `cee7190f2f78321b07f3d1f493baaa5b2cb74d517eb4f229c7e7a6094b877342`
- Bundle manifest BLAKE3: `4ff6a7794cf54fd0000326e7505ba8496f4f3f7c4ddd88d1f876373d652a8b65`
- SpaceWasm runner BLAKE3: `be8aeb698afdecf6fb608910980292517ed952f122b6447705d4bdae485b0221`
- Wasmtime: `41.0.3`

## Differential and resume result

The exact Nix rail completed 14 comparisons with no normalized mismatch.

- Differential report identity: `29cbb2f1518247bafad4155e501b133ecae827a0cd3f7583e252beb091874896`
- Profile identity: `d396464dafdaa63bac95cecb39090496a743f4d2b801b1aa01f488334e617c8c`
- Resume module BLAKE3: `ecde7b7770adc2d059f513403f802fce2bdf4c25004344098abd3bef1ed79ec5`
- Resume probe BLAKE3: `0ca89f6d718b427451e9c699f1e5b14a3e03d4108200d34c2bc7024a34757557`
- Segmented result: `finished` after 33 one-instruction segments
- Streaming result: `finished` with one-byte chunks

The resume report explicitly records that hostless interpreter state is not observable. It does not claim portable interpreter-state serialization.

## Commands

These checks passed:

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -q -p chaoscontrol-evidence --bin check-evidence-contracts -- --root .
nixfmt --check flake.nix
nix flake check --no-build
nix build -L --no-link \
  .#checks.x86_64-linux.spacewasm-mvp-differential \
  .#checks.x86_64-linux.evidence-contracts \
  .#checks.x86_64-linux.tests \
  .#checks.x86_64-linux.clippy \
  .#checks.x86_64-linux.rustdoc
```

The Nix client could not connect to `ssh-ng://root@10.10.10.1`. Nix completed each derivation on the local builder.

## Evidence boundary

This evidence reports bounded agreement for the exact profile and corpus. It does not prove SpaceWasm correctness, SpaceWasm/Wasmtime equivalence, memory safety, WebAssembly conformance, flight qualification, sandbox effectiveness, production readiness, or release eligibility.
