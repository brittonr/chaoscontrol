# Design: Guest Multiprocess Topology

## Context

The VMM boots a kernel and one initrd image. The image currently runs one init binary. Fault injection addresses VM indices. The SDK maps one hypercall page for the whole process and the explorer binds assertions to a per-VM property oracle.

## Decisions

### 1. The process manifest is the admission unit

A guest image declares zero or more processes. Each entry binds an executable path, role name, arguments, environment fields, and membership in shared working directories. Nickel owns the human-authored manifest; Rust owns the runtime process state.

### 2. A deterministic supervisor owns process lifetime

A small PID-1 supervisor loads the manifest, spawns each process, and owns exit and restart policy. The supervisor is instrumented with the SDK so the host observes process lifecycle events deterministically.

### 3. Shared storage is one admitted device surface

Processes that share a working directory share one deterministic block or memory device mounted at that directory. Device ordering and fault semantics stay with the deterministic block device. The host does not model the filesystem.

### 4. Process faults are host-directed through the supervisor

The VMM sends a typed process fault to the supervisor over the hypercall transport. The supervisor kills, pauses, or restarts the named process at a deterministic boundary. The fault ledger reuses the existing Selected, Applied, and Observed stages with a process target.

### 5. Each process owns an isolated transport

The SDK uses a per-process allocation of the hypercall page region as dictated by the manifest, or a shared page behind a guest-resident lock for legacy single-page images. A process that never contacts the SDK emits no traffic. The property oracle aggregates per-process assertion events under explicit process identity.

### 6. Restart does not recreate shared state

Shared working directories and the underlying device survive a process restart. The manifest documents which state is process-resident and which is device-backed.

## Risks

A supervisor adds a moving part between the host and the workload. Its restart policy must be deterministic or restart evidence loses replay meaning. Per-process transport needs admission bounds so a large process set does not exhaust the reserved page region.
