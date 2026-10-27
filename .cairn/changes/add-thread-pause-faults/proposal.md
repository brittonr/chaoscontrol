# Proposal: Thread-pause faults

## Why

Antithesis exposes thread-level pausing as a distinct fault: individual instrumented threads are delayed so concurrent code interleaves differently across runs. ChaosControl currently pauses a whole VM (`ProcessPause`), a vCPU (`CpuStall`), or a declared guest process through the supervisor. None of those reaches the threads inside one process. A race between two threads in a single process therefore remains outside the deterministic fault schedule.

The existing fault surface already binds clock, resource, process, and device faults to exact stages and snapshots. The missing mechanism is a deterministic way to make one logical guest thread stop making progress while another continues. Host signals, `ptrace`, or wall-clock sleeps would not preserve replay, so the mechanism must be cooperative and tick-bound.

This change adds a bounded thread-pause fault for an admitted cooperative guest runtime cohort. It does not claim arbitrary OS-thread preemption or Antithesis parity.

## What Changes

- Define a stable logical thread identity owned by the guest runtime, independent of recycled OS thread IDs. r[chaoscontrol.thread_pause.identity]
- Add cooperative safe points where an instrumented thread can honor a pause request without an asynchronous host signal. r[chaoscontrol.thread_pause.safepoints]
- Add a typed thread-pause fault with a start condition, finite pause window, target thread, and named bounds, admitted only for the supported runtime cohort. r[chaoscontrol.thread_pause.fault]
- Release a paused thread at the exact virtual tick through the guest runtime scheduler, with deterministic selection of other runnable threads and bounded time advancement when none are runnable. r[chaoscontrol.thread_pause.release]
- Preserve paused-thread state, release ticks, and thread generations across snapshots and exact replay. r[chaoscontrol.thread_pause.state]
- Emit bounded evidence for selected, applicable, applied, observed, unobserved, released, and rejected thread pauses. r[chaoscontrol.thread_pause.evidence]
- Preserve fault, scheduling, and replay meanings; no pause result becomes proof that a race exists or that an interleaving is correct. r[chaoscontrol.thread_pause.nonclaims]
- Add positive and negative fixtures for deterministic interleaving, unsupported runtimes, unknown or reused threads, pause overlap, snapshot restore, and replay drift. r[chaoscontrol.thread_pause.fixtures]

## Impact

- **Core**: thread target admission, pause-window validation, boundary arithmetic, stage classification, and replay binding remain pure deterministic logic.
- **Guest SDK/runtime**: the cooperative scheduler, stable thread identity, safe-point gate, and virtual wait path.
- **VMM and fault shell**: typed fault planning, shared pause state, virtual-tick advancement, snapshots, and evidence.
- **Existing owners**: the fault engine keeps schedule ownership; the VMM keeps execution ownership; the SDK keeps guest runtime ownership.
- **Consumers**: exploration and replay can request thread pauses only for an admitted runtime cohort.

## Lifecycle Prerequisites

None. A bounded cooperative runtime fixture can be introduced with the first implementation. Existing OS-thread workloads remain unsupported for this fault and must receive a typed rejection.

## Out of Scope

- Arbitrary instruction-level preemption of uninstrumented OS threads.
- Host `ptrace`, signals, wall-clock sleeps, or host scheduler interference.
- Changes to `ProcessPause`, `CpuStall`, or process-supervisor semantics.
- A claim that a paused interleaving reproduces a particular bug, proves race absence, or establishes Antithesis parity.
- Non-Rust guests, containers, hosted services, or cross-machine execution.

## Affected Specs

- `thread-pause-faults`: identity, safe points, fault admission, release, state, evidence, non-claims, and fixtures.
