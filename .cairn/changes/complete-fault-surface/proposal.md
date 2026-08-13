# Complete Fault Surface

## Why

The fault engine documents 27 fault types across six categories, but several resource and clock faults return explicit unsupported-capability rejections. Clock freeze, memory pressure, and CPU stall are advertised by the campaign API and presence of VM slot fields (`clock_freeze`, `vcpu_stall_until`, `memory_limit_bytes`) without working effect execution. A distributed-system bug that needs a hung clock, a memory ceiling, or a stalled CPU cannot be staged or replayed.

## What Changes

- Implement clock freeze and clock jitter as deterministic virtual-clock effects.
- Implement CPU stall as deterministic vCPU-scheduling suspension for a declared window.
- Implement memory pressure as a deterministic guest-visible memory ceiling with admission and release evidence.
- Move the effect plan, application, and observation records through the existing six-stage fault ledger.
- Keep unsupported capability rejection as a typed, visible outcome for any remaining profile.

## Impact

- **VMM**: virtual clock, vCPU suspension, and memory-ceiling effect execution.
- **Evidence**: fault stages and receipts cover the new effects.
- **Testing**: positive freeze, stall, and pressure cases and negative window, bound, and release cases.

## Non-Goals

- No host memory stealing or cgroup dependency.
- No new fault categories beyond the three named effects and their release forms.
- No change to network or disk fault semantics.
