# Harden PMU Transition Evidence

## Why

ChaosControl already declares exact guest-instruction SMP boundaries: `ExactSingleStep` is the default; opt-in `PmuAccelerated { exact_step_margin }` approaches a quantum with a guest-only instruction counter and single-steps the remainder. Opening a perf event and enabling KVM guest debug establish mechanism availability, not correctness across privilege returns, guest task switches, or host CPU profiles. The existing real-mode spin-loop harness is useful but does not establish those transitions. The current [typed KVM matrix](../../../contracts/kvm-release/matrix.ncl) explicitly has `claims_pmu_support = false`; this plan does not change it.

The pinned [RewindVM preemption failure catalog](https://github.com/fzakaria/rewindvm/blob/6ec1f68de82ae3e657cdfcbe0d83f349bb367082/docs/preemption.md) identifies questions worth testing: lost stepping across SYSRET/IRET or blocking syscalls, TF leakage and guest SIGTRAP, skid, wrong event selection, migration, and behavior-affecting margins missing from identity. These are upstream observations or inferences about upstream branch-time scheduling, **not demonstrated ChaosControl defects**. ChaosControl must investigate its own guest-only retired **INSTRUCTION** semantics, including guest kernel execution, rather than importing cooperative scheduling, user-only branch time, vendor margins, or a guest kernel patch.

## Immediate Outcome and Durable Capability

The immediate outcome is an active, spec-only PMU admission and evidence plan with explicit positive and adversarial acceptance criteria. The durable capability `pmu-transition-evidence` will let maintainers decide whether a narrowly identified local host/CPU/perf profile supports the existing accelerated instruction boundary without weakening exact execution.

The concrete current consumers are `chaoscontrol-vmm`'s `DeterministicVm` SMP progress path and `deterministic_smp_kvm` evidence harness, plus the existing typed KVM release matrix/receipt rail. **Maintenance owner:** `chaoscontrol-vmm`; the existing KVM evidence rail owns matrix/receipt integration and support promotion. Adoption is opt-in, per-profile, after bounded transition and serialized-replay evidence; exact-step remains an explicitly selected CONTROL and the current non-PMU release profile remains independent.

## What Changes

- Define profile admission using host identity, CPU vendor/family/model/stepping/core type, microcode, affinity, kernel/KVM identity, perf mode/permissions and complete event identity. Unsupported, unknown, multiplexed or wrong instruction semantics fail closed; no CPU-family marketing claims. r[chaoscontrol.pmu_transition_evidence.admission]
- Bind the instruction quantum, exact-step margin, progress mode, event semantics and all other behavior-affecting parameters into configuration/trace and checkpoint compatibility identities. r[chaoscontrol.pmu_transition_evidence.identity]
- Specify positive evidence across SYSCALL/SYSRET, IRET, guest task/context switches, user/kernel no-exit loops, overflow near the guard with exact remainder, and checkpoint capture during correction. r[chaoscontrol.pmu_transition_evidence.transitions]
- Specify negative evidence for lost step/TF leakage, unexpected guest SIGTRAP, overshoot, early/late/stale/misattributed overflow, counter failure, unsupported migration, and interrupted or early-ending runs; retain scheduler-owned fail-closed and permanent-poison rules. r[chaoscontrol.pmu_transition_evidence.failure]
- Compare PMU with exact-step CONTROL at normalized guest instruction boundaries and architectural observations, not raw source-specific trace metadata. Repeat admitted runs under bounded host perturbation and replay serialized checkpoints. r[chaoscontrol.pmu_transition_evidence.control] r[chaoscontrol.pmu_transition_evidence.checkpoints]
- Integrate bounded PMU evidence through the existing Nickel matrix and Rust receipts; never count missing, unsupported, skipped, stale or timed-out evidence as pass and never promote a PMU claim without fresh matching required PMU evidence. r[chaoscontrol.pmu_transition_evidence.release]
- Keep admission, normalization and verdict rules pure; KVM/perf, signals, CPU-affinity checks, clocks and artifact I/O stay in the VMM/evidence shells. r[chaoscontrol.pmu_transition_evidence.ownership]

## Impact

- **Planned implementation surfaces:** `crates/chaoscontrol-vmm/src/{vm,perf}.rs`, the existing pure scheduler in `chaoscontrol-sim-core`, focused KVM fixtures, and the existing KVM evidence matrix/receipt contracts. No runtime, accepted spec, matrix, policy or generated projection is changed by this package.
- **Bounded verification:** named configurable `PmuTransitionInstructionLimit`, `PmuCorrectionStepLimit`, `PmuTransitionRepeatCount`, `PmuPerturbationCaseLimit`, `PmuCheckpointCountLimit`, `PmuRowTimeoutSeconds`, `PmuRowArtifactLimit`, `PmuRowArtifactBytesLimit`, and the rail's receipt freshness limit must be finite and recorded. Values are reviewed implementation inputs, not invented performance targets. Limit exhaustion is non-passing, never permission to advance late.
- **Positive acceptance:** admitted transition cases reach exact declared instruction boundaries, match CONTROL architectural observations, repeat under perturbation and resume identically from serialized correction checkpoints.
- **Negative acceptance:** unsupported profiles reject before entry; post-entry uncertain progress retains permanent scheduler poison; missing rows, invalid checkpoints, divergence, TF leakage, wrong counters and incomplete execution cannot produce a passing PMU receipt.
- **Evidence now:** source and pinned-document review only, with native proposal/design lifecycle gates. No PMU run, fixture result, measurement, KVM smoke, support promotion, implementation approval, sync or archive is claimed.

## Dependencies and Overlap

This extends evidence for accepted [deterministic SMP scheduling](../../specs/deterministic-smp-scheduling/spec.md) and [KVM release evidence](../../specs/kvm-release-evidence/spec.md); it does not replace their scheduler, poison or release authority. Existing exact replay and the retained snapshot store suffice; there is no hard dependency on the new debugger, witness or snapshot-store-evaluation packages.

[guest-os-determinism-boundary](../guest-os-determinism-boundary/proposal.md) owns guest entropy, clocks, layout and signal determinization. Its unimplemented surfaces cannot be assumed deterministic here: each fixture declares its admitted guest inputs and unsupported surfaces. [isolate-branch-watchdog-timeouts](../isolate-branch-watchdog-timeouts/proposal.md) owns explorer recovery; watchdogs remain operational outcomes with no scheduling or guest-verdict authority. [repair-incremental-snapshot-lineage](../repair-incremental-snapshot-lineage/proposal.md) owns overlay correctness: faulty or unverified deep snapshot cohorts block this proof, not trigger silent repair. General attribution/interleaving minimization, reusable reducers, and external materialization remain with `causality-analysis-engine`, `extract-failure-reducer-core`, and `externalize-large-dogfood-artifacts` respectively.

## Source and Reference Basis

Reviewed repository revision: `06ac3cdb05c22e25600a583c43125e434b5f35b0`.

- [vm.rs](../../../crates/chaoscontrol-vmm/src/vm.rs): `probe_exact_singlestep`, startup capability selection, `configure_progress_source_for_current_turn`, `prepare_progress_source_for_run`, `step`, and post-entry poison handling.
- [perf.rs](../../../crates/chaoscontrol-vmm/src/perf.rs): hardware instruction event, `exclude_host`, thread-owned fd attribution and generation drainage.
- [scheduler facade](../../../crates/chaoscontrol-sim-core/src/scheduler.rs) and [pure core](../../../crates/chaoscontrol-sim-core/src/scheduler/core.rs): mode, exact remainder, snapshot state, progress planning and invariant rejection.
- [deterministic_smp_kvm.rs](../../../crates/chaoscontrol-vmm/tests/deterministic_smp_kvm.rs): exact real-mode spin-loop, delay/contention, HLT and bounded journal coverage, not PMU privilege-transition proof.
- [matrix.ncl](../../../contracts/kvm-release/matrix.ncl) and [profile.ncl](../../../contracts/kvm-release/profile.ncl): existing finite limits, unique row kinds, PMU conditional required row and non-PMU claim boundary.
- Immutable upstream [tree](https://github.com/fzakaria/rewindvm/tree/6ec1f68de82ae3e657cdfcbe0d83f349bb367082), [failure catalog](https://github.com/fzakaria/rewindvm/blob/6ec1f68de82ae3e657cdfcbe0d83f349bb367082/docs/preemption.md), and [engine implementation](https://github.com/fzakaria/rewindvm/blob/6ec1f68de82ae3e657cdfcbe0d83f349bb367082/crates/rewind-vmm/src/lib.rs). Reference only, not a runtime dependency or parity proof. Engine MIT, guest kernel patch GPL-2.0, desktop app proprietary; no app code or kernel patch is imported.

## Non-Goals

No cooperative branch-count scheduling, late quantum delivery, user-only counting, root/MSR mutation, automatic fallback, automatic vendor workaround, kernel patch import, new runtime dependency, containers or hosted service. Rust-only local-machine scope remains unchanged. No formal proof, all-host equivalence, universal determinism, arbitrary guest OS correctness, performance benchmark, or Intel/AMD blanket support claim. This spec-only package leaves `claims_pmu_support = false` and does not unnecessarily block the current non-PMU release profile.
