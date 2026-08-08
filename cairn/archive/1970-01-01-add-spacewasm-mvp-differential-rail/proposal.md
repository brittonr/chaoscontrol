## Why

ChaosControl's active Wasm component exploration change compares Wasmtime Cranelift and Pulley strategies. That detects backend divergence, but both strategies share Wasmtime parsing, validation, embedding, and substantial runtime machinery. The design already records that agreement can miss shared bugs.

SpaceWasm provides a materially different streaming decoder and interpreter for a narrower WebAssembly 1.0 core-module subset. A separate diagnostic lane can compare independently implemented validation and execution boundaries, exercise deterministic chunking and allocation failure, and test resumable instruction stepping. SpaceWasm cannot execute WIT components and must not be inserted into the component lane as if it were another component backend.

## What Changes

- Add a typed Nickel `spacewasm-mvp` differential profile for the exact intersection of the admitted SpaceWasm and Wasmtime core-module feature sets.
- Consume and remeasure a commit-pinned Mantle SpaceWasm reference bundle rather than fetching or compiling floating upstream source inside exploration.
- Generate and mutate deterministic MVP core-module cases with recorded seeds, chunk schedules, host ABI, memory policy, and expected boundary classes.
- Compare SpaceWasm and Wasmtime validation, instantiation, normalized result/trap, canonical Preserves output, hostcall transcript, observable final state, and resource outcome without comparing raw engine-specific fuel counters.
- Exercise SpaceWasm uninterrupted versus bounded pause/out-of-fuel/resume execution and compare the final normalized observation.
- Add stable differential and resume predicates for replay and shrinking.
- Keep unsupported proposals, components, WIT, WASI, missing bundles, and cohort drift as explicit skip/denial classes rather than runtime divergences.
- Emit diagnostic-only evidence separate from Wasmtime strategy evidence, VM snapshot replay proof, assertion readiness, sandbox proof, and release eligibility.

## Impact

- **Surfaces**: host-side Wasm exploration profiles, core-module corpus generation, runtime adapters, normalized observations, replay/shrink predicates, evidence DTOs/contracts, and retention.
- **Dependencies**: requires an exact Mantle SpaceWasm reference bundle; may consume Octet static profile facts but does not transfer Octet policy semantics.
- **Relationship to active work**: this is a sibling core-MVP lane to `add-adversarial-wasm-component-rail`, not an expansion of SpaceWasm into Component Model execution.
- **Claims**: runtime agreement is bounded differential evidence only. Divergence does not identify the correct runtime, and agreement does not prove either implementation correct.
