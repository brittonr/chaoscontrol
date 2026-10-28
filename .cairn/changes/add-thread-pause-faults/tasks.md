# Tasks

## Phase 1: Baseline and runtime boundary

- [x] [serial] Record current process, vCPU, and scheduler pause behavior and the existing fault stage ledger before source changes. r[chaoscontrol.thread_pause.fault] r[chaoscontrol.thread_pause.evidence]
- [x] [serial] Inventory the guest runtime and SDK surfaces that can own a stable thread identity and cooperative safe point. r[chaoscontrol.thread_pause.identity] r[chaoscontrol.thread_pause.safepoints]
- [x] [serial] Publish the versioned safe-point ABI schema, version, safe-point bound, and unsupported-cohort rejection. r[chaoscontrol.thread_pause.safepoints] r[chaoscontrol.thread_pause.nonclaims]

## Phase 2: Identity, fault, and planner

- [x] [serial] Define stable logical thread identity and pure identity admission. r[chaoscontrol.thread_pause.identity]
- [x] [serial] Implement runtime identity assignment, generation tracking, and snapshot-safe lookup. r[chaoscontrol.thread_pause.identity]
- [x] [serial] Implement pure thread-pause plan admission for target, exact window, duplicate active threads, and named bounds. r[chaoscontrol.thread_pause.fault]
- [x] [serial] Add the typed thread-pause fault variant to the engine catalogue and planner. r[chaoscontrol.thread_pause.fault]
- [x] [parallel] Add negative planner fixtures for unsupported ABI, invalid identity, duplicate active thread, over-long window, and total-pause bound. r[chaoscontrol.thread_pause.fault] r[chaoscontrol.thread_pause.fixtures]

## Phase 3: Safe-point gate and deterministic release

- [x] [serial] Implement the cooperative safe-point gate and guest-runtime scheduler transition for paused and runnable threads. r[chaoscontrol.thread_pause.safepoints] r[chaoscontrol.thread_pause.release]
- [x] [serial] Define exact-tick release and paused-at-tick semantics in the pure core. r[chaoscontrol.thread_pause.release]
- [x] [serial] Implement bounded deterministic virtual-time advancement when no thread is runnable. r[chaoscontrol.thread_pause.release]
- [x] [serial] Bind pause state into snapshots and exact replay validation. r[chaoscontrol.thread_pause.state]

## Phase 4: Evidence and fixtures

- [ ] [parallel] Extend the fault stage ledger and receipt with thread targets, observed safe points, releases, and paused-tick counts. r[chaoscontrol.thread_pause.evidence]
- [x] [parallel] Add positive fixtures for deterministic interleaving, exact release, snapshot continuation, and replay parity. r[chaoscontrol.thread_pause.fixtures]
- [ ] [parallel] Add negative fixtures for unsupported runtime, safe-point starvation, pause overlap, identity drift, snapshot mismatch, replay divergence, and overclaim. r[chaoscontrol.thread_pause.fixtures] r[chaoscontrol.thread_pause.nonclaims]
- [x] [serial] Exercise the bounded race fixture under a real KVM run and compare the paused and control continuations. r[chaoscontrol.thread_pause.release] r[chaoscontrol.thread_pause.state]

## Phase 5: Verification and evidence

- [ ] [serial] Run focused fault, SDK, VMM, sim-core, replay, and evidence tests; Clippy with warnings denied; octet; Cairn validation and gates; and relevant Nix checks. r[chaoscontrol.thread_pause.fixtures]
- [ ] [serial] Record the bounded KVM receipt, runtime ABI and artifact identities, positive and negative results, and remaining non-claims. r[chaoscontrol.thread_pause.evidence] r[chaoscontrol.thread_pause.nonclaims]

## Blocked prerequisite

The guest-side cooperative runtime is absent. The repository has only the pure
admission module in `crates/chaoscontrol-fault/src/thread_pause.rs`; no SDK
runtime assigns logical thread identities, exposes the safe-point ABI, schedules
paused and runnable tasks, or maps them through snapshots. Implementing a VMM
vCPU stall would not satisfy the cooperative guest-thread requirement. The KVM
race proof cannot be completed until that runtime and its guest artifact exist.
