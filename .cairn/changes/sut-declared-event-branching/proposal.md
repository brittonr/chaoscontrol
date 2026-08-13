# SUT-Declared Event Branching

## Why

The explorer forks from coverage-improving snapshots and from `random_choice()` points. It cannot branch from states the workload declares interesting. Antithesis demonstrated this pattern with `ANT_REACH()` markers in SQLite `src/wal.c`: the harness branched from the WAL reset, backfill entry, and checkpoint states, which is how a tight race became reachable. ChaosControl has no equivalent guest-declared branch hook, and it does not treat reachability or assertion events as snapshot opportunities.

## What Changes

- Add an SDK surface for guest-declared event markers with stable identity.
- Have the VMM offer a snapshot at each declared marker to the frontier as a parent candidate.
- Branch at replay-derived and targeted marker instances when the marker is rare or newly observed.
- Bind the marker, the owning process or guest, the tick, and the snapshot into bug and replay evidence.

## Impact

- **SDK**: declared event marker API with stable identity.
- **VMM or explorer**: marker-driven frontier entry and parent snapshot capture.
- **Evidence**: markers enter fingerprints and replay verdicts.
- **Testing**: positive rare-event branching and negative never-reached and identity-conflict cases.

## Non-Goals

- No change to fault-schedule or input-tree exploration modes.
- No automatic marker placement in existing workloads.
- No hosted report surface.
