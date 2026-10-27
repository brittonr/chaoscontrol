# Deterministic resource, CPU, and clock faults

## Directional network faults

`Fault::NetworkDirectional` applies a disruption to one link endpoint and
direction rather than to one VM bidirectionally. The admitted modes are:

- `Stopped` drops packets in the selected direction.
- `Slowed` adds the planned delay in virtual ticks.
- `Jammed` defers packets by a finite bounded amount.

`Fault::NetworkDirectionalHeal` clears the selected link pair in both
directions. `NetworkFabric` records cumulative directional drop, delay, jam,
and delay-tick counters. The effective direction is deterministic and the
fault is included in packet preflight bounds.

The fabric can also admit a baseline latency and low-loss profile applied to
all traffic. Baseline loss is seed-derived and baseline latency is recorded in
the cumulative network counters.

These faults are guest-network simulation facts. They do not manipulate host
networking, prove packet absence, or claim that the selected direction was the
only active fault unless the stage ledger says so.

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

## CPU throttle

`CpuThrottle` limits a VM's aggregate vCPU exits per scheduling round. The scheduler clamps the
kernel plan's exit budget to the admitted `budget_units` while the throttle is active.
The throttle releases at its exact `release_at_tick`, and replay snapshots retain both the budget
and release tick. The planner rejects a zero or over-bound budget.

## Memory pressure

`MemoryPressure` contains a nonzero ceiling and a finite duration. The planner rejects a ceiling that is zero or not below the admitted baseline.
The VMM exposes the current ceiling through `chaoscontrol_sdk::resources::memory_ceiling_bytes`. It restores the baseline at the release tick.

The ceiling is an observation and policy surface. It does not change mapped guest memory or guarantee an allocation failure.

## Pause control

An admitted pause-control window suppresses new fault selection while the virtual tick is inside
the window. Already-applied effects continue to run; the window does not clear them. Replay
snapshots retain the admitted windows, so a restored run resumes the same suppression boundary.

## System runs inject faults by default

A system run configures its fault engine with `random_faults` set, and the default is on.
The engine derives the injected faults from the run's master seed, so the same seed and configuration inject the same faults on every repetition.
A run that needs a quiet system sets `random_faults: false` explicitly; the opt-out is a visible choice, not an omission.
The fault engine's own library default stays off, so a caller that builds an engine directly is unaffected.

Exploration runs take the same setting from the operator.
`chaoscontrol-explore run` and `campaign` accept `--random-faults true|false`, and a run profile accepts `exploration.random_faults`; both default to on.
The explorer, its workers, and the auto-minimizer build every system run with that value.

The run records the value it used, always as an explicit boolean, in `checkpoint.json`, `campaign_progress.json`, and every bug report; replay recordings carry it too.
`resume`, `campaign-resume`, `minimize`, `reproduce`, and replay use the recorded value, so a bug is reproduced under the setting that found it.
Records written before the setting existed carry no value and load as off, because those runs built their fault engine with random injection hard-coded off.
The dogfood `run-config.json` projection carries the setting whenever the checkpoint records it, and keeps the legacy shape for checkpoints that predate it.

## Interactions and precedence

| Situation | Effective rule |
|---|---|
| Partition plus directional fault | Partition drops first; directional faults apply only to reachable links. |
| Overlapping directional windows on one link and direction | The most severe mode wins: `Stopped` > `Jammed` > `Slowed`. |
| Directional heal | Clears both directions for the selected link pair. |
| Directional fault plus baseline profile | Directional mode and delay apply before baseline loss and baseline latency. |
| Per-VM loss plus baseline loss | Baseline loss is evaluated first, then the per-VM loss rate. |
| CPU stall plus CPU throttle | The stall keeps the vCPU unrunnable; the throttle clamps the aggregate budget for runnable work. |
| Pause window plus active effect | New selection is suppressed inside the window; an already-applied effect continues until its own release. |

The stage ledger records the attempt identity, mechanism, effective target, and observed stage for
each accepted effect. It does not infer that a suppressed fault was absent from the host.

### Explicit unsupported cases

- Process-scoped CPU throttle targets are admitted by the pure contract but are not enforced by the VMM.
- Custom effects require an admitted Rust adapter and are not executed by ambient commands or shell text.
- Pause control is an engine-surface request; no CLI flag or Nickel run-profile field is defined yet.
- Host CPU affinity, cgroup limits, and host network manipulation are outside this mechanism.

## Evidence and replay

Each accepted effect records `Selected`, `Applicable`, `Applied`, and `Observed` stages. Invalid windows and unsupported profiles record typed rejections.
Snapshots retain active deadlines, limits, and clock settings. Restore rejects state that does not match an observed applied plan.

These records prove bounded VMM behavior for the selected run. They do not prove guest-kernel OOM behavior, host timing, or production readiness.
