# Default fault injection for system runs

## Why

A system run that injects no fault does not exercise the failure paths it
claims to cover. The VMM controller hard-coded `random_faults: false` when it
built its fault engine, so every system run explored only the faults its
schedule named, and a schedule that named none explored none at all. The
fault engine already supported seed-derived random injection and already had a
deterministic source under a fixed seed, so the safe default was available and
unused.

## What Changes

- Add an explicit `random_faults` knob to the simulation configuration, with
  the default on.
- Pass the configured value into the fault engine instead of hard-coding it
  off.
- Give the VM integration harness and the performance benchmark the explicit
  opt-out, because they assert undisturbed system behaviour.
- Document the default and the opt-out on the fault surface.

## Impact

- **Affected specs:** a new `chaoscontrol.system_runs.fault_injection` capability.
- **Planned files:** the VMM controller configuration and its engine wiring, the
  integration harness and benchmark opt-outs, the fault surface documentation,
  and the lifecycle package.
- **Compatibility:** a system run now injects seed-derived faults unless it
  opts out. The injection is deterministic under a fixed seed, so a run's
  behaviour is reproducible; what changes is that the default is no longer a
  fault-free system.
- **Testing:** the VMM crate's unit tests and build, the repository's own
  gates, and the Cairn gates.

## Out of Scope

- No change to the fault engine's semantics, its schedule model, or its
  determinism guarantees.
- No change to the exploration profile's explicit `exploration.mode`, which
  already names fault schedule, input tree, or hybrid.
- No new fault kind and no change to the fault catalogue.
- No claim that a fault-free run is now impossible. The opt-out stays
  available and explicit.
