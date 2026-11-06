# Proposal: Expand deterministic fault mechanisms

## Why

ChaosControl has a broad fault catalogue: 30 variants across network, disk, process, clock, resource, interrupt, and CPU classes. The mechanisms are still narrower than the fault model used by Antithesis in four concrete ways:

- Network faults are bidirectional per VM. There is no typed inbound/outbound direction, no link-scoped stopped/slowed/jammed partition mode, and no admitted baseline latency/low-loss profile.
- Resource faults include memory pressure and vCPU stall, but no bounded CPU-bandwidth or quota throttle.
- The catalogue is closed. A consumer cannot declare a typed custom fault effect without changing the enum and rebuilding the engine.
- There is no workload-triggered, virtual-time-bounded pause of new fault selection or application around a liveness check.

These are mechanism gaps, not catalogue-count gaps. Each must be added without host-level fault injection, wall-clock behavior, unbounded scripts, or weaker replay.

## What Changes

- Add directional, link-scoped network faults with stopped/slowed/jammed modes, scoped heal, and an admitted baseline latency/low-loss profile. r[chaoscontrol.fault_mechanisms.directional_network]
- Add a bounded CPU-throttle fault expressed as a deterministic virtual instruction or quantum budget over a finite window, with no host cgroup dependency. r[chaoscontrol.fault_mechanisms.cpu_throttle]
- Add a typed custom-fault effect port with a declared identity, bounded parameters, deterministic selection, and an admitted Rust-owned adapter. r[chaoscontrol.fault_mechanisms.custom_effect]
- Add a workload- or operator-triggered pause of new fault selection/application for a bounded virtual-time window, with explicit treatment of already-active effects. r[chaoscontrol.fault_mechanisms.pause_control]
- Define deterministic overlap and precedence rules across network, process, resource, and pause-control mechanisms, with recorded skip/reject reasons. r[chaoscontrol.fault_mechanisms.interactions]
- Extend the fault stage ledger and receipts to every new mechanism without weakening `Selected`, `Applicable`, `Applied`, `Observed`, rejection, or application-failure meaning. r[chaoscontrol.fault_mechanisms.evidence]
- Preserve snapshot, replay, and claim boundaries for each mechanism. r[chaoscontrol.fault_mechanisms.nonclaims]
- Add positive and negative fixtures for every new mechanism and for their interactions. r[chaoscontrol.fault_mechanisms.fixtures]

## Impact

- **Core**: new typed fault variants, directional target admission, throttle scheduling, custom-effect descriptor admission, pause-window arithmetic, overlap classification, and stage evidence stay pure and deterministic.
- **VMM and network fabric**: directional queues, scoped link state, baseline profile application, quantum-budget enforcement, and paused selection/application.
- **SDK and supervisor**: bounded workload pause requests and the admitted custom-effect adapter boundary.
- **Existing owners**: the fault engine keeps schedule and stage ownership; the VMM keeps execution; the SDK keeps guest request ownership.
- **Consumers**: explorers and system runs can select the new mechanisms only through admitted profiles.

## Lifecycle Prerequisites

None. Each mechanism can land behind its own admission and fixtures. Existing fault variants and their serialized forms retain their current meaning.

## Out of Scope

- Host-level network, cgroup, CPU, or process faults.
- Arbitrary unbounded scripts, ambient shells, package installation, network fetches, or non-Rust adapters for custom effects.
- Antithesis parity, universal fault coverage, or a claim that a selected fault was observed or caused a finding.
- Non-Rust guests, containers, hosted services, or cross-machine execution.

## Affected Specs

- `fault-mechanisms`: directional network, CPU throttle, custom effects, pause control, interactions, evidence, non-claims, and fixtures.
