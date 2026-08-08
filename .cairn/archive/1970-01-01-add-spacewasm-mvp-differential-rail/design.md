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

### 9. The exact Mantle producer is a pinned Nix input

The rail consumes Mantle commit `a141fcbaafe41f9a413a81275a33fe915bfca370`. It remeasures manifest BLAKE3 `4ff6a7794cf54fd0000326e7505ba8496f4f3f7c4ddd88d1f876373d652a8b65`, bundle identity `cee7190f2f78321b07f3d1f493baaa5b2cb74d517eb4f229c7e7a6094b877342`, and every declared member before execution. The Nix input follows ChaosControl's existing Crane, Octet, and Tiger Style inputs only where those inputs do not construct the SpaceWasm bundle.

### 10. Resume evidence uses a consumer-owned offline probe

The retained Mantle runner covers the fixed fixture contract but does not expose continuation observations. The Nix rail therefore builds `tools/spacewasm-resume-probe.rs` from the bundle's exact source archive, vendor closure, and Rust toolchain. The probe compares uninterrupted execution with repeated one-instruction segments and requires at least one `OutOfFuel` boundary. It also compares complete-byte and one-byte streaming decode of the same generated module.

### 11. Generated mismatches shrink under one exact predicate

The pure core emits deterministic smaller instruction-module candidates. The shell accepts a candidate only when the runtime pair, profile, and first normalized difference remain unchanged. The report retains the minimized bytes, original and minimized BLAKE3 values, predicate field, and bounded attempt count.

## Functional core / imperative shell split

- **Pure core**: profile, cohort, and case admission; exact-set checks; deterministic MVP generation; normalized observation comparison; first-difference predicates; shrink planning; identity material; and evidence DTO construction.
- **Imperative shell**: bundle remeasurement, bounded process execution, generated-input retention, SpaceWasm and Wasmtime invocation, shrink-candidate execution, persistence, and report rendering. The separate resume probe owns only the exact SpaceWasm allocation, decode, invocation, and segmented-execution effects needed for its bounded oracle.

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
