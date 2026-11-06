## Context

The fault engine is a closed catalogue with a pure planner and a VMM shell. It already distinguishes selection, applicability, application, and observation. The new mechanisms must fit that model instead of introducing a parallel fault path.

The network fabric currently applies per-VM, bidirectional latency, loss, corruption, reorder, duplication, and bandwidth limits. The scheduler can stop a vCPU or a whole process. The CPU model has no resource budget. Custom behavior requires a new enum variant. No workload can ask the fault engine to stop making new faults while it checks a liveness property.

## Decisions

### Decision: Network faults gain a directional link target

**Choice:** a network fault names a source/destination link, an inbound, outbound, or both direction, and a disruption mode. `Stopped` drops, `Slowed` delays, and `Jammed` queues for later delivery. A scoped heal clears the named link without clearing unrelated links. The baseline profile is admitted as a separate named profile and is recorded with every affected packet observation.

**Rationale:** Antithesis's useful network model is link- and direction-scoped. Per-VM bidirectional faults cannot express an asymmetric failure that leaves one direction healthy.

### Decision: CPU throttle uses a virtual execution budget

**Choice:** the throttle assigns a target a finite quantum or instruction budget over a finite virtual window. The scheduler admits no further target progress after the budget is exhausted until the window releases. No host cgroup, `nice`, affinity, or wall-clock sleep participates.

**Rationale:** a deterministic budget is replayable and observable; host scheduling controls are not.

### Decision: Custom faults are typed effects, not scripts

**Choice:** a custom fault descriptor names a stable effect identity and bounded typed parameters. An admitted Rust-owned adapter implements the effect. The descriptor is selected and ordered by the same seeded schedule as catalogue faults. There is no ambient shell, unbounded script, dynamic library, network fetch, or package installation.

**Rationale:** arbitrary scripts would bypass the repository's typed-operation and determinism boundaries. A typed effect port preserves extensibility without giving up replay or authority checks.

### Decision: Pause control stops new work, not observed effects silently

**Choice:** a pause-control request suppresses new fault selection and application for a bounded virtual-time window. Already-applied effects continue unless a separate explicit release mechanism exists. The request records its requester, virtual window, and observed effect on selection and application. It never rewinds an effect.

**Rationale:** a workload may want a clean liveness window, but silently removing active effects would change the state under test and break replay evidence.

### Decision: Overlap has one deterministic precedence table

**Choice:** network faults on one link may overlap; the more restrictive mode wins for the overlap while both windows remain recorded. Process and resource faults admit at most one active effect per target; a second request is rejected or skipped with a stable reason. Pause control affects only new selections. Every interaction is recorded, including skipped and suppressed faults.

**Rationale:** implicit or host-order-dependent overlap would make replays disagree about which effect was effective.

## Risks / Trade-offs

- Directional link state and baseline latency can grow the network fabric's evidence and state; named link and packet bounds are required.
- A quantum budget can starve a target and trigger unrelated watchdog behavior; the release window and `Observed`/`Applied` accounting must distinguish intended throttling from a stall.
- Typed custom effects add an adapter surface that needs admission, versioning, and rejection tests.
- Pause control can hide a fault that would otherwise be selected; the receipt must show the requested window and the suppressed selections.
- Overlap precedence can be surprising; the table and the per-link effective-fault record must be reviewed fixtures.
