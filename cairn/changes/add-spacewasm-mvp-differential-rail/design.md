## Context

The active component exploration rail uses one wasm-tools cohort and Wasmtime Cranelift/Pulley execution. SpaceWasm currently accepts only WebAssembly 1.0 core modules with mutable globals and lacks the Component Model, WIT execution, merged WASI, bulk memory, sign extension, and non-trapping float-to-int conversion. It offers streaming decode, explicit host functions, configurable instruction-count execution, pause/out-of-fuel results, and fixed-page interpreter allocation, while guest linear memory remains separately host-owned.

The useful diversity is therefore an independent core-module decoder/interpreter, not component compatibility or performance competition. The lane must operate only on the exact admitted feature intersection and must distinguish unsupported input from an implementation disagreement.

## Decisions

### 1. A separate profile owns the core-MVP lane

A typed Nickel profile will bind the Mantle bundle, SpaceWasm commit/build/allocator configuration, Wasmtime cohort/configuration, wasm-tools generator cohort, allowed MVP features, import/export ABI, memory/table limits, `memory.grow` posture, chunk scheduling, instruction bounds, corpus classes, observation model, retention, and non-claims.

### 2. Admitted cases are core modules in the exact feature intersection

Components, WIT packages, WASI imports, unsupported post-MVP operators, and modules outside declared structural/resource bounds will be rejected or classified unsupported before differential execution. Profile rejection is not runtime divergence.

### 3. Stable observations exclude engine-specific counters

The comparator will normalize validation class, instantiation/link class, result values, trap class, canonical Preserves output, ordered hostcall transcript, selected globals/tables, final linear-memory BLAKE3, and bounded resource outcome. Raw Wasmtime fuel and SpaceWasm instruction counts have different semantics and remain engine-specific facts rather than equality fields.

### 4. Streaming schedules are explicit replay inputs

Each SpaceWasm decode run will bind an ordered chunk schedule whose lengths cover the exact module bytes without overlap, omission, or extra input. Schedules will include section/opcode/LEB boundary splits and malformed truncation cases generated under named bounds. Wasmtime receives identical complete bytes; the schedule is a SpaceWasm-path fact.

### 5. Resumption has an intra-engine oracle

For admitted modules and deterministic host effects, uninterrupted SpaceWasm execution and one or more bounded resume segments must produce the same normalized terminal observation. A pause requested by an admitted host function is distinct from out-of-fuel. The rail will not claim canonical state serialization or cross-host migration.

### 6. Differential and resume failures shrink under typed predicates

Shrinking may remove or simplify module structure, inputs, and chunk schedules only while preserving the selected boundary and mismatch class. A candidate that becomes unsupported, fails earlier for an unrelated reason, or loses the same runtime pair/profile identity is rejected.

### 7. Neither runtime is the oracle

A match records tested observational agreement. A divergence records both observations and preserves the case for triage. The evidence never labels SpaceWasm or Wasmtime correct without an independent expected result or specification fixture.

### 8. The lane remains diagnostic-only

SpaceWasm is version `0.0.0` with no release and known missing tooling/features. All results remain host-side diagnostic evidence and cannot satisfy VM replay, sandbox, production runtime, package trust, or release gates.

## Functional core / imperative shell split

- **Pure core**: profile/cohort/case admission, feature-intersection checks, chunk-schedule validation, observation normalization, match/divergence classification, resume-equivalence predicates, shrink-step admission, identity material, retention plans, and evidence DTO construction.
- **Imperative shell**: bundle remeasurement, corpus generation/mutation, runtime store allocation, SpaceWasm streaming decode/execution, Wasmtime execution, observation capture, shrinker invocation, persistence, and report rendering.

## Risks / Trade-offs

- SpaceWasm incorporates some Wasmtime-derived material and shared WebAssembly tests, so implementation diversity is useful but not absolute independence.
- Floating-point NaN payloads and diagnostics may differ while conforming. The observation profile must normalize only reviewed equivalence classes and retain raw facts for triage.
- A restrictive feature intersection may reject most component-oriented corpora. The lane needs its own MVP corpus instead of silently lowering arbitrary components.
- SpaceWasm's unsafe containers are part of the system under test, not trusted evidence infrastructure.
- Differential execution is costlier than a single backend. Fast fixed corpora and separately bounded deep exploration are required.

## Non-Goals

- No SpaceWasm Component Model, WIT, WASI, or post-MVP extension implementation.
- No cross-runtime performance ranking.
- No claim that matching runtime results prove WebAssembly conformance or memory safety.
- No production SpaceWasm host or guest-language SDK.
- No merging with VM snapshot-backed replay evidence classes.
