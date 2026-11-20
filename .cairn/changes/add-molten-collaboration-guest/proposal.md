## Why

Aspen's collaboration-native runtime is a real three-process Raft service with durable state, explicit source admission, installed effects, and a committed completion path. It has no ChaosControl guest workload, so the runtime cannot be executed under deterministic VM schedules, snapshots, fault injection, or retained campaign evidence.

Aspen's existing `collaborationnative` integration driver already starts the three signed node processes, installs the native effect, submits protected work, observes the commit, shuts the owner down, and reopens it. Reimplementing that driver in ChaosControl would duplicate authority and lifecycle semantics. The missing capability is a guest wrapper and initrd that run the existing driver against an independently provisioned signed source trust.

## What Changes

- Add a `chaoscontrol-molten-guest` wrapper that mounts guest pseudo-filesystems, selects the source trust from the kernel cmdline, runs Aspen's exact `collaborationnative` driver case, and records lifecycle/assertion evidence.
- Add an initrd assembly path that preserves the signed Aspen output's absolute Nix store identity, its runtime closure, the driver binary, and the operator-provisioned trust record.
- Add a single-guest, no-fault campaign as the first executable evidence slice.
- Add a three-guest cohort only after the single-guest flow completes; each guest starts one independently admitted node and the campaign records crash/restart, partition, and recovery observations.
- Keep guest evidence scoped to the exact Aspen output, closure, trust record, driver revision, kernel, initrd, seed, schedule, and bounds.

## Impact

- **Surfaces**: a Rust guest wrapper, Nix initrd assembly, an exploration app/campaign configuration, and evidence classification.
- **Scope**: no new guest-language SDK, no Aspen runtime changes, no synthetic source trust, no authority derivation from successful execution.
- **Dependencies**: an independently signed Aspen node output and source trust; the exact driver binary built from the matching Aspen revision.
- **Claims**: guest execution proves only the bounded runtime, schedule, inputs, and observations under the selected signed output. It is not global exactly-once, arbitrary-fault, or production-readiness proof.
