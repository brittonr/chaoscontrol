# Wasm Component Exploration Specification

## Purpose

Generate, mutate, execute, compare, replay, and shrink bounded WebAssembly component cases through a Rust-owned ChaosControl harness without broadening guest SDK or evidence claims.

## Requirements

### Requirement: Wasm exploration uses one versioned profile

r[chaoscontrol.wasm_component.profile] ChaosControl MUST define a typed Wasm exploration profile that binds the wasm-tools generator/mutator/shrinker cohort, optional Waffle cohort, Wasmtime strategies, WIT/component profile, deterministic runtime configuration, corpus classes, observation model, bounds, and non-claims.

#### Scenario: Case matches admitted cohort
- GIVEN a case and profile identify one supported complete Octet/Aspen-compatible cohort
- WHEN exploration admission runs
- THEN ChaosControl MAY schedule the case under that exact profile identity.

#### Scenario: Tool or runtime cohort drifts
- GIVEN a generator, parser, mutator, shrinker, transform, Wasmtime, WIT, or runtime configuration differs from the admitted profile
- WHEN admission runs
- THEN the case MUST be denied or classified diagnostic-only before proof-eligible execution.

### Requirement: Generated corpora are exactly reproducible

r[chaoscontrol.wasm_component.corpus] Every generated module, WIT package, and component case MUST bind generator kind/version, seed bytes, configuration, parent inputs, componentization inputs, expected validity class, and exact output BLAKE3, and bounded regeneration MUST reproduce exact bytes.

#### Scenario: Seed is replayed
- GIVEN a saved corpus manifest and supported cohort
- WHEN regeneration runs
- THEN the produced artifact BLAKE3 MUST match the manifest.

#### Scenario: Regeneration differs
- GIVEN identical declared inputs produce different bytes or validity class
- WHEN corpus verification runs
- THEN ChaosControl MUST classify the case as replay mismatch and MUST NOT treat it as reproducible evidence.

### Requirement: Mutations retain explicit outcome classes

r[chaoscontrol.wasm_component.mutation] Mutation cases MUST bind input artifact, mutator/version, seed, transform sequence, profile, expected validity/outcome class, and output identity, and MUST distinguish static rejection, profile rejection, compile rejection, link rejection, deterministic trap, and result mismatch.

#### Scenario: Valid mutation reaches execution
- GIVEN a mutation remains statically/profile valid
- WHEN the harness evaluates it
- THEN it MAY proceed to compile/link/execute and MUST retain each boundary outcome.

#### Scenario: Invalid mutation is rejected correctly
- GIVEN a mutation intentionally violates a declared static or profile rule
- WHEN the corresponding boundary runs
- THEN the case MUST pass only if it fails at the expected boundary/class.

### Requirement: Strategy differential compares normalized outcomes

r[chaoscontrol.wasm_component.differential] Executable cases MUST run with identical component/profile/input/recorded-effect facts under the selected Wasmtime Cranelift and Pulley strategies and MUST compare normalized result/trap class, canonical Preserves output, hostcall transcript, resource class, and final state identity.

#### Scenario: Strategies agree
- GIVEN both strategies complete under identical admitted facts
- WHEN normalized comparison runs
- THEN a strategy-match receipt MUST bind both executions and matching observations.

#### Scenario: Strategies diverge
- GIVEN result, trap, canonical output, hostcall, resource, or final state observations differ
- WHEN comparison runs
- THEN ChaosControl MUST emit strategy-divergence evidence and retain the case for replay/shrinking without claiming which strategy is correct.

### Requirement: Wasm transforms remain experimental evidence

r[chaoscontrol.wasm_component.transforms] Optional Waffle round-trip or instrumentation cases MUST bind original bytes, transform configuration, transformed bytes, validation, and normalized execution outcomes and MUST remain recorded-only without semantic-equivalence claims.

#### Scenario: Original and transformed observations match
- GIVEN both artifacts validate and produce matching bounded observations
- WHEN transform comparison runs
- THEN ChaosControl MAY record transform-match but MUST NOT claim general behavior preservation.

#### Scenario: Transform changes outcome
- GIVEN transformed validation or normalized execution differs
- WHEN comparison runs
- THEN ChaosControl MUST record transform-divergence with both artifact identities.

### Requirement: Failing cases shrink under a stable predicate

r[chaoscontrol.wasm_component.shrink] ChaosControl MUST define a typed failure predicate over normalized outcome class and required identities, MUST accept a shrink step only when the predicate remains true, and MUST bind every accepted step and final candidate.

#### Scenario: Divergence is minimized
- GIVEN a reproducible strategy divergence and bounded shrink budget
- WHEN shrinking runs
- THEN every accepted smaller artifact MUST reproduce the same declared divergence predicate.

#### Scenario: Candidate changes failure class
- GIVEN a smaller artifact fails for an unrelated parser, profile, compile, link, or trap reason
- WHEN shrink admission runs
- THEN ChaosControl MUST reject that step.

### Requirement: Exploration is bounded

r[chaoscontrol.wasm_component.bounds] The profile MUST enforce named bounds for artifact/WIT size, structural counts, memory/table declarations, fuel, hostcall bytes, generation/mutation/branch/shrink counts, execution steps, concurrency, samples, and retained artifacts.

#### Scenario: Case exceeds bound
- GIVEN generation, validation, compilation, execution, or shrinking would exceed a declared bound
- WHEN the boundary is reached
- THEN ChaosControl MUST stop the operation and emit a typed bound-skip or denial without silently increasing the limit.

### Requirement: Wasm evidence classes remain separate

r[chaoscontrol.wasm_component.evidence] ChaosControl MUST emit distinct static/profile/compile/link/trap/match/divergence/transform/replay/shrink/bound/harness classes and MUST NOT merge them with VM snapshot-backed replay proof, assertion readiness, package trust, semantic equivalence, sandbox proof, or release eligibility.

#### Scenario: Host-side strategy match is reported
- GIVEN a component matches across selected strategies
- WHEN evidence is classified
- THEN it MUST remain bounded host-side Wasm differential evidence and MUST NOT satisfy a VM replay-proof requirement.

#### Scenario: Evidence requests universal correctness
- GIVEN a report claims general Wasm, compiler, runtime, or transform correctness from bounded cases
- WHEN evidence validation runs
- THEN ChaosControl MUST reject the overclaim.

### Requirement: Exploration decisions have a functional core

r[chaoscontrol.wasm_component.functional_core] Profile/case validation, seed/identity construction, outcome classification, normalized comparison, failure predicates, shrink admission, bound decisions, retention plans, and evidence DTO construction MUST be pure deterministic logic.

#### Scenario: Identical observations produce identical classification
- GIVEN identical profile, case, execution, transform, and shrink facts
- WHEN the core classifies them
- THEN it MUST produce the same identities, decisions, and diagnostics without filesystem, environment, process, clock, network, runtime, KVM, or output effects.

### Requirement: Wasm exploration has positive and negative validation

r[chaoscontrol.wasm_component.validation] The rail MUST include positive corpus replay/profile/strategy-match cases and negative malformed, profile-denied, compile/link, trap, divergence, replay, shrink-class, bound, evidence, and overclaim cases plus focused lifecycle validation.

#### Scenario: Exploration rail changes
- GIVEN profile, generator, mutator, runtime, transform, comparison, shrinker, bound, or evidence behavior changes
- WHEN validation evidence is assembled
- THEN it MUST include deterministic positive and negative cases, bounded smoke execution, evidence-contract checks, and Cairn gates while preserving the Rust-only SDK boundary.
