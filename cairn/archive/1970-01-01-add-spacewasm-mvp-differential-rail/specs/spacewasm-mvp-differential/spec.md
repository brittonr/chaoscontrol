# SpaceWasm MVP Differential Specification

## Purpose

Generate, execute, compare, replay, and shrink bounded WebAssembly 1.0 core-module cases across exact SpaceWasm and Wasmtime cohorts while keeping results diagnostic-only and distinct from component and VM evidence.

## Requirements

### Requirement: The differential lane uses one typed profile

r[chaoscontrol.spacewasm_mvp.profile] ChaosControl MUST define a typed profile that binds the exact Mantle SpaceWasm bundle, Wasmtime and wasm-tools cohorts, admitted MVP feature intersection, host ABI, runtime configurations, limits, chunk scheduling, observation model, retention, and non-claims.

#### Scenario: Complete matching profile is admitted
- GIVEN all bundle, runtime, tool, feature, ABI, and bound identities match
- WHEN profile admission runs
- THEN the case MAY enter the SpaceWasm MVP differential lane.

#### Scenario: Profile or cohort drifts
- GIVEN any required identity or generated profile projection differs
- WHEN admission runs
- THEN the case MUST be denied or classified diagnostic-only before differential execution.

### Requirement: Stable inputs come from a remeasured Mantle bundle

r[chaoscontrol.spacewasm_mvp.materialization] ChaosControl MUST remeasure every required SpaceWasm runtime, profile, fixture, license, and report member from a complete Mantle reference bundle and MUST NOT fetch, patch, or build floating upstream source inside exploration.

#### Scenario: Exact bundle verifies
- GIVEN a complete bundle whose portable members match their BLAKE3 identities
- WHEN materialization admission runs
- THEN ChaosControl MAY use its runtime and fixture members and MUST retain the bundle identity in every result.

#### Scenario: Bundle is incomplete or stale
- GIVEN a required member, digest, role, parent edge, or cohort identity is missing or mismatched
- WHEN admission runs
- THEN execution MUST remain blocked without fallback acquisition.

### Requirement: Only the admitted core-MVP intersection executes

r[chaoscontrol.spacewasm_mvp.execution] ChaosControl MUST execute only core modules whose binary features, imports, exports, memories, tables, growth posture, and structural bounds are accepted by both declared runtime profiles.

#### Scenario: MVP case satisfies both profiles
- GIVEN a core module uses only admitted features and resources
- WHEN execution admission runs
- THEN both runtime shells MAY receive identical module, input, and recorded host-effect facts.

#### Scenario: Case uses an unsupported surface
- GIVEN a case is a component, requires WIT/WASI, uses a disallowed proposal, or exceeds a declared bound
- WHEN admission runs
- THEN it MUST be classified as profile rejection or unsupported and MUST NOT be labeled runtime divergence.

### Requirement: Streaming schedules exactly cover module bytes

r[chaoscontrol.spacewasm_mvp.streaming] Every SpaceWasm decode schedule MUST be an ordered bounded partition of the exact module bytes and MUST bind schedule identity, generation inputs, expected class, and returned-buffer behavior.

#### Scenario: Valid boundary schedule is replayed
- GIVEN a schedule covers every module byte exactly once in order
- WHEN SpaceWasm streaming decode runs
- THEN the run MUST retain that schedule identity and return each consumed chunk through the declared interface.

#### Scenario: Schedule omits, overlaps, reorders, or extends bytes
- GIVEN a malformed schedule does not exactly partition the declared module
- WHEN schedule admission runs
- THEN the core MUST reject it before invoking the runtime.

### Requirement: MVP corpora are exactly reproducible

r[chaoscontrol.spacewasm_mvp.corpus] Every generated or mutated core-module case MUST bind generator or mutator identity, seed, configuration, parent artifact, expected admission/outcome class, exact output BLAKE3, and streaming schedule inputs, and bounded regeneration MUST reproduce the same bytes and class.

#### Scenario: Corpus case is regenerated
- GIVEN a saved case manifest and matching tool/profile cohort
- WHEN bounded regeneration runs
- THEN module bytes, expected class, and schedule inputs MUST match the manifest.

#### Scenario: Regeneration drifts
- GIVEN identical declared inputs produce different bytes, class, or schedule facts
- WHEN corpus verification runs
- THEN the case MUST be classified replay-mismatch and MUST NOT enter the stable differential corpus.

### Requirement: Differential comparison uses normalized observations

r[chaoscontrol.spacewasm_mvp.differential] ChaosControl MUST compare validation/link class, normalized result or trap, canonical Preserves output, ordered hostcall transcript, selected observable final state, final linear-memory BLAKE3, and bounded resource outcome while retaining raw engine-specific counters outside equality decisions.

