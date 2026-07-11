## Context

wasm-tools provides deterministic generators, mutators, and shrinkers for valid Wasm/WIT inputs. Waffle can round-trip core Wasm through an SSA IR and is fuzzed by differential execution, while Wasmtime provides multiple execution strategies including Cranelift and Pulley. These are useful adversarial inputs but do not by themselves supply ChaosControl's seed, bound, replay, minimization, and evidence classifications.

The first rail should run as a Rust-owned host harness and may later be packaged as a VM workload. This preserves the current Rust-only SDK boundary and avoids treating arbitrary component languages as supported workload APIs.

## Decisions

### 1. One versioned exploration profile binds the cohort

**Choice:** Define a typed Nickel profile naming the Mantle materialization bundle, wasm-tools generator/mutator/shrinker versions, optional Waffle version, Wasmtime cohort/strategies, WIT/component profile, deterministic runtime configuration, corpus classes, limits, observation model, and non-claims. A case is proof-eligible only when it matches the admitted Mantle/Octet/Aspen cohort vector.

**Rationale:** Parser/runtime drift can change whether a seed is valid or how a trap is classified.

### 2. Corpus generation is deterministic and reconstructable

**Choice:** Each generated case binds generator kind/version, seed bytes, generation configuration, WIT/package inputs, componentization inputs, output BLAKE3, expected validity class, and parent case. Checked-in small cases or bounded regeneration must reproduce exact bytes.

**Rationale:** A random failing blob without reconstructable inputs cannot support replay or minimization.

### 3. Mutations preserve an explicit expected class

**Choice:** Mutation plans identify input, mutator/version, seed, transform sequence, profile, expected valid/invalid class, and output. Static parser rejection, profile rejection, compile rejection, link rejection, deterministic trap, and result mismatch remain distinct outcome classes.

**Rationale:** Combining all failures into one crash class hides whether a check failed correctly.

### 4. Differential execution compares normalized outcomes

**Choice:** For cases admitted to execution, run the same component/profile/input/recorded effects under selected Wasmtime Cranelift and Pulley strategies. Compare normalized result/trap class, canonical Preserves output, hostcall transcript, resource class, and final state identity—not timing or target-specific diagnostics.

**Rationale:** Strategy agreement can expose runtime/backend divergence, but raw logs and performance are not portable comparison surfaces.

### 5. Waffle transforms are experimental inputs

**Choice:** Optional Waffle round-trip or instrumentation records original bytes, transform configuration, transformed bytes, validation facts, and differential outcomes. A clean comparison is recorded-only and never semantic-equivalence proof.

**Rationale:** A transform can preserve tested observations while differing outside the bounded inputs.

### 6. Failures shrink under a stable predicate

**Choice:** The pure core defines a typed failure predicate over normalized outcome class and required identities. The shell invokes wasm-shrink or a bounded reducer, reruns the case, and accepts a smaller artifact only when the predicate remains true. Receipts retain every accepted shrink step and final minimal candidate.

**Rationale:** Minimization must not silently change a parser failure into an unrelated trap.

### 7. All exploration dimensions are bounded

**Choice:** Profiles name artifact/WIT size, function/type/import/export counts, memory/table declarations, fuel, hostcall bytes, generation/mutation counts, branches, shrink attempts, runtime steps, concurrent cases, and retained artifacts through named fields. Exceeding a bound produces a typed skipped/denied class.

**Rationale:** Hostile artifacts can otherwise turn the test rail into an unbounded compiler/runtime workload.

### 8. Evidence classes stay separate

**Choice:** Emit `static-rejection`, `profile-rejection`, `compile-rejection`, `link-rejection`, `deterministic-trap`, `strategy-match`, `strategy-divergence`, `transform-match`, `transform-divergence`, `replay-mismatch`, `shrink-complete`, `bound-skip`, and `harness-error` classes. These artifacts do not count as VM snapshot-backed proof, assertion coverage, correctness, package trust, sandbox proof, or release eligibility.

### 9. Mantle materializes stable exploration inputs

**Choice:** Mantle produces rehashable bundles for the pinned host harness/tool closure, baseline components, WIT packages, and fixed-seed corpora promoted into regression lanes. ChaosControl verifies those identities, then owns iterative generation, mutation, strategy execution, shrinking, retention, and evidence. Newly discovered cases remain exploration artifacts until an explicit promotion regenerates them through Mantle; the live search loop is never forced into a build derivation.

**Rationale:** Stable inputs benefit from Mantle's reproducibility and cache, while stateful bounded search and minimization remain test-runtime behavior owned by ChaosControl.

## Functional core / imperative shell split

- **Pure core**: profile/materialization/case validation, seed and identity material, expected/outcome classification, normalized comparison, failure predicates, shrink-step admission, promotion decisions, bound decisions, retention plans, and evidence DTO construction.
- **Imperative shell**: rehash Mantle baselines, invoke generator/mutator/shrinker/transform/runtime libraries or tools, allocate stores, execute components, collect observations, persist exploration artifacts, and render reports.

## Risks / Trade-offs

- Generator and runtime matrices can become expensive. Keep a Mantle-materialized fast deterministic corpus and separately scheduled deep exploration.
- Pulley is substantially slower and its precompiled bytecode is version-specific. Compare portable source cases and normalized outcomes, not durable Pulley bytecode.
- Waffle currently targets core Wasm transforms, so component cases may require explicit extraction/recomposition and remain experimental.
- Differential agreement can miss shared bugs; explicit non-claims and independent static/negative fixtures remain required.

## Non-Goals

- No language-agnostic guest SDK or support commitment for JavaScript, Python, C, Go, or other component languages.
- No cross-runtime ranking, WAMR integration, performance benchmark, or universal Wasm correctness claim.
- No promotion of generated artifacts into trusted packages or production executables; promotion into a regression corpus requires explicit Mantle rematerialization and remains test evidence only.
- No merging of host-side Wasm exploration with VM snapshot-backed replay evidence classes.
