## Phase 0: Prerequisites and shared seam

- [ ] [serial] Complete the unified AGPL license boundary before publication and adoption. [depends:adopt-unified-agpl-license]
- [ ] [serial] Complete deterministic SMP progress and remove wall-clock schedule input before scheduler extraction. [depends:remove-wall-clock-smp-preemption]
- [ ] [serial] Complete explicit fault application outcomes before generic scheduled-event adapters. [depends:verify-fault-application-outcomes]
- [ ] [serial] Complete full VM snapshot state before shared snapshot adapters replace local state. [depends:complete-vm-snapshot-state]
- [ ] [serial] Compare ChaosControl and Aspen clock, entropy, scheduler, event, choice, and snapshot semantics. r[shared.deterministic_sim.aspen_boundary]
- [ ] [serial] Stop repository creation if the comparison finds no stable product-neutral subset. r[shared.deterministic_sim.aspen_boundary]

## Phase 1: Shared core repository

- [ ] [serial] Establish the `deterministic-sim` AGPL repository and immutable publication workflow. r[shared.deterministic_sim.repository]
- [ ] [parallel] Implement a versioned checked virtual clock with explicit tick policy and overflow failure. r[shared.deterministic_sim.clock]
- [ ] [parallel] Implement versioned ChaCha20 domain-separated streams with exact snapshot and resume state. r[shared.deterministic_sim.entropy]
- [ ] [serial] Implement a pure scheduler over ordered runnable identities, supplied progress facts, and seeded choice state. r[shared.deterministic_sim.scheduler]
- [ ] [parallel] Implement generic scheduled-event ordering and recorded-choice override state machines. r[shared.deterministic_sim.events] r[shared.deterministic_sim.choices]
- [ ] [serial] Implement versioned complete core snapshots and pure compatibility preflight. r[shared.deterministic_sim.snapshot]

## Phase 2: Consumer adapters

- [ ] [parallel] Add a ChaosControl entropy adapter with exact-byte and snapshot-resume parity fixtures. r[shared.deterministic_sim.chaoscontrol_boundary]
- [ ] [serial] Add a ChaosControl scheduler adapter only after exact guest progress supplies all required facts. r[shared.deterministic_sim.chaoscontrol_boundary]
- [ ] [parallel] Adapt fault schedule ordering and input-tree choices without moving fault or workload meaning into the shared repository. r[shared.deterministic_sim.events] r[shared.deterministic_sim.choices]
- [ ] [serial] Replace only the reusable in-process mechanisms and remove the xorshift and saturating-clock duplicates. r[shared.deterministic_sim.migration]
- [ ] [parallel] Add the agreed Aspen adapter in Aspen with its product policy unchanged. r[shared.deterministic_sim.aspen_boundary]

## Phase 3: Compatibility and checks

- [ ] [serial] Publish an algorithm and snapshot compatibility table for shared and consumer versions. r[shared.deterministic_sim.compatibility]
- [ ] [parallel] Add positive repeated-run, snapshot-resume, stream-separation, event-order, and override-replay checks. r[shared.deterministic_sim.validation]
- [ ] [parallel] Add negative overflow, empty runnable set, stale progress, invalid override, stream mismatch, incomplete snapshot, unsupported version, and budget checks. r[shared.deterministic_sim.validation]
- [ ] [serial] Run shared checks, focused ChaosControl and Aspen checks, workspace checks, dependency policy, replay comparisons, and Cairn gates before sync or archive. r[shared.deterministic_sim.validation]
