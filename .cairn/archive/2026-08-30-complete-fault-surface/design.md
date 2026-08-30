# Design: Complete Fault Surface

## Context

Per-VM slots already carry `clock_freeze`, `clock_jitter_bound`, `vcpu_stall_until`, and `memory_limit_bytes` fields. The fault planner emits typed rejections for these capabilities. The six-stage ledger tracks selection, application, and observation.

## Decisions

### 1. Clock freeze suspends virtual time

Clock freeze holds the admitted virtual-clock boundary for the declared window. The VMM pauses TSC and derived guest time sources during the window and resumes deterministically. Clock jitter applies a declared bound to the same virtual clock.

### 2. CPU stall suspends a vCPU

CPU stall marks a vCPU as not runnable for the declared virtual window using the existing stall plumbing. The scheduler skips the stalled vCPU and resumes it exactly at the window end.

### 3. Memory pressure is guest-visible

Memory pressure exposes a deterministically managed memory ceiling to the guest. The implementation uses the existing `memory_limit_bytes` slot field and a guest-visible mechanism such as an admitted overcommit setting or a deterministic courtesy hog commanded by the host. Release returns the ceiling to the admitted baseline.

### 4. Effect evidence uses the stage ledger

Each new fault moves through Selected, Applicable, Applied, and Observed stages. A rejected, expired, or misapplied window records a typed rejection or application-failure record. Selection never implies workload impact.

### 5. Unsupported capability stays visible

Any capability that a profile still does not execute returns the same typed unsupported rejection it does today. The campaign policy chooses whether a rejection is fatal.

## Risks

Guest OOM behavior depends on the kernel and workload, not on the VMM. A memory-ceiling fault, therefore, observes application effects but cannot guarantee a guest-visible allocation failure. The evidence must record the applied ceiling and any observed guest behavior without overclaiming causation.
