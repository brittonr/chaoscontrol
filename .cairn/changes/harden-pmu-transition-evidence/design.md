# Design: Harden PMU Transition Evidence

## Context and Evidence Limits

This is a spec-only change at source revision `06ac3cdb05c22e25600a583c43125e434b5f35b0`. It changes no runtime, matrix, policy or accepted specification. The immediate deliverable is a reviewable plan; the durable result is `pmu-transition-evidence`, maintained by `chaoscontrol-vmm` with the existing KVM evidence rail. Current consumers are the production SMP progress path, its focused KVM harness and PMU support eligibility decisions.

### Reviewed local mechanisms

- [vm.rs](../../../crates/chaoscontrol-vmm/src/vm.rs), `probe_exact_singlestep` and startup selection (approximately lines 832–855 and 1095–1174), enables/disables debug and opens/resets/disables an instruction-overflow event. The probe is closed; the live counter is rebound lazily to the KVM execution thread. This establishes API availability, not transition correctness.
- `configure_progress_source_for_current_turn`, `prepare_progress_source_for_run` and `step` (approximately lines 3591–3830) choose PMU versus exact remainder, reserve evidence before entry, arm/disarm fd-attributed overflow, classify interrupts/guest exits, read progress and invoke the pure planner. `singlestep_active` is shell state, not independent proof that guest stepping survived a privilege return.
- [perf.rs](../../../crates/chaoscontrol-vmm/src/perf.rs) requests `PERF_TYPE_HARDWARE` / `PERF_COUNT_HW_INSTRUCTIONS`, with host exclusion and no guest exclusion. It binds ownership to a Linux TID and attributes a dedicated real-time overflow signal to its fd, with bounded slots, drainage and checked generations. This is not Rewind's raw user-branch event. An API-success boolean is not enough to admit a physical CPU/core-type profile.
- The [pure scheduler](../../../crates/chaoscontrol-sim-core/src/scheduler/core.rs) owns `ProgressMode`, `ExactStepState::Active { remaining }`, guard/boundary arithmetic, progress-source validation, overshoot rejection and canonical identities. The [scheduler facade](../../../crates/chaoscontrol-sim-core/src/scheduler.rs) snapshots that complete deterministic state. Existing scheduler poison and controller poison remain authoritative.
- [snapshot.rs](../../../crates/chaoscontrol-vmm/src/snapshot.rs) serializes `scheduler_snapshot`; VMM restore validates it and reconstructs progress helpers. Host perf fd, TID and signal generations cannot be serialized as replay authority. Correct restoration during correction still needs targeted evidence.
- [deterministic_smp_kvm.rs](../../../crates/chaoscontrol-vmm/tests/deterministic_smp_kvm.rs) covers a real-mode exact-step spin loop under delay/contention, HLT runnable-state behavior and journal preflight. It is not evidence for long-mode SYSCALL/SYSRET/IRET, Linux task switches or PMU correction.
- Accepted [deterministic SMP](../../specs/deterministic-smp-scheduling/spec.md) and [KVM release evidence](../../specs/kvm-release-evidence/spec.md) specify exact boundaries, operational-only watchdogs, poison, complete snapshots and bounded receipts. [matrix.ncl](../../../contracts/kvm-release/matrix.ncl) has `claims_pmu_support = false`. [profile.ncl](../../../contracts/kvm-release/profile.ncl) already allows a required `pmu` kind, disallows duplicate kinds, and requires that kind for a PMU support claim.

### Pinned comparison, not a shared defect claim

