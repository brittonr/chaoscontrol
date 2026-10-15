# Counterfactual Replay Witnesses Specification

## Purpose

Provide a Rust-only, local-machine operator with a retained failing/non-failing replay pair that shares an exact verified prefix and differs by one declared intervention. The immediate outcome is an actionable CLI/report witness for an admitted v2 bug; the durable capability is bounded independently checkable comparison evidence. Replay/evidence maintainers own the evidence contract and operator wording; explorer maintainers own integration with the existing minimizer. Broader causality analysis and reusable reduction are separate consumers/owners, not prerequisites. This delta plans implementation; it does not assert that a qualifying pair has been produced.

The design reference is [RewindVM revision 6ec1f68de82ae3e657cdfcbe0d83f349bb367082](https://github.com/fzakaria/rewindvm/tree/6ec1f68de82ae3e657cdfcbe0d83f349bb367082), particularly [`crates/rewind-core/src/check.rs`](https://github.com/fzakaria/rewindvm/blob/6ec1f68de82ae3e657cdfcbe0d83f349bb367082/crates/rewind-core/src/check.rs): its neighboring one-step-shorter run and explicit edge-minimality/non-monotonicity limitations motivate retained adjacent evidence, not algorithm transplantation or parity. Engine MIT, guest kernel patch GPL-2.0, desktop app proprietary; no app code or external runtime dependency is introduced. Exact ChaosControl source observations and file/line links are recorded in this change's design.

## ADDED Requirements

### Requirement: A complete pair binds every input and one intervention

r[chaoscontrol.replay_witness.pair_binding] A complete witness MUST bind the same exact guest/kernel/initrd/disk artifacts, deterministic configuration, master seed, VM/vCPU topology, progress mode/capability profile, admitted complete assertion catalog, retained common parent snapshot and selected exact v2 assertion identity for both runs. The target MUST include its descriptor, canonical bytes, fingerprint and catalog token; aliases, locations/messages and bug dedup keys MUST NOT replace exact identity. The manifest MUST bind source/build identity, snapshot depth/tick/codec/schema/digest, intervention and boundary identities, predeclared comparison horizon, configured bounds, each run's schedule/run identity and digest-bound trace/recording/verdict refs. The supported intervention MUST omit exactly one identified canonical schedule entry, preserving all other entries' timing/parameters/labels/order and every other execution input. Run/schedule/attempt identity changes mechanically derived from that omission MUST be explicit and validated, not treated as unexplained input changes. The parent MUST precede the intervention, and invalid inputs MUST reject before VM mutation.

#### Scenario: One occurrence is omitted from an admitted schedule

- GIVEN a retained parent, admitted exact target and a confirmed failing schedule with two identical-looking same-time entries
- WHEN one original canonical entry index is omitted and all other inputs remain equal
- THEN pairing MUST bind that occurrence, before/after schedule identities and branch index correspondence without conflating the two entries.

#### Scenario: A second input differs

- GIVEN two runs that have differing target/catalog, seed, topology, guest/configuration identity, parent snapshot or a second schedule edit
- WHEN pairing admission runs
- THEN it MUST reject the pair with the mismatched field and MUST NOT execute or label the result complete.

#### Scenario: A parent is unavailable or a deep lineage is unverified

- GIVEN a missing/digest-invalid parent or a cohort whose incremental lineage fidelity cannot be established
- WHEN witness admission attempts to load the parent
- THEN evidence MUST remain blocked and MUST NOT replace the parent, bootstrap silently, or repair overlays inside this capability.

### Requirement: Both outcomes require positive branch-local observation

r[chaoscontrol.replay_witness.outcomes] Before executing candidates, the operator MUST declare `comparison_ticks_after_parent`; both sides of a complete witness MUST positively complete that horizon using each branch's own completed ticks and valid trace evidence. Equal tick numbers after divergence MUST NOT imply equal guest work. The failing side MUST positively evaluate and violate the selected target within the branch interval. The non-failing side MUST positively evaluate that same target in the branch interval and finish without its violation under existing kind-specific oracle semantics. Parent or prior-run aggregate hits/verdicts MUST NOT count as new branch evaluation. `!Failed`, an absent bug file, `Unexercised`, unrelated assertion outcomes, or an early stop MUST NOT establish non-failure. Targets lacking a positive evaluation channel, including a non-hit-only unreachable assertion under the current record model, MUST be reported unsupported rather than changing assertion meaning. Timeout/watchdog, poison, missing/truncated traces, early termination, unsupported control/target observation and runtime exhaustion MUST produce indeterminate outcomes, never non-failing proof.

#### Scenario: Both branches finish and the selected target differs

- GIVEN identical admitted inputs and horizon, with branch-local observations for the exact target
- WHEN both runs complete the horizon and the target fails only in the intervention-present run while it is evaluated and satisfies its existing semantics in the omission run
- THEN the outcomes MUST qualify as failing and horizon-complete non-failing, subject to independent trace and pair verification.

#### Scenario: A restored parent supplies all target hits

- GIVEN a parent containing a passing target record and a new branch that never evaluates that target
- WHEN the branch reaches the horizon without a new failure
- THEN it MUST NOT qualify as a non-failing side from the inherited passing verdict.

#### Scenario: A failure record is absent for an operational reason

- GIVEN a candidate that times out, stops early, exhausts trace capacity, lacks a trace or cannot apply its supported control
- WHEN its target has no recorded failure
- THEN it MUST be indeterminate with a named reason and MUST NOT be paired as non-failing.

### Requirement: Independent trace verification precedes cross-run comparison

r[chaoscontrol.replay_witness.trace_comparison] Each side MUST retain and validate its own complete bounded canonical round, exact SMP transition and fault-stage evidence, native ordering, identities, checkpoint/ledger associations and horizon before cross-run comparison. Each retained side MUST be independently replayed from the admitted common parent and checked against its own recording, including its divergent suffix; it MUST NOT be compared to a single original failing suffix. The cross-run comparator MUST verify the entire pre-intervention execution prefix and boundary state using explicit validated correspondence for mechanically changed schedule/run/attempt identifiers without discarding execution-relevant state, event order, timing, progress or outcomes. The versioned canonical comparison ordering MUST be recorded. It MUST report the earliest unequal retained canonical entry after the common prefix, with the last equal entry and branch-local first unmatched entries/values or bounded refs, kinds/fields, local coordinates and artifact refs. Insertion/end-of-stream MUST be explicit. After divergence, tick-based suffix realignment MUST NOT be inferred. A parent-boundary empty prefix MUST be labelled empty and bind equal restored state rather than claim earlier verified execution. Missing comparability or divergence evidence MUST remain incomplete.

#### Scenario: The changed suffix replays exactly on its own

- GIVEN two internally valid branch recordings with identical verified pre-intervention execution and different suffixes
- WHEN each is replayed against itself and cross-run comparison reaches the first changed entry
- THEN the witness MUST preserve both successful self-verifications and branch-local divergence coordinates without rejecting the omission suffix merely because it differs from the original run.

#### Scenario: An internal trace is forged or reordered

- GIVEN two apparent prefixes whose one branch has a forged transition, invalid ledger, reordered entry or unbound checkpoint
- WHEN native trace validation runs
- THEN comparison MUST reject that branch before reporting a verified prefix or first divergence.

#### Scenario: Unexplained execution diverges before intervention

- GIVEN different derived schedule identities but a declared later intervention boundary
- WHEN validation finds an execution-relevant difference before that boundary
- THEN pairing MUST reject the candidate and MUST NOT hide the difference through identifier normalization.

#### Scenario: A divergence is not a source location

- GIVEN a first unmatched canonical event with per-branch tick, VM and transition index
- WHEN the report renders its location
- THEN it MUST link those exact retained coordinates and MUST NOT infer a guest source line, corresponding suffix work or first differing machine instruction.

### Requirement: Candidate search and retention have deterministic finite bounds

r[chaoscontrol.replay_witness.bounded_search] Witness search MUST use existing fault-schedule subset/minimizer facilities, independently confirm its starting failing schedule, and enumerate one-entry omissions by ascending original canonical index, preserving same-time occurrence order. It MUST return the first qualifying pair, not a ranked or globally minimal result, and MUST NOT assume monotonic outcomes. An omitted sole entry MAY yield an empty candidate; an empty starting schedule MUST report no candidates. Configuration MUST supply finite positive `max_candidate_schedules`, `max_run_evaluations`, `comparison_ticks_after_parent`, `max_trace_records_per_run`, `max_trace_bytes_per_run`, `max_artifact_bytes_total`, `max_snapshot_bytes`, `max_manifest_bytes` and `run_watchdog_timeout_ms`, admitted within existing hard caps and checked arithmetic. Every baseline/candidate/independent-verification evaluation MUST count; the admitted budget MUST permit at least one complete baseline/candidate/verification sequence or reject before execution. Exceeding a bound MUST stop partial with the named bound and attempted/completed counts, not truncate proof or invent a pair. An indeterminate candidate MAY be followed only by the next declared candidate within the remaining admitted bounds, without hidden retries. Exhaustive determinate search without a pair MUST report `no_pair_found`; any unresolved candidate MUST keep that conclusion incomplete.

#### Scenario: Order is reproducible and a neighbor qualifies

- GIVEN a fixed admitted schedule and bounds sufficient for the first qualifying omission and its independent verifications
- WHEN search evaluates candidates
- THEN it MUST use original-index order, count all runs and publish that first qualifying pair with the recorded search prefix.

#### Scenario: Outcomes are non-monotonic

- GIVEN sampled omissions whose target outcomes alternate between failing and non-failing
- WHEN search records a qualifying pair
- THEN it MUST NOT infer that more omitted faults preserve non-failure or that the resulting schedule is globally minimal or uniquely causal.

#### Scenario: Retention or evaluation capacity runs out

- GIVEN admitted finite bounds and no verified pair yet
- WHEN a candidate would exceed evaluation, snapshot, trace, manifest or total artifact capacity
- THEN search MUST stop with a partial result naming the exhausted limit and MUST NOT publish a complete witness with missing evidence.

### Requirement: Local artifacts and CLI links are independently actionable

r[chaoscontrol.replay_witness.operator_surface] An opt-in local witness CLI MUST consume an admitted v2 bug and explicit bounded configuration without silently changing existing minimize/reproduce behavior. It MUST persist a versioned `chaoscontrol.counterfactual-replay-witness.v1` manifest with explicit retained parent/checkpoint refs and branch-local verdict artifacts, rather than relying on serde-skipped recording fields. Local refs MUST remain contained within the artifact root and be digest-checked on reopen. Required artifacts MUST be verified before atomic complete-manifest publication; I/O failure MUST NOT leave an apparently complete pair. CLI/show/report output MUST name the target descriptor, evidence status/non-claims, intervention, common-prefix endpoint, first canonical divergence, configured limits and stop reason, and provide resolvable links to both recordings, both traces/verdicts and common parent plus runnable local show/verify/rerun instructions. Missing or tampered artifacts MUST produce named failure diagnostics rather than success links. No GUI or external runtime service is required.

#### Scenario: A retained witness is reopened by an operator

- GIVEN a complete local pair and the exact manifest/artifact tree
- WHEN the operator follows the printed show/verify commands and per-run divergence links
- THEN every referenced digest MUST validate, each side MUST resolve to its own retained evidence, and the report MUST expose a reproducible rerun recipe with exact inputs.

#### Scenario: Publication or reopen encounters missing bytes

- GIVEN an interrupted artifact write or a manifest whose trace/parent/verdict bytes were deleted or changed
- WHEN publication or verification runs
- THEN complete publication MUST fail or reopen MUST reject with the exact failing ref, never present a verified pair from filenames alone.

### Requirement: Pure evidence decisions and effect ownership stay separate

r[chaoscontrol.replay_witness.ownership] Existing replay-evidence-core MUST own pure witness admission, canonical identity, candidate ordering/accounting, branch-local classification, prefix/divergence comparison and publication eligibility over supplied facts. Existing simulation/scheduler/fault validators MUST remain in their current pure owners. Replay and explorer shells MUST own KVM restore/execution and trace capture; filesystem/clock/process/artifact publication and rendering MUST remain shell effects, with evidence maintainers owning report validation. The capability MUST reuse existing retained snapshot contracts without adding a store, codec/digest migration or external dependency. General attribution/interleaving minimization, generic reducer extraction, remote artifact materialization and incremental overlay repair MUST remain with their existing owners. Sibling debugger persistence, page-store evaluation and PMU evidence changes MUST NOT become hard dependencies for this bounded witness contract.

#### Scenario: Identical admitted facts are classified offline

- GIVEN immutable DTOs with validated input identities, outcomes and trace facts
- WHEN pure admission/comparison is evaluated repeatedly without KVM or filesystem access
- THEN its identity, ordering, classification and divergence result MUST be identical.

#### Scenario: A shell effect fails

- GIVEN a pure plan eligible to attempt a candidate
- WHEN snapshot load, KVM execution, trace retention or publication fails
- THEN the shell MUST supply an explicit error observation and MUST NOT substitute invented successful facts or perform a weaker-determinism fallback.

### Requirement: Witness evidence does not promote unrelated claims

r[chaoscontrol.replay_witness.evidence_boundary] Each branch MUST retain its existing replay verdict artifact/class unchanged. A complete witness MUST additionally satisfy this specification's paired predicates; `snapshot_backed_not_reproduced` alone MUST NOT be treated as horizon-complete non-failure, and only independently eligible `snapshot_backed_reproduced` evidence MAY support the failing snapshot-backed replay claim. Legacy ID-only carriers, schedule-only gaps, local dry-runs, raw logs and lifecycle PASS results MUST remain insufficient. Witnesses MUST describe bounded evidence for the recorded target/inputs/horizon/intervention, not unique cause, ranked attribution, a globally minimal schedule, whole-program correctness, inferred source location or product parity. Verification MUST include positive identity-bound retained-pair/reopen/link/self-replay coverage and negative identity, prefix, observation, unsupported-control, operational and budget cases; a local controlled Rust KVM smoke MUST substantiate runtime acceptance. Spec-only work MUST leave implementation, fixtures, KVM measurements and release/sync/archive closeout unchecked until their evidence exists.

#### Scenario: An accepted failing replay participates in a complete witness

- GIVEN independently accepted failing snapshot-backed replay evidence and a qualifying non-failing branch under this contract
- WHEN the pair is published
- THEN the report MUST preserve the failing replay class and state the additional bounded comparison claim without generalizing beyond recorded inputs and horizon.

#### Scenario: A negative replay class or structural gate is offered as proof

- GIVEN only `snapshot_backed_not_reproduced`, a schedule-only gap, historical alias-only artifacts, raw logs or a passed Cairn gate
- WHEN a consumer attempts witness promotion
- THEN it MUST reject the complete-pair claim and name the missing independent trace/outcome/input evidence without rewriting accepted replay meaning.
