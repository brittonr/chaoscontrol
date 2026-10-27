## Context

`chaoscontrol-fault` selects and plans faults; `chaoscontrol-vmm` executes them. Existing pause-like faults stop a VM, a vCPU, or a POSIX process. Guest threads are not represented in the fault surface, so two threads in one process cannot be interleaved by a fault.

A deterministic thread pause needs three properties that host facilities do not provide together: a stable target identity, a bounded pause window in virtual time, and a release point that replay can reproduce. Signals and `ptrace` stop a thread at an arbitrary host-observed point and make the resulting interleaving host-timing dependent.

## Decisions

### Decision: Use cooperative safe points, not preemption

**Choice:** an instrumented thread checks the pause gate only at declared safe points. A pause request blocks the logical thread inside the guest runtime scheduler; it never uses an asynchronous host stop.

**Rationale:** a safe point is a deterministic boundary that replay can reproduce. The cost is that only admitted cooperative threads are pausable.

### Decision: Thread identity belongs to the guest runtime

**Choice:** the runtime assigns a stable logical identity from the process role and a monotonic thread generation. OS thread IDs, addresses, and recycled kernel IDs are not identities. Snapshot and replay preserve the mapping.

**Rationale:** OS thread IDs can be reused and are not stable across restore. A logical identity keeps evidence and restored pause state unambiguous.

### Decision: Pause is a bounded window with one active request per thread

**Choice:** a target thread has at most one active pause window. A second request for the same thread is rejected unless a future extend operation is explicitly admitted. The window ends at an exact virtual tick. If no thread is runnable, the runtime requests deterministic virtual-time advancement.

**Rationale:** overlapping pause semantics would create ambiguous release ordering and unbounded evidence. One window per thread keeps the scheduler transition explicit.

### Decision: An unobserved pause remains unobserved

**Choice:** if the target never reaches a safe point before the horizon, the fault records `Applied` but not `Observed`. It does not block the campaign forever and does not claim that the thread was paused.

**Rationale:** safe-point density is workload-owned. An unobserved pause is useful evidence about instrumentation, not a fabricated effect.

### Decision: Unsupported runtimes fail closed

**Choice:** a workload not built with the admitted cooperative runtime is rejected before execution. There is no signal-based, busy-wait, or host-sleep fallback.

**Rationale:** a fallback would silently change determinism semantics and produce unreplayable evidence.

## Risks / Trade-offs

- Cooperative safe points do not cover uninstrumented OS threads; the mechanism must state that limit instead of implying general thread control.
- Safe-point callbacks have runtime overhead; the admitted cohort must bound density and cost.
- A thread blocked in an uninterruptible guest operation cannot reach a safe point; the horizon and `unobserved` classification make that visible.
- Guest-side scheduling must remain deterministic while some threads are paused; the fixture must compare exact continuation and replay, not only the final result.
