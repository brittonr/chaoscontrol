## Context

The MVP rail (`spacewasm-mvp-differential`) admits hostless `run: () -> ()` modules with no imports, memory or table, and runs Wasmtime through the locked 41.0.3 CLI. That CLI cannot supply Molten hostcalls, so it cannot execute the full ABI.

Molten owns `molten.wasm.abi.v1`. Its SpaceWasm inspection admits only these modules:

- function imports from `molten:hostcall` named `assert` or `send`, each `(i32, i32) -> i64`, with no duplicate;
- exactly one bounded 32-bit memory with a non-zero initial size, no shared or custom-page memory, and no table;
- no start function and no `memory.grow`;
- exactly four exports: `memory` (memory zero), `molten_alloc: (i32) -> i32`, `molten_dealloc: (i32, i32) -> ()` and `molten_run: (i32, i32) -> i64`.

Molten's host loads `(ptr, len)` request bytes bounded by the profile limit and requires canonical Preserves. It stores the canonical reply at guest offset 4096 and returns the descriptor `(4096 << 32) | len`; a reply may also request a pause. Each call is receipted as name, request identity and response.

Mantle's `materialize-molten-spacewasm-abi` bundle (verified identity `774331f6c2defe75e30c354b2b3080838cf56438bbd99268779fe4590f8c9f39` on 2026-10-30) carries full-ABI positive and negative modules and a segmentation module with observe and pause imports. Its descriptors require a consumer-owned execution receipt, and its design reserves bundle publication for release authority.

## Decisions

1. **Separate profile.** A new schema, `chaoscontrol.spacewasm-full-abi-differential-profile.v1`, covers the full ABI. The MVP schema, rail and Nix check keep their scope; a full-ABI case never runs under the MVP profile.
2. **Pinned ABI contract.** The profile names `molten.wasm.abi.v1` and records its exact import, memory and export surface. ChaosControl admits modules against that record, not against Molten source. A change to Molten's ABI is a new profile, not a silent widening.
3. **Embedded engines.** The Wasmtime side is a Rust harness that embeds the patched Wasmtime 49.0.5 crate cohort. The SpaceWasm side is a consumer-owned host built offline from the bundle's source closure, like the existing resume probe. Neither engine receives WASI, filesystem, network, environment, clock, randomness or any import outside the declared hostcalls.
4. **One deterministic host.** Both engines call the same pure hostcall model. It validates canonical request bytes, answers from the scenario's declared replies, writes them at the ABI reply offset and records an ordered transcript of name, request identity and response identity. The transcript and the replies are compared, never re-derived per engine.
5. **Release-bound inputs.** Evidence-bearing runs admit the Mantle bundle only when a release-authority publication receipt binds its producer revision and identity. A locally built bundle with identical bytes runs only as a diagnostic and cannot satisfy the evidence class.
6. **Cumulative fuel.** Segmented execution (pauses and out-of-fuel resumes) must match uninterrupted execution in result, transcript and total consumed fuel. Resumption can never replenish the admitted budget.
7. **Functional core.** Profile, module and scenario admission, the hostcall model, observation normalization, comparison, fuel accounting and evidence DTOs stay pure. Process, filesystem, engine and clock work stays in the shell.

## First slice

Start with the profile contract, its positive and negative fixtures, and pure full-ABI module admission. They need neither the published bundle nor the embedding harness, and they fix the surface the later engine work must honor.

## Risks

- The patched Wasmtime 49.0.5 crate must come from the same cohort Molten uses. A differently patched build would compare against the wrong reference.
- Molten may revise `molten.wasm.abi.v1`. Drift fails admission until the profile is updated deliberately.
- Release authority may not publish the 2026-10-30 bundle unchanged. The profile then pins the published identity, and fixtures are re-verified against it.

## Non-claims

A matching report proves only that the two selected engine builds produced the same normalized observations for the admitted bounded scenarios. It does not prove SpaceWasm correctness, SpaceWasm/Wasmtime equivalence, memory safety, WebAssembly conformance, Molten runtime correctness, flight qualification, sandbox effectiveness, production readiness or release eligibility.