#### Scenario: Runtimes agree under the profile
- GIVEN both runtimes complete with matching normalized observations
- WHEN comparison runs
- THEN a runtime-match result MUST bind both raw executions and the normalized equality facts.

#### Scenario: Runtimes disagree
- GIVEN any required normalized observation differs
- WHEN comparison runs
- THEN a runtime-divergence result MUST retain both observations without declaring either runtime correct.

### Requirement: Segmented SpaceWasm execution matches uninterrupted execution

r[chaoscontrol.spacewasm_mvp.resume] For deterministic admitted host effects, ChaosControl MUST compare uninterrupted SpaceWasm execution with execution segmented by declared instruction bounds or host pauses and MUST require the same normalized terminal observation.

#### Scenario: Out-of-fuel execution resumes consistently
- GIVEN an admitted module and segment schedule eventually reach a terminal result
- WHEN segmented and uninterrupted runs complete
- THEN their normalized terminal observations MUST match for a resume-match result.

#### Scenario: Resumption changes the observation
- GIVEN segmented execution produces a different result, trap, hostcall transcript, or observable final state
- WHEN resume comparison runs
- THEN ChaosControl MUST emit resume-mismatch evidence and retain the case for replay and shrinking.

### Requirement: Differential failures shrink under stable predicates

r[chaoscontrol.spacewasm_mvp.shrink] ChaosControl MUST define typed predicates for runtime divergence and resume mismatch and MUST accept a shrink step only when the same profile, runtime pair, admitted boundary, and mismatch class remain reproducible.

#### Scenario: Smaller candidate preserves the mismatch
- GIVEN a bounded smaller candidate reproduces the selected predicate
- WHEN shrink admission runs
- THEN the candidate MAY replace its parent and MUST retain an accepted-step receipt.

#### Scenario: Smaller candidate changes boundary class
- GIVEN a candidate becomes unsupported or fails at an unrelated earlier boundary
- WHEN shrink admission runs
- THEN the candidate MUST be rejected for that predicate.

### Requirement: Every exploration dimension is bounded

r[chaoscontrol.spacewasm_mvp.bounds] The profile MUST impose named bounds on artifact size, structural counts, memories/tables, guest linear memory, interpreter allocation, hostcall bytes, instructions, resume segments, chunk count, generation/mutation/shrink work, concurrency, samples, and retained artifacts.

#### Scenario: A declared bound is reached
- GIVEN an operation would exceed a profile bound
- WHEN the boundary is reached
- THEN ChaosControl MUST stop it and emit a typed denial or bound-skip without silently widening the profile.

### Requirement: Differential decisions have a functional core

r[chaoscontrol.spacewasm_mvp.functional_core] Profile/materialization/case admission, schedule validation, observation normalization, comparison, resume predicates, shrink admission, identities, retention, and evidence DTO construction MUST be pure deterministic logic.

#### Scenario: Identical facts are classified twice
- GIVEN identical normalized inputs and observations
- WHEN the core evaluates them
- THEN it MUST return identical identities, decisions, and diagnostics without filesystem, process, network, clock, environment, runtime, KVM, or output effects.

### Requirement: SpaceWasm differential evidence remains separate

r[chaoscontrol.spacewasm_mvp.evidence] ChaosControl MUST keep SpaceWasm runtime-match, runtime-divergence, resume-match, resume-mismatch, replay, shrink, bound, and harness evidence distinct from component strategy evidence, VM snapshot replay proof, assertion readiness, package trust, semantic equivalence, sandbox proof, and release eligibility.

#### Scenario: Bounded runtimes match
- GIVEN a case produces runtime-match and resume-match evidence
- WHEN readiness is classified
- THEN those results MUST remain host-side diagnostic evidence and MUST NOT satisfy VM or production readiness gates.

#### Scenario: Evidence requests a correctness claim
- GIVEN a report infers general WebAssembly or runtime correctness from match results
- WHEN evidence validation runs
- THEN ChaosControl MUST reject the overclaim.

### Requirement: The lane has positive and negative validation

r[chaoscontrol.spacewasm_mvp.validation] The rail MUST include positive exact-bundle, MVP admission, runtime-match, streaming-boundary, expected-trap, and resume-match cases plus negative cohort, unsupported-feature, component, WASI, malformed-stream, invalid-schedule, growth, OOM, host-trap, divergence, resume-mismatch, replay, shrink-class, bound, harness, evidence, and overclaim cases.

#### Scenario: Differential behavior changes
- GIVEN profile, bundle, corpus, runtime, streaming, comparison, resume, shrink, bound, or evidence behavior changes
- WHEN closeout evidence is assembled
- THEN focused deterministic tests, positive and negative fixtures, bounded runtime smoke, evidence contracts, Cairn validation, and lifecycle gates MUST run or record an exact blocker and next-best check.
