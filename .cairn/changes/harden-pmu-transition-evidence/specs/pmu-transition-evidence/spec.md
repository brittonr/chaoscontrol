# PMU Transition Evidence Specification

## Purpose

Define `pmu-transition-evidence`: narrowly admitted, bounded instruction-progress evidence for `chaoscontrol-vmm`'s opt-in PMU-accelerated SMP mode and the existing KVM release evidence rail. Current consumers are `DeterministicVm`, `deterministic_smp_kvm`, and family-specific PMU release decisions. The VMM owns progress integration; the existing KVM rail owns receipts and support eligibility. This delta is a plan, not an implementation or a passing KVM receipt.

Reference: immutable [RewindVM tree](https://github.com/fzakaria/rewindvm/tree/6ec1f68de82ae3e657cdfcbe0d83f349bb367082), [preemption failure catalog](https://github.com/fzakaria/rewindvm/blob/6ec1f68de82ae3e657cdfcbe0d83f349bb367082/docs/preemption.md), and [engine code](https://github.com/fzakaria/rewindvm/blob/6ec1f68de82ae3e657cdfcbe0d83f349bb367082/crates/rewind-vmm/src/lib.rs). These motivate local tests, not a claim that their bugs occur in ChaosControl. Engine MIT, guest kernel patch GPL-2.0, desktop app proprietary; no application code, guest patch or external runtime dependency is imported.

## ADDED Requirements

### Requirement: PMU admission identifies the physical execution profile

r[chaoscontrol.pmu_transition_evidence.admission] Before guest execution in a PMU-admitted profile, ChaosControl MUST validate explicit host identity; physical CPU vendor/family/model/stepping/core type and allowed affinity set; microcode; host kernel and KVM implementation/build/capabilities; perf access mode/permissions; effective PMU and event type/config/config1/config2 identity and filters; exact-step and overflow ownership capabilities; and non-multiplexed, unscaled guest retired-instruction semantics. Guest CPU presentation MUST be distinguished from physical host facts. The admitted event MUST count guest user and kernel retired INSTRUCTIONS while excluding host execution; user-only branch time, speculative work and elapsed time MUST NOT substitute. Unknown/missing facts, wrong/unavailable counters, unsupported CPU/core types, unverified hybrid migration, affinity drift or ambiguous filtering/scaling MUST reject the profile. CPU-vendor marketing labels or successful event opening alone MUST NOT establish support. Live execution-thread/affinity admission MUST be rechecked at entry and helper reconstruction; detected uncertainty after possible progress MUST use existing poison rules, not automatic fallback or host reconfiguration.

#### Scenario: Matching instruction profile executes

- GIVEN a fully identified homogeneous allowed CPU set and a matching admitted kernel/KVM/perf event profile
- WHEN fixed user and kernel fixture paths execute with separate host work under exact-step CONTROL and PMU
- THEN the evidence MUST demonstrate guest user and kernel retired instruction accounting and host exclusion, retain the effective event identity and admit only that matching profile.

#### Scenario: Counter or CPU profile is unsupported

- GIVEN an unavailable/wrong counter, scaled/multiplexed event, missing microcode identity, unsupported CPU family/core type, or unverified hybrid/affinity migration
- WHEN admission runs
- THEN it MUST emit a typed non-admission before guest entry, MUST NOT silently select another event/progress mode, and MUST NOT emit a passing PMU run artifact.

#### Scenario: Execution thread leaves its admitted CPU set

- GIVEN a profile was admitted but execution-thread affinity or observed CPU/core type no longer satisfies it
- WHEN the shell rechecks before entry or detects drift after possible guest progress
- THEN it MUST respectively reject before entry or permanently poison through the existing scheduler owner, and MUST NOT retain a PMU support pass.

### Requirement: Behavior-affecting progress parameters have exact identities

r[chaoscontrol.pmu_transition_evidence.identity] Versioned configuration, trace and checkpoint compatibility identities MUST bind the declared instruction progress mode, quantum, scheduler policy/seed, exact-step margin, effective event/filter semantics, guest CPU/config and artifact identities, and every added parameter capable of changing guest behavior. Host profile evidence MUST bind its observed physical facts without granting them schedule authority. Operational limits MUST be recorded in receipts; any limit that changes stepping MUST also enter the deterministic identity. Exact replay MUST reject incompatible or missing identities rather than treating changed margins or event semantics as the same execution.

#### Scenario: Margin changes

- GIVEN two otherwise identical configurations differ in exact-step margin or event semantics
- WHEN configuration/trace identities and checkpoint compatibility are evaluated
- THEN their behavior-affecting identities MUST differ and exact replay MUST NOT accept an artifact bound to the other configuration.

#### Scenario: Old artifact omits a required binding

- GIVEN a trace/checkpoint omits a required mode, margin, guest artifact or event compatibility binding
- WHEN PMU evidence or exact restore admission evaluates it
- THEN it MUST remain unsupported or fail compatibility validation before execution and MUST NOT gain an invented default identity.

### Requirement: Transition fixtures establish exact retirement through privilege and task changes

r[chaoscontrol.pmu_transition_evidence.transitions] Required PMU evidence MUST include independently expected instruction/architectural observations through SYSCALL/SYSRET, IRET, blocking syscall and guest task/context switches, no-exit user and kernel loops, guard-adjacent overflow and exact-step remainder boundaries, and ordinary guest exit/HLT before the target. Each fixture MUST bind its guest/kernel/initrd/config/input artifacts, declared retired-instruction boundaries, privilege/state/trap expectations and admitted OS surfaces. Correction MUST be exercised on both sides of privilege returns and after task changes. Lost stepping, skipped instructions, TF leakage including guest-visible R11/flags, unexpected guest SIGTRAP or other unexpected architectural differences MUST NOT count as a passing transition. Unsupported fixture surfaces MUST be explicit non-passing evidence rather than presumed guest-OS determinism.

#### Scenario: Privilege returns and task switches preserve progress

- GIVEN controlled long-mode fixtures enter via SYSCALL, return via SYSRET and IRET, and a Rust/Linux fixture blocks and resumes a different task
- WHEN PMU correction crosses each declared transition and CONTROL executes the matching path
- THEN the expected RIP/CPL, instruction counts, R11/guest-visible flags, memory and trap observations MUST match at common boundaries, with no unrequested guest SIGTRAP or unaccounted execution.

#### Scenario: User or kernel computation has no ordinary exits

- GIVEN one runnable vCPU executes a user or kernel no-exit loop and another runnable vCPU can progress
- WHEN the declared instruction quantum is reached
- THEN the next vCPU/action MUST be selected by the existing deterministic scheduler at the exact boundary, without timer arrival or ordinary guest exits selecting it.

#### Scenario: Overflow correction finishes without lateness

- GIVEN cases land before/at/after the guarded count without exceeding the target, including a last remainder instruction and a boundary exactly at the quantum
- WHEN the accelerated path corrects to the target
- THEN the expected retired count and scheduling action MUST occur exactly, without accepting a late quantum or importing upstream branch-count margins.

#### Scenario: Stepping is lost or leaks into the guest

- GIVEN an injected lost-step observation or a real fixture run with unexpected TF/R11 state, missing instruction boundary or guest SIGTRAP
- WHEN transition evidence is classified
- THEN the transition MUST be non-passing, the architectural discrepancy MUST remain visible and post-entry uncertainty MUST follow existing poison authority.

### Requirement: CONTROL comparison normalizes guest boundaries without hiding defects

r[chaoscontrol.pmu_transition_evidence.control] A versioned pure normalizer MUST compare PMU and explicitly selected `ExactSingleStep` CONTROL using common guest retired-instruction boundaries, vCPU/per-vCPU counts, quantum, runnable set, selected action/policy state, and declared architectural/terminal observations. Each mode MUST retain and validate its own source-aware raw trace and configuration identity. Cross-mode comparison MUST NOT require identical progress-source metadata, internal sampling/step records or raw trace hashes. It MUST NOT mask guest-visible flags/TF, R11, RIP/CPL, register/memory/trap/outcome differences, interpolate missing boundaries or resynchronize after divergence. CONTROL MUST itself satisfy fixture expectations and admitted repetitions before it can serve as evidence.

#### Scenario: Different source metadata has the same guest semantics

- GIVEN valid PMU and CONTROL raw traces with distinct source/configuration identities and distinct internal record shapes
- WHEN normalization evaluates matching declared guest boundaries
- THEN matching counts, selected actions and architectural/terminal observations MUST support bounded cross-mode equivalence without demanding raw trace identity.

#### Scenario: Normalization would hide a discrepancy

- GIVEN one mode skips a boundary, changes guest-visible TF/R11 or memory, produces an unexpected trap, or terminates early
- WHEN normalized observations are compared
- THEN the comparison MUST fail with the first unmatched declared boundary/observation and MUST NOT discard the difference as source metadata.

#### Scenario: CONTROL is not independently sound

- GIVEN CONTROL fails fixture expectations or repeated admitted execution
- WHEN a PMU run happens to match that CONTROL
- THEN PMU evidence MUST remain non-passing or unsupported rather than treating agreement as correctness.

### Requirement: Invalid evidence preserves scheduler-owned fail-closed and poison behavior

r[chaoscontrol.pmu_transition_evidence.failure] Negative verification MUST cover overshoot; attributed overflow before the guard; late delivery beyond the target; stale/repeated/wrapped/wrong-fd/wrong-thread or wrong-vCPU attribution; lost/short/regressing/discontinuous counter reads; unavailable/wrong events; interrupted execution and early-ending fixtures. Pre-entry failures proven mutation-free MUST reject without inventing progress. Once guest state may have changed, uncertain/missing instruction evidence and exit/commit failures MUST follow accepted scheduler VM/controller poison rules; partial traces remain diagnostic-only and MUST NOT authorize execution, snapshots/restores, mutation, complete recordings or success. Unrelated EINTR/Intr MUST NOT select schedule transitions; any subsequent progress MUST be exactly accounted through the existing reservation protocol. Watchdog/deadline outcomes MUST remain operational-only, never guest verdicts, late-boundary acceptance or replay proof. This capability MUST NOT weaken or duplicate scheduler recovery authority.

#### Scenario: Signal or count cannot prove the boundary

- GIVEN PMU execution reports an overshoot, premature/late overflow, stale/repeated/wrapped or misattributed signal, or invalid/short/regressing counter observation
- WHEN post-entry progress planning or shell validation runs
- THEN the evidence MUST reject the claimed boundary, retain the diagnostic and invoke permanent poison wherever progress may have occurred without exact evidence.

#### Scenario: Unrelated interruption is not a quantum

- GIVEN unrelated EINTR or `VcpuExit::Intr` interrupts a run without an attributed accepted PMU boundary
- WHEN the existing execution/reservation owner handles it
- THEN it MUST NOT add deterministic progress or choose another vCPU because of that interruption, and later accepted progress MUST neither lose nor double-count already executed instructions.

#### Scenario: Run stops before required evidence completes

- GIVEN a case halts before its expected target, is interrupted without provable completion, or exceeds its host deadline
- WHEN the row verdict is computed
- THEN it MUST be incomplete, failed or operationally timed out rather than passed, except where the fixture explicitly requires and validates that exact early terminal outcome; no fabricated quantum or guest deadlock verdict is permitted.

#### Scenario: Caller attempts to reuse a poisoned run

- GIVEN a post-entry evidence or exit-handling failure has poisoned the VM/controller
- WHEN a caller requests further execution, mutation, snapshot/restore, trace publication, recording or success
- THEN the existing owner MUST reject before further mutation, and partial journals MUST remain diagnostic-only.

### Requirement: Repetition and serialized checkpoints preserve progress evidence

r[chaoscontrol.pmu_transition_evidence.checkpoints] Required evidence MUST repeat admitted executions under recorded bounded host delay, CPU contention and unrelated interrupt perturbations, and compare accepted same-mode normalized observations and canonical traces according to that mode's contract. It MUST also capture established exact boundaries during PMU correction, serialize complete checkpoints, discard live helpers, restore bytes into a fresh compatible VM and compare the next boundary and subsequent continuation with uninterrupted execution. Snapshots MUST bind required active/runnable/policy/seed/per-vCPU progress, quantum, mode/margin/event compatibility, exact remainder, logical counter offset if needed, HLT latches and complete architectural/device/memory state. Unresolved in-flight execution MUST block capture. Host fd/TID/signal generations MUST be reconstructed and stale signals drained, never trusted as serialized replay authority. Missing/tampered/incompatible progress state and faulty or unverified deep snapshot lineages MUST block proof under their existing owners rather than be silently repaired here.

#### Scenario: Admitted host perturbations preserve accepted execution

- GIVEN the same admitted guest/configuration/seed/profile runs repeatedly with the recorded delay, contention and unrelated interrupt cases
- WHEN executions complete within their bounds
- THEN same-mode canonical schedule evidence and bounded guest observations MUST match, cross-mode evidence MUST satisfy the normalized CONTROL comparison, and operational timeouts MUST be reported separately as non-passing rows.

#### Scenario: Serialized correction checkpoint resumes

- GIVEN an exact correction boundary with a nonzero remainder and complete admitted checkpoint bindings
- WHEN its serialized bytes are restored into a fresh compatible VM after live helpers are discarded
- THEN reconstruction MUST use the admitted execution thread/source without stale signals, and the next boundary, later switch actions and terminal observations MUST match uninterrupted execution without restarting or skipping the consumed remainder.

#### Scenario: Progress state or snapshot lineage is invalid

- GIVEN a checkpoint lacks a remainder/binding, has a changed mode/margin/profile, carries unresolved in-flight execution, or depends on a faulty/unverified deep overlay
- WHEN capture, restore or evidence admission evaluates it
- THEN the operation or proof MUST be blocked before a successful continuation claim, with no silent repair or invented state.

### Requirement: Every evidence path has finite named bounds

r[chaoscontrol.pmu_transition_evidence.bounds] PMU case execution MUST admit and record positive finite schema-bounded `PmuTransitionInstructionLimit`, `PmuCorrectionStepLimit`, `PmuTransitionRepeatCount`, `PmuPerturbationCaseLimit`, `PmuCheckpointCountLimit`, `PmuRowTimeoutSeconds`, `PmuRowArtifactLimit`, `PmuRowArtifactBytesLimit`, and the existing rail's `max_receipt_age_seconds`. Perturbation cases MUST additionally bound delay duration, contention work and interrupt count. Required case/repetition/checkpoint minima MUST fit the admitted maxima. Values MUST be reviewed configuration, not invented vendor benchmarks. Limit exhaustion, missing required subcases/artifacts or truncated evidence MUST be non-passing and MUST NOT authorize late delivery, silent omission or unbounded retry. Limits affecting guest execution MUST also be identity-bound.

#### Scenario: Bounded complete inventory executes

- GIVEN reviewed finite bounds admit the complete required fixture/repetition/checkpoint inventory and correction remainder
- WHEN the evidence shell runs each case
- THEN the receipt MUST bind those limits and expected terminal classifications and retain all required observations within the existing artifact caps.

#### Scenario: Bound is missing or exhausted

- GIVEN a missing/nonfinite/out-of-schema bound, required inventory larger than its configured maximum, or an exhausted instruction/correction/time/artifact budget
- WHEN admission or execution classifies the case
- THEN it MUST reject or report the bounded non-passing outcome and MUST NOT declare the omitted work completed.

### Requirement: Existing typed KVM receipts exclusively gate PMU support promotion

r[chaoscontrol.pmu_transition_evidence.release] PMU evidence MUST extend the existing typed Nickel KVM matrix and Rust-owned runtime receipt rail rather than create a parallel release format. A future PMU-claiming profile MUST require a matching `pmu` row with individually identified mandatory transition, negative, CONTROL, perturbation and serialized-checkpoint subcase outcomes. The existing unique-row-kind contract MUST be respected, for example by one complete umbrella PMU row per selected family-specific profile/cohort. Receipts MUST bind source/dirty-state policy, worker/CPU-family/microcode/kernel/KVM/perf profile, commands/builds, guest/config identities, limits, normalizer version and retained CONTROL/candidate/checkpoint artifacts. Missing, absent, skipped, unsupported, timed-out, stale, tampered, dirty-policy-invalid or mismatched required rows/subcases MUST NOT pass. A negative test passes only when its expected rejection is observed. No PMU support claim MAY be promoted without fresh matching required receipts. This spec-only change MUST leave `claims_pmu_support = false` unchanged and MUST NOT make optional PMU research evidence a prerequisite of the current non-PMU release profile.

#### Scenario: Complete family-specific PMU evidence supports a later bounded claim

- GIVEN every mandatory subcase has the expected outcome and all artifacts/identities/freshness checks pass on the admitted family-specific profile
- WHEN the existing KVM rail evaluates that explicitly selected PMU-claiming profile
- THEN it MAY support only the recorded bounded profile claim, not Intel/AMD-wide or all-host support, and promotion MUST remain an explicit later reviewed action.

#### Scenario: Required evidence is missing or cannot execute

- GIVEN a mandatory PMU row/subcase is missing, unsupported, skipped, timed out, stale, tampered, dirty-policy-invalid or mismatched
- WHEN claim eligibility runs
- THEN that PMU claim MUST remain blocked with the specific gap, even if every portable check or unrelated KVM row passes.

#### Scenario: Current non-PMU release is evaluated

- GIVEN the current matrix retains `claims_pmu_support = false` and satisfies its own required base rows
- WHEN optional PMU research has no fresh matching receipt or reports unsupported
- THEN it MUST NOT add a PMU claim or unnecessarily block the non-PMU profile solely for that PMU gap.

### Requirement: Decisions remain pure and claims remain local and bounded

r[chaoscontrol.pmu_transition_evidence.ownership] `chaoscontrol-sim-core` MUST retain deterministic scheduler and invariant authority. Profile admission, identity compatibility, boundary normalization and matrix/receipt verdicts MUST be pure functions over supplied facts, reusing existing owners and conventions. `chaoscontrol-vmm` and existing evidence shells MUST own CPU/affinity/kernel/perf queries, KVM/debug/signals, clocks, filesystem/materialization and publication. This capability MUST remain Rust-only and local-machine; MUST NOT introduce automatic fallback, late scheduling, root/MSR mutation, guest kernel patch/app code import or external runtime dependency; and MUST NOT claim a demonstrated local defect from upstream documentation, formal proof, universal determinism, all-host equivalence, complete guest-OS determinism or workload correctness. Guest-OS determinization, watchdog explorer recovery, overlay correctness, general attribution/reducers and external materialization MUST remain with their existing changes; no sibling debugger/witness/page-store change is a hard prerequisite.

#### Scenario: Identical supplied facts produce identical decisions

- GIVEN identical admitted profile/configuration, raw observations, normalizer version, limits and receipt facts
- WHEN pure admission/normalization/verdict evaluation runs twice
- THEN the decisions MUST match without environment, clock, file, process, signal, perf or KVM access.

#### Scenario: Report exceeds observed evidence

- GIVEN only source review, lifecycle gates or one admitted family-specific cohort exists
- WHEN a report claims implemented PMU support, an actual ChaosControl TF defect, formal/all-host proof, root-workaround necessity or broader guest-OS determinism without matching evidence
- THEN claim validation MUST reject that interpretation and preserve the bounded evidence class and existing ownership boundaries.
