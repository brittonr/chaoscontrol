# Deterministic resource, CPU, and clock faults

ChaosControl executes four fault classes that previously returned an unsupported result.

## Clock freeze

`ClockFreeze` records the current virtual TSC and a finite release tick. The virtual counter does not advance during this window.
At the release tick, counter advancement continues from the frozen value. The release does not add elapsed host time.

## Clock jitter

`ClockJitter` adds a deterministic guest-visible offset to the virtual TSC. The absolute offset does not exceed `bound_tsc`.
The same counter and bound produce the same value. Set `bound_tsc` to zero to remove jitter.

## CPU stall

`CpuStall` marks one vCPU as not runnable until its release tick. Other admitted vCPUs can continue.
A single-vCPU VM records zero exits while the stall is active. The scheduler makes the vCPU runnable at the exact release tick.

## Memory pressure

`MemoryPressure` contains a nonzero ceiling and a finite duration. The planner rejects a ceiling that is zero or not below the admitted baseline.
The VMM exposes the current ceiling through `chaoscontrol_sdk::resources::memory_ceiling_bytes`. It restores the baseline at the release tick.

The ceiling is an observation and policy surface. It does not change mapped guest memory or guarantee an allocation failure.

## Evidence and replay

Each accepted effect records `Selected`, `Applicable`, `Applied`, and `Observed` stages. Invalid windows and unsupported profiles record typed rejections.
Snapshots retain active deadlines, limits, and clock settings. Restore rejects state that does not match an observed applied plan.

These records prove bounded VMM behavior for the selected run. They do not prove guest-kernel OOM behavior, host timing, or production readiness.
