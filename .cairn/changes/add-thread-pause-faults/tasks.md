# Tasks

## Phase 1: Baseline and runtime boundary

- [x] [serial] Record current process, vCPU, and scheduler pause behavior and the existing fault stage ledger before source changes. r[chaoscontrol.thread_pause.fault] r[chaoscontrol.thread_pause.evidence]
- [x] [serial] Inventory the guest runtime and SDK surfaces that can own a stable thread identity and cooperative safe point. r[chaoscontrol.thread_pause.identity] r[chaoscontrol.thread_pause.safepoints]
- [ ] [serial] Publish the versioned safe-point and pause-gate ABI, including its supported runtime cohort and explicit unsupported cases. r[chaoscontrol.thread_pause.safepoints] r[chaoscontrol.thread_pause.nonclaims]

## Phase 2: Identity, fault, and planner

- [ ] [serial] Implement logical thread identity assignment, generation tracking, and snapshot-safe lookup in the admitted runtime. r[chaoscontrol.thread_pause.identity]
- [ ] [serial] Add the typed thread-pause fault variant and pure admission for target, window, and named bounds. r[chaoscontrol.thread_pause.fault]
- [ ] [parallel] Add negative planner fixtures for unknown, reused, duplicate, zero-length, overflowing, and out-of-window targets. r[chaoscontrol.thread_pause.fault] r[chaoscontrol.thread_pause.fixtures]

## Phase 3: Safe-point gate and deterministic release

- [ ] [serial] Implement the cooperative safe-point gate and guest-runtime scheduler transition for paused and runnable threads. r[chaoscontrol.thread_pause.safepoints] r[chaoscontrol.thread_pause.release]
- [ ] [serial] Implement exact-tick release and bounded deterministic virtual-time advancement when no thread is runnable. r[chaoscontrol.thread_pause.release]
- [ ] [serial] Bind pause state into snapshots and exact replay validation. r[chaoscontrol.thread_pause.state]

## Phase 4: Evidence and fixtures

- [ ] [parallel] Extend the fault stage ledger and receipt with thread targets, observed safe points, releases, and paused-tick counts. r[chaoscontrol.thread_pause.evidence]
- [ ] [parallel] Add positive fixtures for deterministic interleaving, exact release, snapshot continuation, and replay parity. r[chaoscontrol.thread_pause.fixtures]
- [ ] [parallel] Add negative fixtures for unsupported runtime, safe-point starvation, pause overlap, identity drift, snapshot mismatch, replay divergence, and overclaim. r[chaoscontrol.thread_pause.fixtures] r[chaoscontrol.thread_pause.nonclaims]
- [ ] [serial] Exercise the bounded race fixture under a real KVM run and compare the paused and control continuations. r[chaoscontrol.thread_pause.release] r[chaoscontrol.thread_pause.state]

## Phase 5: Verification and evidence

- [ ] [serial] Run focused fault, SDK, VMM, sim-core, replay, and evidence tests; Clippy with warnings denied; octet; Cairn validation and gates; and relevant Nix checks. r[chaoscontrol.thread_pause.fixtures]
- [ ] [serial] Record the bounded KVM receipt, runtime ABI and artifact identities, positive and negative results, and remaining non-claims. r[chaoscontrol.thread_pause.evidence] r[chaoscontrol.thread_pause.nonclaims]
