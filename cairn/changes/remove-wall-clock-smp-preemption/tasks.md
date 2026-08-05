## Phase 1: Deterministic scheduling core

- [ ] [serial] Define pure schedule inputs, `ScheduleState`, transition actions, invariants, and BLAKE3 state identity over active/runnable vCPUs, policy state, seeded choices, deterministic progress, quantum boundary, and exact-step state. r[chaoscontrol.deterministic_smp.schedule_core]
- [ ] [serial] Restrict transition inputs to replay-stable guest events and reject host signal, elapsed-time, thread, or ambient-state inputs. r[chaoscontrol.deterministic_smp.wall_clock_isolation]
- [ ] [serial] Add positive transition assertions and negative invalid-vCPU, impossible-progress, overshoot, stale-event, and host-event assertions. r[chaoscontrol.deterministic_smp.validation.core]

## Phase 2: Exact execution progress

- [ ] [serial] Implement a portable exact progress source that can preempt a no-exit guest loop at a named instruction boundary. r[chaoscontrol.deterministic_smp.progress_boundary]
- [ ] [parallel] Implement optional PMU acceleration with capability checks, exact remainder handling, and typed overshoot or lost-progress rejection. r[chaoscontrol.deterministic_smp.progress_boundary] r[chaoscontrol.deterministic_smp.capability_policy]
- [ ] [serial] Select and validate the declared progress mode before execution and remove implicit timer-only SMP fallback. r[chaoscontrol.deterministic_smp.capability_policy]
- [ ] [serial] Route all active-vCPU changes through the pure transition core and delete dormant or duplicate switch authority. r[chaoscontrol.deterministic_smp.schedule_core]

## Phase 3: Watchdog and state evidence

- [ ] [serial] Make `SIGALRM`, `VcpuExit::Intr`, and `EINTR` retry or return an operational watchdog timeout without changing deterministic schedule, virtual time, or guest verdict state. r[chaoscontrol.deterministic_smp.watchdog]
- [ ] [serial] Emit canonical progress and switch records with pre/post BLAKE3 schedule-state identities and no wall-clock fields in deterministic identity. r[chaoscontrol.deterministic_smp.schedule_evidence]
- [ ] [serial] Expose complete schedule/progress state to the VM snapshot adapter without taking ownership of whole-VM payloads or replay artifact references. r[chaoscontrol.deterministic_smp.snapshot_state]

## Phase 4: Regression evidence

- [ ] [parallel] Add spurious-interrupt tests proving arbitrary `Intr` and `EINTR` sequences cannot change vCPU selection or deterministic counters. r[chaoscontrol.deterministic_smp.validation.spurious_interrupts]
- [ ] [parallel] Add PMU-unavailable and PMU-overshoot tests proving deterministic startup or execution fails closed without timer-only fallback. r[chaoscontrol.deterministic_smp.capability_policy]
- [ ] [parallel] Add no-exit spin-loop tests proving switching occurs only at the declared deterministic progress boundary. r[chaoscontrol.deterministic_smp.validation.spin_loop]
- [ ] [serial] Add repeated KVM runs under injected host delay, watchdog-cadence variation, and CPU contention and compare canonical transition traces and bounded guest observations. r[chaoscontrol.deterministic_smp.validation.host_perturbation]
- [ ] [serial] Add snapshot/resume tests at partial quantum and exact-step boundaries in coordination with `complete-vm-snapshot-state`. r[chaoscontrol.deterministic_smp.validation.snapshot]
- [ ] [serial] Document progress modes, capability failures, watchdog non-claims, and bounded portability. r[chaoscontrol.deterministic_smp.capability_policy] r[chaoscontrol.deterministic_smp.watchdog]
- [ ] [serial] Run focused scheduler/perf/KVM tests, workspace tests, determinism comparison tests, Cairn validation, and proposal/design/tasks gates before sync or archive. r[chaoscontrol.deterministic_smp.validation]
