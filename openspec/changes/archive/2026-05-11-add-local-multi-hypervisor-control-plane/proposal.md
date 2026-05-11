## Why

The desired product shape is a single machine running multiple local ChaosControl hypervisors. Existing local multi-hypervisor and KVM smoke evidence proves bounded execution, but the operator-facing control plane still needs stronger orchestration: resource placement, restart behavior, artifact hygiene, bug handoff, and a local dashboard.

## What Changes

- **Local control plane**: Manage N local hypervisor workers from one durable queue/state file.
- **Resource isolation**: Pin CPUs, bound memory/artifact directories, and attribute failures to a worker/run.
- **Bug handoff**: Automatically schedule reproduce/minimize follow-up work locally when a worker finds a bug.
- **Local dashboard**: Render queue, worker, run, bug, reproduce/minimize, and receipt status without hosted service claims.

## Capabilities

### Modified Capabilities
- `replay-readiness-operator`: Extends local multi-hypervisor orchestration and dashboard evidence.
- `campaign-dashboard`: Adds a single-machine multi-hypervisor operator view.

## Impact

- **Files**: Evidence models/validators, scheduler CLI, Nix local rail, dashboard renderer/docs.
- **APIs**: CLI flags and receipt schema additions; no guest SDK API required.
- **Dependencies**: None expected.
- **Testing**: Pure model tests, receipt fixtures, local sample runner, optional KVM smoke.
