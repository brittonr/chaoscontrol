# Guest Multiprocess Topology

## Why

ChaosControl tests one low-level guest process per VM. Real systems are multi-process: databases run writer and checkpoint roles, distributed services run several cooperating binaries, and their bugs live in the interleavings between processes sharing state. The fault engine targets whole VMs, so it cannot crash one process while another survives. The SDK transport is one shared hypercall page, so two SDK-instrumented processes in one VM would corrupt each other's assertion traffic.

## What Changes

- Declare a guest image as a set of processes with roles, arguments, and shared working directories.
- Run a deterministic guest supervisor as PID 1 that spawns, monitors, and restarts the declared processes.
- Give processes a shared deterministic storage surface (a common working directory backed by one deterministic block or memory device).
- Target process faults (crash, pause, restart) at a process identity, not the whole VM.
- Give each process an isolated, deterministic SDK transport so concurrent assertion traffic cannot cross-corrupt.

## Impact

- **SDK**: per-process assertion transport.
- **VMM**: per-process fault application and supervisor protocol.
- **Guest packaging**: supervisor binary and process manifest.
- **Testing**: positive multi-writer sanity, negative process-crash, restart, and transport-collision cases.

## Non-Goals

- No cross-VM or cross-machine process scheduling.
- No container image format (that is `oci-container-intake`).
- No change to single-process campaigns.
