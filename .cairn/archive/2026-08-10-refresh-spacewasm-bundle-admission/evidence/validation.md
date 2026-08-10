# Validation evidence

Date: 2026-08-10

## Baseline

Unchanged `origin/main` failed the focused check before runtime execution:

```text
bundle manifest digest mismatch: expected=4ff6a7794cf54fd0000326e7505ba8496f4f3f7c4ddd88d1f876373d652a8b65 actual=39e4790a7b9d0b14fcafffe5810e268cd8af342d38d7e952a6ede923e33882b2
```

## Producer verification

The exact Mantle verifier returned:

- schema: `mantle-spacewasm-bundle-verify-summary-v1`
- valid: `true`
- bundle identity: `c4826bb63fa9eef1fa619e0f0c4c2c35dd10ca92a8d4999fec10c55e92b692b7`
- diagnostics: empty

## Consumer validation

These checks passed:

- `cargo fmt --all -- --check`
- `cargo test -p chaoscontrol-wasm-differential --all-targets`: 7 passed
- `cargo clippy -p chaoscontrol-wasm-differential --all-targets -- -D warnings`
- Nickel profile export exactly matched the checked JSON projection
- the post-MVP negative Nickel fixture failed as required
- `check-product-scope --write` followed by the read-only product-scope check
- `check-evidence-contracts`
- strict Cairn validation
- `nix build .#checks.x86_64-linux.spacewasm-mvp-differential -L`

The focused differential rail compared 14 cases with no mismatch. Its report identity is `2fb7f4d0f1098711889a09974f4d4b69dafe65fc2deda89f435577449e6003ea`. The resume probe finished after 33 one-instruction segments and matched uninterrupted execution.

Full `nix flake check -L` passed all 38 checks, including workspace tests, Clippy, Rustdoc, evidence contracts, the focused SpaceWasm rail, KVM smoke, and negative policy checks.

The remote builder connection failed as before. Nix used the local builder.

## Evidence boundary

This evidence admits only the exact bounded diagnostic cohort. It does not prove SpaceWasm correctness, SpaceWasm and Wasmtime equivalence, WebAssembly conformance, sandbox effectiveness, production readiness, or release eligibility.
