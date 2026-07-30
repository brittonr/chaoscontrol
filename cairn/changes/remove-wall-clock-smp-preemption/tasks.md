## Phase 1: Deterministic scheduling core

- [x] [serial] Define pure schedule inputs, `ScheduleState`, transition actions, invariants, and BLAKE3 state identity over active/runnable vCPUs, policy state, seeded choices, deterministic progress, quantum boundary, and exact-step state. r[chaoscontrol.deterministic_smp.schedule_core]
- [x] [serial] Restrict transition inputs to replay-stable guest events and reject host signal, elapsed-time, thread, or ambient-state inputs. r[chaoscontrol.deterministic_smp.wall_clock_isolation]
- [x] [serial] Add positive transition assertions and negative invalid-vCPU, impossible-progress, overshoot, stale-event, and host-event assertions. r[chaoscontrol.deterministic_smp.validation.core]

## Phase 2: Exact execution progress

- [x] [serial] Implement a portable exact progress source that can preempt a no-exit guest loop at a named instruction boundary. r[chaoscontrol.deterministic_smp.progress_boundary]
- [x] [parallel] Implement optional PMU acceleration with capability checks, exact remainder handling, and typed overshoot or lost-progress rejection. r[chaoscontrol.deterministic_smp.progress_boundary] r[chaoscontrol.deterministic_smp.capability_policy]
- [x] [serial] Select and validate the declared progress mode before execution and remove implicit timer-only SMP fallback. r[chaoscontrol.deterministic_smp.capability_policy]
- [x] [serial] Route all active-vCPU changes through the pure transition core and delete dormant or duplicate switch authority. r[chaoscontrol.deterministic_smp.schedule_core]

## Phase 3: Watchdog and state evidence

- [x] [serial] Make `SIGALRM`, `VcpuExit::Intr`, and `EINTR` retry or return an operational watchdog timeout without changing deterministic schedule, virtual time, or guest verdict state. r[chaoscontrol.deterministic_smp.watchdog]
- [x] [serial] Emit canonical progress and switch records with pre/post BLAKE3 schedule-state identities and no wall-clock fields in deterministic identity. r[chaoscontrol.deterministic_smp.schedule_evidence]
- [x] [serial] Permanently poison VM execution after post-entry evidence, exit-handling, or schedule-action failure. r[chaoscontrol.deterministic_smp.vm_poison]
- [x] [serial] Expose complete schedule/progress state to the VM snapshot adapter without taking ownership of whole-VM payloads or replay artifact references. r[chaoscontrol.deterministic_smp.snapshot_state]
- [x] [serial] Latch controller poison after any failed round mutation and block later execution, mutation, snapshot, restore, recording, and success paths. r[chaoscontrol.deterministic_smp.controller_poison]

## Phase 4: Regression evidence

- [x] [parallel] Add spurious-interrupt tests proving arbitrary `Intr` and `EINTR` sequences cannot change vCPU selection or deterministic counters. r[chaoscontrol.deterministic_smp.validation.spurious_interrupts]
- [x] [parallel] Add PMU-unavailable and PMU-overshoot tests proving deterministic startup or execution fails closed without timer-only fallback. r[chaoscontrol.deterministic_smp.capability_policy]
- [x] [parallel] Add negative post-commit exit-handling and schedule-action tests proving permanent VM poison. r[chaoscontrol.deterministic_smp.validation.vm_poison]
- [x] [parallel] Add no-exit spin-loop tests proving switching occurs only at the declared deterministic progress boundary. r[chaoscontrol.deterministic_smp.validation.spin_loop]
- [x] [serial] Add repeated KVM runs under injected host delay, watchdog-cadence variation, and CPU contention and compare canonical transition traces and bounded guest observations. r[chaoscontrol.deterministic_smp.validation.host_perturbation]
- [x] [serial] Add snapshot/resume tests at partial quantum and exact-step boundaries in coordination with `complete-vm-snapshot-state`. r[chaoscontrol.deterministic_smp.validation.snapshot]
- [x] [serial] Add a multi-VM failed-round test proving retry cannot advance an earlier VM, tick, fault state, or network state. r[chaoscontrol.deterministic_smp.validation.controller_poison]
- [x] [serial] Document progress modes, capability failures, watchdog non-claims, and bounded portability. r[chaoscontrol.deterministic_smp.capability_policy] r[chaoscontrol.deterministic_smp.watchdog]
- [x] [serial] Run focused scheduler/perf/KVM tests, workspace tests, determinism comparison tests, Cairn validation, and proposal/design/tasks gates before sync or archive. r[chaoscontrol.deterministic_smp.validation]
