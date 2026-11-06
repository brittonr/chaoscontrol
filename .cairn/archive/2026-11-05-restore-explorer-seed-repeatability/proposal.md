# Restore Explorer Seed Repeatability

## Why

The explorer documents that exploration is deterministic for a given seed (`lib.rs`, "All exploration is deterministic given the same seed"). A 2026-09-25 local run contradicts that. Two runs used the same Nix-built explorer, kernel, Raft initrd, seed 42, and configuration (1 VM, 8 branches, 2000 ticks, bootstrap budget 5000). No watchdog fired in their first two rounds, and they still diverged:

| Variant | Round 1 new edges | Round 2 new edges |
| --- | --- | --- |
| control, run A / run B | 84 / 83 | 17 / 12 |
| `fig8_commit`, run A / run B | 83 / 84 | 7 / 16 |

The existing `vm-determinism-drift` and `vm-determinism-matrix` rails check single-VM execution. No rail checks the explorer's round-by-round trajectory, so this divergence went unnoticed.

## What Changes

- Record a per-branch digest of coverage, oracle report, exit counts, and total ticks so the first divergent branch can be located. r[chaoscontrol.explorer_repeatability.branch_digest]
- Add an explorer repeatability gate that runs the same configuration twice and compares per-round digests, frontier selections, and bug identities. r[chaoscontrol.explorer_repeatability.gate]
- Diagnose and remove the host-dependent input that causes the divergence. r[chaoscontrol.explorer_repeatability.cause]
- Exclude runs with host watchdog firings, discarded branches, or failed branches from repeatability claims. Report them as operational rather than as mismatches. r[chaoscontrol.explorer_repeatability.operational_exclusion]

## Impact

- **Code**: branch-result digests, a repeatability comparison core, a runner shell, and the fix the diagnosis identifies.
- **Evidence**: a passing gate supports seed repeatability only for the gated workload, kernel, initrd, configuration, and host profile.

## Lifecycle Prerequisites

- `repair-incremental-snapshot-lineage`: stale pages left between sibling branches are a candidate cause. The gate must run after that fix, so that its result is not confounded by the known lineage defect.

## Non-Goals

- No claim of determinism for runs with watchdog firings or discarded branches.
- No change to the single-VM determinism rails.
