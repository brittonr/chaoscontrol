# Tasks: Extract the replay evidence core

## Phase 1: Baseline and authority inventory

- [ ] [serial] Record current replay verdict bytes, enum spellings, optional fields, artifact algorithms, explorer call sites, evidence-reader call sites, and focused test results. r[chaoscontrol.replay_evidence.compatibility]
- [ ] [serial] Inventory pure DTO, validation, and classification ownership separately from filesystem, clock, VM, checkpoint, snapshot, process, logging, and rendering effects. r[chaoscontrol.replay_evidence.core_boundary]
- [ ] [parallel] Define the compatibility adapter map for every duplicated or re-exported replay type. r[chaoscontrol.replay_evidence.shared_authority]

## Phase 2: Shared core

- [ ] [serial] Add `chaoscontrol-replay-evidence-core` with replay verdict, artifact hash, snapshot reference, parent reference, replay class, validation status, and bounded diagnostic DTOs. r[chaoscontrol.replay_evidence.shared_authority]
- [ ] [serial] Move pure replay and snapshot classification into the shared core without filesystem, clock, process, network, KVM, async-runtime, logging, or Nickel runtime authority. r[chaoscontrol.replay_evidence.core_boundary]
- [ ] [serial] Keep `write_verdict`, `new_run_id`, artifact reads, snapshot and checkpoint access, VM execution, and report publication in shell crates. r[chaoscontrol.replay_evidence.shell_boundary]

## Phase 3: Compatibility and tests

- [ ] [parallel] Add positive fixtures for snapshot-backed reproduction, schedule-only gaps, valid no-bug results, accepted evidence receipts, and stable serialization. r[chaoscontrol.replay_evidence.compatibility]
- [ ] [parallel] Add negative fixtures for malformed or wrong-algorithm hashes, missing or inconsistent snapshot references, unsafe artifact references, stale hashes, unsupported classes, contradictory exits, legacy identities, and overclaim wording. r[chaoscontrol.replay_evidence.compatibility]
- [ ] [serial] Migrate explorer and evidence call sites through compatibility adapters without changing accepted public JSON bytes. r[chaoscontrol.replay_evidence.shared_authority] r[chaoscontrol.replay_evidence.compatibility]

## Phase 4: Enforcement and closeout

- [ ] [serial] Add dependency and source guards plus positive shell fixtures and negative forbidden-core-import fixtures. r[chaoscontrol.replay_evidence.architecture_guard]
- [ ] [serial] Document Rust and Nickel ownership, SHA-256 interoperability fields, BLAKE3 defaults, shell effects, and replay non-claims. r[chaoscontrol.replay_evidence.claim_boundary]
- [ ] [serial] Run focused core, explorer, and evidence tests, Cargo formatting, focused Clippy, Cairn validation, proposal/design/tasks gates, and relevant Nix checks. Record exact results before sync and archive. r[chaoscontrol.replay_evidence.architecture_guard] r[chaoscontrol.replay_evidence.compatibility]
