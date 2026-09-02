# Design: Cooperative interruption and forced exit

## Context

The explore shell installs SIGINT and SIGTERM handlers. The first signal sets a global flag. Explorer and campaign loops poll that flag after bounded work units.

The second signal calls `std::process::exit` from the handler. The source calls that handler async-signal-safe, but Rust does not establish that postcondition for `std::process::exit`.

The old graceful-shutdown specification exists only in migration archives. A current accepted capability must state the lifecycle and evidence boundary.

## Terms

- **Stop request:** the shell observed the first operator signal.
- **Cooperative interruption:** the application reached a declared boundary and stopped new admission.
- **Finalization:** the shell saves required progress and produces the bounded report.
- **Interrupted:** finalization completed and the terminal result was published.
- **Interruption failed:** required finalization failed or remained uncertain.
- **Forced exit:** a repeated signal ended the process without cooperative completion evidence.

Graceful shutdown is the application policy. Cooperative interruption is its normal mechanism for exploration and campaign commands.

## Lifecycle

```text
Running
  -> StopRequested
  -> BoundaryReached
  -> Finalizing
  -> Interrupted | InterruptionFailed

Running | StopRequested | BoundaryReached | Finalizing
  -> ForcedExit
```

A signal request is never terminal. The first path stops after the current declared work unit. Explorer uses a round boundary. Campaign execution uses its documented round and seed boundaries.

No new round or seed starts after the shell observes the request at its admission boundary.

## Functional core

A narrow reducer receives supplied signal count, command phase, boundary observation, checkpoint disposition, report disposition, and output policy.

It returns the next phase, admission decision, terminal class, report plan, and claim limits. It performs no signal, process, clock, filesystem, KVM, output, or persistence effect.

The reducer rejects stale phases, terminal mutation, duplicate finalization, a completion without its required boundary, and a successful claim after finalization failure.

## Signal shell

The handler performs only operations with an established async-signal-safe contract.

The first signal records request intent through atomics and returns. It does not log, allocate, persist, render, or publish a terminal result.

A repeated signal uses an immediate async-signal-safe exit primitive. The implementation uses a named nonzero exit status. It runs no Rust cleanup protocol and emits no terminal receipt.

The shell owns handler installation, atomics, process exit, logging, checkpoints, reports, and command status.

## Finalization and compatibility

The existing serialized `interrupted` finish reason can remain for compatibility. Its documented meaning narrows to completed cooperative interruption.

When an output directory is configured, required checkpoint and progress writes must succeed before the command reports `interrupted`. A failure produces a non-successful interruption class.

When no output directory is configured, the policy requires the bounded report but no checkpoint file.

A campaign completes its current documented unit, skips later seeds, writes admitted progress, and aggregates only completed observations.

## Fault and recovery scope

Harness interruption controls the exploration command. It is not a simulated guest fault.

`ProcessKill` is an abrupt guest termination attempt. `ProcessRestart` is a separate recovery attempt. Each keeps its fault-stage identity and observation.

A restart observation proves only that the selected restart mechanism ran. Consumer-owned oracles must establish data integrity, protocol recovery, and progress.

Storage-loss, torn-write, and flush-failure campaigns remain under their existing specifications. This change adds no universal crash-recovery claim.

## Verification

Positive cases cover a first signal during a round, boundary completion, checkpoint or no-output policy, report completion, skipped later work, and terminal classification.

Negative cases cover checkpoint failure, report failure, duplicate finalization, stale phase, premature terminal reporting, and new work after request observation.

A subprocess fixture sends repeated signals and requires the named nonzero exit result. It must find no cooperative terminal marker created after the forced path.

Fault-scope fixtures prove that harness interruption cannot produce guest `ProcessKill` or `ProcessRestart` evidence. Guest restart cannot produce harness interruption evidence.

## Claim Boundary

Passing evidence proves only the selected ChaosControl command lifecycle. It does not prove arbitrary cleanup, application recovery, signal delivery timing, guest correctness, or production readiness.