Use the immutable [RewindVM tree](https://github.com/fzakaria/rewindvm/tree/6ec1f68de82ae3e657cdfcbe0d83f349bb367082), [preemption catalog](https://github.com/fzakaria/rewindvm/blob/6ec1f68de82ae3e657cdfcbe0d83f349bb367082/docs/preemption.md) and [engine progress code](https://github.com/fzakaria/rewindvm/blob/6ec1f68de82ae3e657cdfcbe0d83f349bb367082/crates/rewind-vmm/src/lib.rs). The catalog reports Ryzen measurements and labels Intel/hybrid behavior inferred rather than verified. It motivates this transition inventory; it does not prove any matching ChaosControl failure. Its numerical branch skid, margins, lateness, benchmarks, cooperative guest hooks and user-only branch semantics are not portable instruction evidence.

The engine is MIT, the guest kernel patch GPL-2.0, and the desktop app proprietary. No app code, guest kernel patch or external runtime is imported. Local Rust guests and local x86_64 Linux/KVM remain the scope.

## Decisions

### 1. Admit one explicit physical host/CPU/perf profile

The shell collects facts without root escalation or MSR writes. A pure admission function compares those facts with an explicitly versioned allowlist/profile. Required facts are host identity; CPU vendor, family, model, stepping, core type/topology and allowed logical CPU set; microcode; host kernel release/build/config identity; KVM implementation/module/build and relevant capability facts; perf access mode/permissions; event type/config/config1/config2 and effective PMU/event mapping; host/guest and user/kernel filters; thread binding; overflow delivery/ownership; and multiplexing/scaling status. Guest CPU presentation is recorded separately from physical host facts.

The initial admitted PMU policy uses verified affinity to a homogeneous explicitly identified CPU set. Unknown core type, unsupported family/model, unverified hybrid migration, affinity drift, missing microcode/build facts, wrong event mapping, multiplexed/scaled counting, unavailable overflow or unavailable exact-step support is not an admitted PMU profile. Opening a generic instruction event or seeing an Intel/AMD name is not admission. Recheck the live execution thread and allowed CPUs before entry and on helper reconstruction; detected post-entry drift becomes uncertain progress under the scheduler's existing poison rule. Do not silently move to a different event, CPU, progress mode or root workaround.

A new family/microcode/kernel profile requires its own matching evidence. Host facts are compatibility/evidence identity; they are not injected into deterministic scheduling choices.

### 2. Count guest retired instructions, including privileged execution

PMU semantics are guest-only retired **INSTRUCTIONS**, not branches, host work, speculative work, user-only execution or wall time. The profile must demonstrate that user and kernel execution count while host activity does not, using fixed fixture paths and exact CONTROL observations. Record generic event identity and effective mapping, not just a friendly name. Zero/wrong-mode counts, unadmitted scaling, ambiguous retirement or counter discontinuities reject the affected evidence/profile.

Every behavior-affecting choice is included in versioned configuration and trace identity: progress mode, instruction quantum, scheduler policy/seed, exact-step margin, effective event/filter semantics, guest CPU/config and artifact identities, fixture inputs and any added correction/guard behavior. Changing a margin or such a parameter changes identity and prevents old exact traces/checkpoints from being accepted as the new run. A host-only timeout or artifact cap is recorded in the receipt but does not become guest scheduling authority. Any bound that actually changes guest stepping must also be identity-bound. No lateness allowance is introduced.

### 3. Normalize a declared boundary observation, not the entire raw trace

For the exact-step CONTROL and PMU candidate, use the same guest artifacts, initial architectural state, seed, scheduler policy and declared instruction quantum, differing only in explicitly declared progress-source parameters. Each retains its own valid raw source-aware schedule trace and identity. Do not demand equal trace hashes, PMU sampling records, internal step counts or progress-source metadata across modes.

A versioned pure normalizer projects both onto **common retired guest-instruction boundaries**: vCPU, per-vCPU retired count, declared quantum, runnable set, selected scheduling action/policy state, and fixture-defined architectural observations at that boundary. Architectural observations include RIP/CPL, general registers, guest-visible flags, selected control/debug state, fixture memory identities, guest trap/signal events and terminal outcome. The same policy and per-vCPU counts must agree at switches and other declared observation points. No interpolation over a missing boundary or resynchronization after divergence is allowed.

Only monitor-private provenance and host-local handles are excluded. Normalization must not hide R11/TF changes, unexpected guest SIGTRAP, skipped instructions, memory differences, missing traps, changed outcomes or wrong instruction counts. Legitimate guest-requested debug behavior has an explicit fixture expectation, rather than blanket masking TF. If CONTROL itself cannot establish a boundary or differs under repetition, the row fails or remains unsupported; it cannot certify PMU by agreement between two broken runs.

### 4. Test the failure catalog as explicit transition obligations

| Case family | Positive observable criterion | Negative observable criterion |
| --- | --- | --- |
| SYSCALL/SYSRET | Long-mode Rust fixture reaches controlled user/kernel boundaries with expected RIP, CPL, R11/flags, memory and instruction counts; correction can start on either side. | Lost stepping, missing boundary, TF leakage into guest-visible return state or an unrequested guest SIGTRAP is a failure, not expected noise. |
| IRET | Controlled interrupt/exception return restores the expected privilege/state and exact next boundary during correction. | Lost-step or leaked flags/traps cannot be normalized away or counted as one instruction merely because a debug exit occurred. |
| Task/context switch | A pinned guest fixture deliberately blocks/yields through a syscall and resumes another task while both user and kernel retired instructions remain accounted for. | A task continuing unstepped, silently skipped execution or misattributed vCPU progress is non-passing. |
| No-exit computation | Separate user and kernel loops allow another runnable vCPU to progress at the declared quantum, without ordinary guest exits selecting the switch. | A lost-step loop ending only at the row watchdog is timed out, not deterministic preemption or guest deadlock. |
| Guard and correction | Cases before, at and after the guard, at the last exact remainder instruction, and at the exact quantum finish with the expected count/action. Include ordinary guest exit/HLT before the target. | Overshoot, premature attributed overflow, late delivery beyond the target, stale/repeated/wrong-fd/wrong-TID signals and counter regression reject; an early halt cannot masquerade as a completed quantum. |
| Interruptions and early ends | Unrelated EINTR/Intr does not authorize schedule progress; validated later guest boundaries account for execution without duplication. Declared terminal fixtures have their own terminal expectation. | Interrupt order, watchdog arrival or partial fixture output cannot manufacture instruction counts, success or a completed evidence row. |
| Snapshot during correction | Serialized restore at declared exact correction boundaries reproduces the next instruction boundary, later switches and terminal observations. | Missing/incompatible remainder/profile/config state or stale helper attribution blocks restore/evidence before a success claim. |

Use controlled minimal long-mode fixtures for exact retirement and ordinary locally packaged Rust/Linux fixtures for blocking task transitions. Bind kernel/initrd/config/input artifacts. Fixture guest scheduler and signal expectations must be explicitly admitted; broader guest-OS determinization is not presumed. If a required transition cannot run without an unsupported OS surface, classify it unsupported, not vacuously passed. Injection tests exercise corrupt observations/negative cases; real KVM positive runs are separately required. Do not claim an injected lost step is an observed production defect.

### 5. Preserve scheduler-owned failure and poison authority

Pre-entry configuration, capability, helper setup or evidence reservation failure rejects before execution and leaves authoritative scheduler state unchanged when the existing owner proves no mutation. Once guest state may have changed, failure to prove exact progress, trap semantics, counter identity, signal attribution, exit handling or commit must follow existing permanent VM/controller poison. Keep partial journals diagnostic-only; no snapshot/restore, later mutation, success output or recording publication through a poisoned controller. This package tests those rules, not invents a recovery exception.

A genuinely unrelated host interruption is not a schedule event. The existing reservation/re-entry protocol must retain enough evidence to account for subsequent guest progress. An interruption that leaves exact progress unknowable is not an innocuous retry. Row deadlines and watchdogs terminate operationally without adding a guest instruction, selecting a vCPU, fabricating a guest verdict or becoming replay evidence. Explorer controller replacement/retry remains outside this package.

### 6. Serialize deterministic progress, reconstruct host helpers

Capture only at established exact boundaries with the existing scheduler/snapshot owner: active/runnable vCPUs and HLT latches, policy/seed state, per-vCPU progress, quantum, mode/margin/event compatibility, correction remaining count and complete architectural/device/guest memory state. Preserve any logical counter offset needed to restart counting, or derive it canonically from admitted schedule state. In-flight fd/TID/generation/reservation state is not portable snapshot authority; unresolved in-flight execution blocks capture.

Restore preflights complete checkpoint/config/guest/profile identities and snapshot lineage, then reconstructs disabled host helpers on the admitted execution thread, drains old signals and arms only the proper source. Correction resumes by exact stepping; it must not restart PMU counting against an already partially consumed quantum. Serialize bytes, discard live helpers, reload into a fresh compatible VM, and compare the continuation with uninterrupted execution. Do not merely compare two references to an in-memory snapshot. Tampered/omitted remaining count, mode/margin/profile mismatch and poisoned-state capture attempts are negative cases. Faulty/deep overlay cohorts block proof under the existing lineage owner; no silent materialization repair is added.

### 7. Use one typed evidence rail and finite named bounds

Future implementation extends the existing Nickel matrix and Rust receipt family, not a second receipt format. Preserve the current non-PMU profile with `claims_pmu_support = false`. Use a separate explicit PMU-claiming profile only after approval and matching fresh required evidence. Because row kinds are currently unique, represent the family-specific transition subcase inventory in one required `kind = "pmu"` umbrella row per selected profile/cohort, with individually identified outcomes and required-subcase completeness; do not add duplicate PMU row kinds without a separately reviewed contract change.

The PMU profile binds the admitted CPU-family/microcode/kernel/perf tuple, source/dirty-state policy, commands/builds, guest artifacts, config, normalizer version, CONTROL and candidate artifacts, full positive/negative inventory and checkpoint references. Required subcases must all finish as expected. A negative injection row passes only when its **expected rejection** is observed; an unsupported or timed-out positive row does not become a passing negative test.

| Configured bound | Meaning and terminal policy |
| --- | --- |
| `PmuTransitionInstructionLimit` | Finite retired-instruction work budget per case; incomplete case is non-passing. |
| `PmuCorrectionStepLimit` | Finite exact correction budget, validated against admitted margin/remainder; exhaustion is failure, never permission to deliver late. |
| `PmuTransitionRepeatCount` | Finite repeated admitted executions, enough to compare independent repetitions; missing repetitions are incomplete. |
| `PmuPerturbationCaseLimit` | Finite recorded delay/contention/unrelated-interrupt perturbation inventory; omitted required cases are incomplete. |
| `PmuCheckpointCountLimit` | Finite serialized checkpoints per case; required correction points cannot be silently dropped to fit. |
| `PmuRowTimeoutSeconds` | Operational deadline; timeout is a non-passing row, not guest failure evidence. |
| `PmuRowArtifactLimit`, `PmuRowArtifactBytesLimit` | Existing typed row artifact count/byte limits; exhaustion/missing artifacts fail closed without publishing a complete receipt. |
| Existing `max_receipt_age_seconds` | Rail freshness bound applied to matching source/profile/artifact identities; stale receipts cannot promote support. |

All values must be positive, finite, schema-bounded, reviewed and recorded before execution; required case/repetition/checkpoint minima must fit admitted maxima. Reuse existing matrix timeout/artifact bounds where appropriate and reject incompatible values; do not choose numerical skid/performance benchmarks from upstream. Bound host delay duration, contention work and interrupt injection count through the recorded perturbation cases. No unbounded signal/case/artifact queue is introduced.

Required missing, unsupported, skipped, absent, timed-out, stale, tampered, dirty-policy-invalid or mismatched rows block **that PMU claim**. No support promotion until every required PMU subcase has a fresh matching receipt. Optional research failure does not block the existing non-PMU base release profile or downgrade exact-step into fallback.

### 8. Keep pure decisions and effects with their owners

`chaoscontrol-sim-core` remains scheduler authority. Reuse its transition/invariant vocabulary; pure PMU profile admission and boundary normalization take supplied facts, never issue KVM/perf calls. Extend the existing evidence core's pure matrix/receipt classification rather than introduce another release gate. The `chaoscontrol-vmm` shell collects CPU/kernel/perf facts, checks affinity, executes KVM, controls debug/perf/signal state and captures architectural observations. Existing local evidence shells own commands, clocks, filesystem hashing, serialized materialization and publication. No generic scheduler/reducer abstraction is introduced.

## Adoption, Dependencies and Non-Claims

1. Review this active plan; source-review tasks alone are complete.
2. Implement profile/identity/normalization and adversarial pure checks using existing owners.
3. Add controlled guest transition fixtures, PMU/CONTROL KVM runs, repetition/perturbation and serialized replay.
4. Obtain fresh family-specific receipts and review profile eligibility on the existing rail. Promotion is a later explicit action, not a consequence of lifecycle gates passing.

No sibling new debugger, witness or page-store-evaluation change is required. Existing retained snapshots/exact replay suffice; artifact materialization continues through whichever current owner supplies its contract, without a storage migration here. [guest-os-determinism-boundary](../guest-os-determinism-boundary/proposal.md) owns broader guest entropy/time/layout/signals; [watchdog isolation](../isolate-branch-watchdog-timeouts/proposal.md) owns explorer recovery; [lineage repair](../repair-incremental-snapshot-lineage/proposal.md) owns overlays. `causality-analysis-engine`, `extract-failure-reducer-core` and `externalize-large-dogfood-artifacts` retain attribution, reusable reduction and remote/storage-neutral materialization respectively.

No runtime fixes are assumed needed until observed locally. If real transitions expose a defect, fix it within scheduler/VMM ownership with failing-before/passing-after evidence before admitting the profile; never suppress an exception or weaken exactness. This plan is neither implementation approval nor KVM execution evidence. It imports no kernel patch or app code, performs no root/MSR mutation, permits no automatic progress fallback or host-time scheduling, and claims no formal proof, all-host equivalence, universal determinism, workload correctness, performance target, proprietary UI parity, containers or hosted services.

## Risks / Trade-offs

- Exact CONTROL itself may fail a privilege transition: this blocks admission and must not be hidden by PMU agreement. Independent fixture markers and architectural/trap expectations prevent circular proof.
- Retired instruction semantics may vary by CPU/KVM/perf combination. Restrictive admission costs coverage but preserves honest support boundaries; a vendor string cannot generalize evidence.
- Pinning limits host flexibility; hybrid parts without a demonstrated homogeneous allowed set remain unsupported. No automatic hardware reconfiguration is offered.
- Minimal fixtures isolate exact boundaries but do not represent all Linux behavior. Linux task fixtures add realism without claiming the entire guest-OS boundary solved.
- Repetition/perturbation gives bounded empirical evidence only. Receipt freshness, profile matching and explicit nonclaims are required even after all rows pass.
