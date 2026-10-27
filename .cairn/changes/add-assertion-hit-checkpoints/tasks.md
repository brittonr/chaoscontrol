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
- [ ] [serial] Persist assertion provenance in the Campaign journal so resumed entries retain stale-parent validation. r[chaoscontrol.assertion_checkpoints.frontier]
- [ ] [serial] Connect branch scheduling and reporting to checkpoint provenance without changing assertion or finding semantics. r[chaoscontrol.assertion_checkpoints.nonclaims]

## Phase 4: Evidence and adversarial fixtures

- [ ] [parallel] Emit the bounded checkpoint receipt with selected, captured, skipped, duplicate, failed, and frontier outcomes. r[chaoscontrol.assertion_checkpoints.evidence]
- [ ] [parallel] Add negative fixtures for unbound assertions, identity drift, stale parents, storage failure, missing snapshots, and overclaim. r[chaoscontrol.assertion_checkpoints.fixtures] r[chaoscontrol.assertion_checkpoints.nonclaims]
- [ ] [serial] Exercise a real KVM workload where a rare assertion hit is selected, captured, restored, and used as a branch parent. r[chaoscontrol.assertion_checkpoints.observation] r[chaoscontrol.assertion_checkpoints.frontier]

## Phase 5: Verification and evidence

- [ ] [serial] Run focused explorer, VMM, SDK, replay, and evidence tests; Clippy with warnings denied; octet; Cairn validation and gates; and relevant Nix checks. r[chaoscontrol.assertion_checkpoints.fixtures]
- [ ] [serial] Record the bounded KVM receipt, exact source and artifact identities, positive and negative results, and remaining non-claims. r[chaoscontrol.assertion_checkpoints.evidence] r[chaoscontrol.assertion_checkpoints.nonclaims]
