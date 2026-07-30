## Why

Multi-vCPU execution currently arms a host POSIX timer before each `KVM_RUN`. Consecutive `SIGALRM`/`EINTR` exits directly select another runnable vCPU. The instruction counter is opened in counting mode, but the live step path does not read it or call the dormant quantum-switch path; if PMU setup fails, VM creation explicitly falls back to timer-only switching. Host load, signal delivery, and KVM entry latency can therefore change which vCPU executes next, which real exits advance virtual time, and which state is captured.

A wall-clock watchdog can protect operator liveness, but it cannot be an input to a deterministic guest schedule or guest-crash verdict.

## What Changes

- Route every vCPU selection through a pure schedule state machine driven only by replay-stable configuration, guest execution progress, runnable state, and seeded choices.
- Implement an exact deterministic progress boundary for no-exit guest loops; optional acceleration must prove the same boundary and detect overshoot.
- Remove `SIGALRM`/`EINTR` counts and arrival order from vCPU-switch and guest-verdict decisions.
- Fail closed when the requested deterministic progress source is unavailable instead of silently falling back to wall-clock preemption.
- Keep host watchdogs as an operational abort mechanism with an explicit non-deterministic timeout classification that cannot satisfy deterministic replay evidence.
- Persist and trace the complete deterministic scheduler/progress state needed to resume at the same boundary.
- Add spurious-interrupt, host-delay, PMU-unavailable, spin-loop, snapshot-resume, and repeated-run schedule tests.

## Impact

- **Files**: `crates/chaoscontrol-vmm/src/vm.rs`, `perf.rs`, scheduler/progress state, dlog schedule records, VM configuration, snapshot-state adapters, and SMP tests.
- **Compatibility**: deterministic SMP startup will return a capability error when no approved progress source is available; it will not continue in timer-only mode.
- **Performance**: a portable exact progress source may be slower than wall-clock liveness switching; accelerated modes remain opt-in until they preserve exact boundaries.
- **Reliability**: host watchdog expiration stops the run without fabricating a guest crash, schedule choice, or replay-success result.
- **Claims**: schedule repeatability is scoped to the declared guest, CPU/KVM capability profile, deterministic progress source, and tested observation horizon.
