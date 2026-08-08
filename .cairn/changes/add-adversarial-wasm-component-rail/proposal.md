## Why

ChaosControl already provides deterministic seeds, coverage-guided branching, fault schedules, minimization, replay, and typed evidence for Rust workloads, but it has no bounded adversarial rail for WebAssembly component artifacts and runtimes. Octet and Aspen will rely on wasm-tools parsing and a deterministic Wasmtime component profile; malformed, unusual, or valid-but-hostile artifacts need reproducible generation, mutation, differential execution, shrinking, and classified evidence before those boundaries are trusted.

The rail must remain a Rust-owned test harness rather than reopening ChaosControl's language-agnostic SDK or hosted-product scope.

## What Changes

- Add a versioned host-side Wasm component exploration profile bound to the shared Mantle/Octet/Aspen materialization, artifact, and runtime cohorts.
- Use Mantle to materialize the pinned exploration tool closure, host harness, baseline components, and promoted fixed-seed regression corpora; keep iterative mutation, execution, shrinking, and scheduling in ChaosControl.
- Generate bounded deterministic module, WIT, and component corpora with wasm-smith, wit-smith, and component encoders from recorded seeds.
- Apply wasm-mutate and deterministic profile-aware mutations, then replay and shrink failures with wasm-shrink or a bounded predicate-preserving reducer.
- Compare static validation and normalized execution outcomes across selected Wasmtime Cranelift and Pulley strategies under identical deterministic inputs.
- Permit optional Waffle round-trip or instrumentation experiments only with original/transformed identity and differential receipts.
- Enforce explicit fuel, memory, table, artifact-size, generation, mutation, branch, sample, and wall-independent step bounds.
- Keep Wasm exploration evidence separate from VM snapshot-backed replay proof, assertion readiness, package trust, semantic equivalence, and release eligibility.

## Impact

- **Surfaces**: a Rust host harness, corpus manifests, generator/mutator/shrinker adapters, deterministic execution profiles, exploration scheduling, evidence DTOs/contracts, fixtures, and optional Nix checks.
- **Scope**: no new guest-language SDK, component package manager, production Wasm host, WAMR support, or cross-runtime benchmark product.
- **Dependencies**: the first admitted cohort should match the Mantle materialization bundle, Octet static artifact rail, and Aspen deterministic component profile; missing or mismatched cohort evidence keeps cases diagnostic-only.
- **Claims**: differential agreement and replay prove only the bounded tested artifacts, strategies, inputs, and observation model.
