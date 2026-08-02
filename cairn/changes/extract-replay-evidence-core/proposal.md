# Extract the replay evidence core

## Why

Replay evidence DTOs and decisions are split across `chaoscontrol-explore` and `chaoscontrol-evidence`. Replay verdict construction, artifact hashes, snapshot references, replay classes, validation status, filesystem publication, and wall-clock run identity also share source files. This allows evidence readers and writers to drift and prevents Cargo from enforcing the replay decision boundary.

An existing legacy OpenSpec draft identified the same shared-core need. This native Cairn package preserves that intent and becomes the lifecycle source for implementation.

## What Changes

- Add a `chaoscontrol-replay-evidence-core` crate as the single Rust authority for replay verdict DTOs and pure validation and classification.
- Move `ReplayVerdict`, artifact hash, snapshot reference, replay parent reference, replay class, validation status, and compatibility decisions into the shared core or re-export them from it.
- Keep verdict file reads and writes, run-ID creation, checkpoint and snapshot access, VM execution, process work, clocks, Nickel evaluation, logging, and report publication in shell crates.
- Preserve current public JSON fields and accepted replay classes through explicit compatibility adapters.
- Add positive and negative fixtures shared by explorer emission and evidence-readiness validation.
- Add dependency and source guards for the core crate.

## Capability Delta

A new `replay-evidence` spec domain defines shared DTO authority, pure classification, shell ownership, compatibility, and bounded evidence claims.

## Impact

- **Core source**: pure parts of `crates/chaoscontrol-explore/src/replay_verdict.rs` and duplicated replay validation in `chaoscontrol-evidence`.
- **Shell source**: `write_verdict`, `new_run_id`, artifact and snapshot reads, checkpoint access, VM execution, and readiness report I/O.
- **Wire compatibility**: current JSON field names, enum spellings, required SHA-256 artifact fields, and accepted diagnostic classes remain stable.
- **Dependencies**: no new runtime service is required. The core cannot depend on KVM, filesystem, process, clock, network, logging, or Nickel runtime authority.

## Non-Goals

- Making Nickel the owner of runtime replay traces or verdict records.
- Rewriting the full campaign or explorer loop in this change.
- Changing replay exit classes, snapshot requirements, or evidence-readiness policy.
- Replacing SHA-256 fields required by the accepted replay artifact format. New ChaosControl-owned identities still default to BLAKE3.
- Claiming VM correctness, deterministic replay, snapshot correctness, or release readiness from DTO and source-boundary checks.

## Verification Expectations

Implementation must record current verdict bytes and focused tests before moves. It must run core tests, explorer and evidence compatibility tests, malformed and stale artifact negatives, forbidden-dependency fixtures, Cairn validation and gates, Cargo formatting, focused Clippy, and the relevant Nix checks.
