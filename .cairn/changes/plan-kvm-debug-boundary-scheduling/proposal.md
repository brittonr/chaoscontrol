# Plan KVM Debug-Boundary Scheduling

## Why

The concrete consumer is the first-party Rust/Linux SMP campaign and its persisted debugger session on the current KVM backend, where strict retired-instruction scheduling is not supportable across interrupt entry. The retained diagnostic `/tmp/cc-persisted-debugger-0tw7wr4k/pueue-35-irq-state.log` reports injected IRQ 81 at RIP 512 with AX/BX zero, followed by a single debug exit at RIP 513 with AX/BX one: the interval retired three instructions, not one. This supplied observation is a counterexample to instruction equivalence, not a completed coarse-mode campaign. Rearming guest debug before each `KVM_RUN` can address POPF/IRET flag changes but does not establish instruction visibility inside IRQ/NMI/exception entry.

Strict admission must therefore fail closed where exact instruction semantics cannot be established. A separately selected, explicitly coarser contract could make the consumer useful without relabeling these intervals as instructions. The owners are the `chaoscontrol-vmm` execution shell and `chaoscontrol-sim-core` scheduler maintainers, with `chaoscontrol-replay` maintainers owning persisted intake, continuation checks, and debugger presentation; explorer maintainers own the campaign/producer CLI integration.

## What Changes

- Plan an explicit `KvmDebugBoundary` progress mode whose units are accepted boundaries from a versioned KVM debug/exit profile, never retired instructions. Declare interrupt-handler atomic regions as intervals between accepted boundaries, including invisible handler entry/return work. r[chaoscontrol.kvm_debug_boundary.units]
- Preserve strict `ExactSingleStep` and `PmuAccelerated` semantics, admission failure, and artifacts; never auto-fallback from them or claim the new mode satisfies their proof obligations. r[chaoscontrol.kvm_debug_boundary.admission]
- Migrate the exact instruction-bearing configuration/state/event APIs to discriminated instruction-versus-boundary units, reusing seeded selection, runnable-set validation, journal reservation/commit, poison, and canonical verification only where semantics remain sound. r[chaoscontrol.kvm_debug_boundary.scheduler]
- Require a finite host-control recovery watchdog for every guest entry, including nonreturning interrupt handlers; host time has abort authority only. Permanently poison VM/controller state after unverified possible partial execution. r[chaoscontrol.kvm_debug_boundary.recovery]
- Bind mode, unit, boundary-profile/schema identity, boundary observations, pending delivery state, and typed progress in canonical evidence, snapshots, recordings, and persisted sessions. Use explicit CLI opt-in and reject unit/schema/mode confusion without aliases, lossy migration, or fallback. r[chaoscontrol.kvm_debug_boundary.persistence]
- Demand real repeated KVM campaign/replay controls, interrupt and no-exit-handler probes, host-signal perturbation, fresh-process session reload, and corruption rejection before any narrowly bounded determinism claim. r[chaoscontrol.kvm_debug_boundary.evidence]

## Impact

This package is planning only: no product code, accepted specifications, existing strict-SMP tasks, dependency edges, historical receipts, or shared documentation changes. Planned implementation touches sim-core schedule APIs and validation, VMM KVM entry/exit and interrupt/clock handling, snapshot codecs, replay/session identity and intake, and first-party campaign/replay/inspect flags. Future documentation must describe boundary units in operator output rather than call them instructions.

The admission/codec migration is a clean cutover: current strict artifacts remain strict and diagnostic reading may retain their original identity, but old artifacts cannot be converted into coarse execution proof. A requested incompatible continuation rejects before KVM effects. New schema versions and domains must not reinterpret old bytes. The initial coarse profile is experimental and makes no present claim of replay determinism or backend completeness.

## Dependencies and Related Changes

The accepted `deterministic-smp-scheduling` requirements remain unchanged and authoritative for strict instruction scheduling. This new capability is distinct, not an amendment weakening them. Strict admission repair proceeds independently and remains necessary even if this plan is implemented later. No old strict-SMP task is checked or dependency rewritten by this package.

`add-persisted-debugger-replay` and `add-local-debug-query-workspace` supply existing consumers; their own outstanding execution prerequisites remain outstanding. This plan does not make their strict SMP evidence pass. `harden-pmu-transition-evidence` and `evaluate-progress-based-tsc-clock` retain separate ownership; neither PMU support nor a new counter-time model is implied. No new hard dependency edges are introduced.

## Non-Goals and Claim Boundary

No implementation, host-kernel installation or patch, instruction-accurate emulation, PMU acceleration of boundary units, automatic strict downgrade, cross-host/CPU/KVM equivalence, universal IRQ/NMI/exception determinism, guest deadlock inference from a timeout, or invented guest progress. No commit, publication, sync, archive, release promotion, or claim that current KVM already fully implements this contract. Proposal/design gates check document structure only; all implementation, runtime evidence, and archive tasks stay open.
