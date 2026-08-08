## Phase 1: Profile and semantic core

- [x] [serial] Define a typed Nickel SMR workload profile with initial-state identity, observation mode, and finite command, client, concurrency, progress, trace, fault, and evidence bounds. r[chaoscontrol.smr_chain.profile]
- [x] [serial] Add Rust profile projection types and fail-closed revalidation for external JSON. r[chaoscontrol.smr_chain.profile] r[chaoscontrol.smr_chain.boundary]
- [x] [serial] Implement pure domain-separated BLAKE3 genesis and transition functions with explicit canonical framing. r[chaoscontrol.smr_chain.transition]
- [x] [serial] Implement pure observation normalization, chain-link validation, safety evaluation, and conditional liveness evaluation. r[chaoscontrol.smr_chain.history] r[chaoscontrol.smr_chain.safety] r[chaoscontrol.smr_chain.liveness]
- [x] [serial] Add assertions for deterministic hashes, malformed framing, rollback, lossless gaps, sampled gaps, duplicate observations, divergence, lag, and stalled recovery. r[chaoscontrol.smr_chain.validation]

## Phase 2: Workload and guest adapters

- [x] [serial] Define a bounded Rust consumer adapter for proposals, committed-transition observations, lifecycle state, and terminal status without consensus-internal APIs. r[chaoscontrol.smr_chain.adapter]
- [x] [serial] Preserve stable operation identity across acknowledged, definitely rejected, indefinite, and retried proposals. r[chaoscontrol.smr_chain.indefinite_outcomes]
- [x] [parallel] Integrate the adapter with the first-party Raft guest while keeping existing protocol assertions and replay classes separate. r[chaoscontrol.smr_chain.adapter] r[chaoscontrol.smr_chain.boundary]
- [x] [parallel] Add a deliberately faulty fixture that diverges, reapplies, rolls back, or stalls under selected schedules. r[chaoscontrol.smr_chain.validation]

## Phase 3: Campaign exploration and replay

- [x] [depends:fault-application-outcomes] Add no-fault controls and bounded network, process, storage, scheduler, and clock profiles that require applicable, applied, and observed fault outcomes. r[chaoscontrol.smr_chain.fault_campaign]
- [x] [parallel] Add deterministic swarm feature subsets, command generation, client concurrency, and coverage summaries with exact seed and choice records. r[chaoscontrol.smr_chain.fault_campaign]
- [x] [serial] Add replay comparison for command identities, proposal outcomes, observations, safety prefixes, liveness preconditions, and terminal verdicts. r[chaoscontrol.smr_chain.replay]
- [x] [parallel] Add causal reduction for commands, clients, fault actions, and schedules while preserving the failure class and valid profile preconditions. r[chaoscontrol.smr_chain.replay]

## Phase 4: Evidence and consumer boundary

- [x] [serial] Emit bounded Rust-owned evidence and a compact receipt for profile, build, seed, schedule, faults, observation completeness, bounds, verdicts, replay, and non-claims. r[chaoscontrol.smr_chain.evidence]
- [x] [serial] Add fail-closed evidence validation for malformed histories, missing build or profile refs, selected-only faults, absent liveness preconditions, replay mismatch, and claim promotion. r[chaoscontrol.smr_chain.evidence]
- [x] [parallel] Document the immutable consumer handoff, observer trust boundary, operation-outcome semantics, safety and liveness split, and external evidence role. r[chaoscontrol.smr_chain.boundary]

## Phase 5: Validation

- [x] [serial] Run pure-core and property tests, positive and negative adapter fixtures, no-fault and fault campaigns, replay, reduction, receipt, and bounded KVM smoke tests. r[chaoscontrol.smr_chain.validation]
- [x] [serial] Run Cairn validation and proposal, design, and tasks gates before sync or archive. r[chaoscontrol.smr_chain.validation]
