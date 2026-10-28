# Tasks

## Phase 1: Baseline and boundary design

- [x] [serial] Record current assertion-coverage enrichment, frontier proposal, snapshot, and marker-restore behavior before source changes. r[chaoscontrol.assertion_checkpoints.observation] r[chaoscontrol.assertion_checkpoints.frontier]
- [x] [serial] Confirm the assertion hypercall/port-I/O exit is fully handled before the next guest instruction and record the intended capture boundary. r[chaoscontrol.assertion_checkpoints.observation]
- [ ] [serial] Publish the hit-observation and capture-boundary facts, including the exact cases that cannot be captured. r[chaoscontrol.assertion_checkpoints.observation] r[chaoscontrol.assertion_checkpoints.nonclaims]

## Phase 2: Pure policy and observation

- [x] [serial] Implement the bounded assertion-hit observation record and admission against the accepted catalog and oracle report. r[chaoscontrol.assertion_checkpoints.observation]
- [x] [serial] Implement the pure seeded selection policy with named limits, novelty and rarity scoring, duplicate suppression, and stable reasons. r[chaoscontrol.assertion_checkpoints.policy]
- [x] [parallel] Add pure policy fixtures for deterministic rerun, duplicate hits, exhausted budgets, and skip reasons. r[chaoscontrol.assertion_checkpoints.policy] r[chaoscontrol.assertion_checkpoints.fixtures]

## Phase 3: Capture and frontier integration

- [x] [serial] Capture the selected boundary state using the existing snapshot store and bind the exact assertion and execution facts. r[chaoscontrol.assertion_checkpoints.capture]
- [x] [serial] Add captured checkpoints to the frontier as deduplicated branch proposals with an assertion provenance marker. r[chaoscontrol.assertion_checkpoints.frontier]
- [x] [serial] Persist the checkpoint selector hit counts and campaign capture budget across resume. r[chaoscontrol.assertion_checkpoints.policy]
- [x] [serial] Bind full assertion provenance and reject stale or mismatched checkpoint parents on restore. r[chaoscontrol.assertion_checkpoints.frontier]
- [x] [serial] Connect branch scheduling and reporting to checkpoint provenance without changing assertion or finding semantics. r[chaoscontrol.assertion_checkpoints.nonclaims]

## Phase 4: Evidence and adversarial fixtures

- [x] [parallel] Emit the bounded checkpoint receipt with selected, captured, skipped, duplicate, failed, and frontier outcomes. r[chaoscontrol.assertion_checkpoints.evidence]
- [x] [parallel] Add negative fixtures for unbound assertions, identity drift, stale parents, storage failure, missing snapshots, and overclaim. r[chaoscontrol.assertion_checkpoints.fixtures] r[chaoscontrol.assertion_checkpoints.nonclaims]
- [x] [serial] Exercise a real KVM workload where a rare assertion hit is selected, captured, and admitted to the Campaign frontier. r[chaoscontrol.assertion_checkpoints.observation] r[chaoscontrol.assertion_checkpoints.frontier]
- [x] [serial] Exercise a restored assertion-checkpoint parent in a follow-up exploration round. r[chaoscontrol.assertion_checkpoints.frontier]

## Phase 5: Verification and evidence

- [x] [serial] Run focused explorer, VMM, fault, and replay tests; Clippy with warnings denied; Cairn validation and gates; the product-scope check; and a bounded KVM smoke. r[chaoscontrol.assertion_checkpoints.fixtures]
- [x] [serial] Run the complete Nix gate; Octet currently reports pre-existing `chaoscontrol-smr` findings outside the changed files. r[chaoscontrol.assertion_checkpoints.fixtures]
- [ ] [serial] Record a formal admitted-cohort KVM receipt with exact source and artifact identities, positive and negative results, and remaining non-claims. r[chaoscontrol.assertion_checkpoints.evidence] r[chaoscontrol.assertion_checkpoints.nonclaims]
