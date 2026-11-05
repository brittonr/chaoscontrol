## Context

`chaoscontrol-protocol::diagnostic` owns admission and encoding. The supervisor has
an admitted manifest with executable paths, while the request carries only the exact
BLAKE3 identity and arguments.

## Decisions

### Decision: Supervisor supplies the execution path

**Choice:** `run_diagnostic` takes the executable path separately and verifies its
bytes against the request identity before spawning.

**Rationale:** Host requests stay path-free, and identity verification detects file
substitution between admission and execution.

### Decision: Bounded synchronous runner

**Choice:** Capture at most 64 KiB per stream, poll `try_wait` on a 5 ms interval,
kill and reap on timeout, and return an outcome instead of guest-visible content.

**Rationale:** Digest-only outcomes keep receipts bounded. The runner is
synchronous and intended for supervisor-owned diagnostic requests, not workload
processes.

## Risks / Trade-offs

- Polling adds up to 5 ms of latency; the executable is read up to 16 MiB.
- A killed process may leave descendants; supervisor ownership and cleanup remain
  outside this runner.
