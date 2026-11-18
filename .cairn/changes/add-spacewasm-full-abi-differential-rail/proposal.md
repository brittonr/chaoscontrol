## Why

The SpaceWasm MVP differential rail compares only hostless WebAssembly 1.0 modules, on the Wasmtime 41.0.3 CLI. Molten runs SpaceWasm actors through the `molten.wasm.abi.v1` surface: one bounded memory, `molten_alloc`, `molten_dealloc` and `molten_run` exports, and `molten:hostcall` `assert` and `send` imports. Molten and the shared hardened runtime use the patched Wasmtime 49.0.5 cohort. Mantle's `materialize-molten-spacewasm-abi` bundle now carries full-ABI and segmentation fixtures whose descriptors require a consumer-owned execution receipt. ChaosControl owns that differential observation. Until it exists, nothing compares SpaceWasm and Wasmtime on the surface Molten actually executes.

## What Changes

- Add a separate typed full-ABI differential profile. It binds the Mantle SpaceWasm bundle, admitted only through a release-authority publication receipt, together with the patched Wasmtime 49.0.5 embedding cohort, the exact `molten.wasm.abi.v1` surface, the scenario matrix, bounds and non-claims. The MVP profile and rail stay unchanged.
- Add pure full-ABI module admission: exact hostcall imports and signatures, exactly one bounded memory, no table, no start function or `memory.grow`, and the exact four-export ABI set.
- Add a Rust-owned Wasmtime 49.0.5 embedding harness and a consumer-owned SpaceWasm host. Both serve the same deterministic hostcall implementation. Compare normalized results, traps, canonical output, ordered hostcall transcripts and cumulative fuel.
- Run the positive and negative ABI, hostcall, memory, trap, stack and cumulative-fuel scenarios against the exact bundle member bytes, and record ChaosControl-owned receipts.

## Impact

- **Surfaces**: a full-ABI profile contract and fixtures, pure admission and comparison code in `chaoscontrol-wasm-differential`, an embedding harness, a SpaceWasm host runner and a Nix check.
- **Dependencies**: the Mantle `materialize-molten-spacewasm-abi` bundle identity, bound through release authority (not yet granted), and the patched Wasmtime 49.0.5 cohort that Molten and the shared hardened runtime use.
- **Non-claims**: no SpaceWasm correctness, SpaceWasm/Wasmtime equivalence, memory safety, WebAssembly conformance, Molten runtime correctness, flight qualification, sandbox effectiveness, production readiness or release eligibility. This change does not publish the Mantle bundle.
