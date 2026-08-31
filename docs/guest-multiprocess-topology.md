# Guest multiprocess topology

ChaosControl can package a bounded set of cooperating processes in one guest.
The feature is experimental. It does not change single-process initrd behavior.

## Manifest

`contracts/guest-processes/process-manifest.ncl` owns the typed manifest. Each
process declares an absolute executable path, role, arguments, environment,
restart policy, shared directory membership, and optional SDK transport slot.
The Rust admission core repeats all bounds before execution. It assigns stable
BLAKE3 identities to the manifest, processes, and shared directories.

The current bounds are 32 processes, 16 shared directories, 64 arguments per
process, and 64 environment fields per process. Duplicate roles, unknown shared
directories, unsafe paths, excess restart budgets, and transport-slot conflicts
fail before a process starts.

## Supervisor

`chaoscontrol-protocol::guest_process` owns the deterministic state machine.
`chaoscontrol-sdk::supervisor` owns the process shell. The core plans spawn,
kill, pause, resume, exit, and restart transitions. The shell alone creates
directories, starts child processes, sends signals, and observes child exits.

The Nix package `guest-supervisor` builds a static PID-1 binary. The
`initrd-multiprocess` package contains the supervisor, two fixture services, the
manifest, and one shared memory-backed `/data` directory.

Restart replaces process-resident state. It does not replace a declared shared
directory or its admitted device identity. ChaosControl does not model
filesystem semantics.

## Host-directed process faults

A `SimulationController` can queue a typed `ProcessFaultCommand` for one VM.
The command names one process identity or unique role. The guest supervisor
polls the bounded queue through `CMD_PROCESS_FAULT_POLL`. Kill, pause, and
restart affect only the selected child. An unknown role, malformed command,
duplicate request identity, or full queue fails with a typed error.

The pending command queue is part of the fault-engine snapshot. Restore and
replay therefore preserve the same unobserved host command.

## SDK transport and evidence

Instrumented child processes set `CHAOSCONTROL_SDK_TRANSPORT_LOCK`. The SDK
holds one guest-wide file lock before it writes the shared hypercall page and
until the host response returns. This lock prevents concurrent page mutation.
A missing configured lock fails the SDK call.

The first bounded cohort uses one shared assertion catalog. Repeated exact
catalog submission is idempotent. A conflicting catalog still fails closed.
Assertion details include the process BLAKE3 identity. Oracle records retain a
bounded `process_instances` set. Stable assertion descriptors should also use
the process role as their guest owner.

`chaoscontrol.multiprocess-receipt.v1` binds the manifest, process, shared
directory, and lifecycle-event identities. Its claim scope is exactly
`declared-processes-only`. Identity drift, an unknown event owner, duplicate
identity, or a broader claim fails validation.

## Claim boundary

This feature does not provide container namespaces, cgroups, registry behavior,
Kubernetes semantics, cross-VM process scheduling, or cross-machine control.
A shared directory proves only that declared processes use one admitted guest
device path. A passing process receipt does not prove application correctness,
filesystem correctness, or complete process-fault observation.
