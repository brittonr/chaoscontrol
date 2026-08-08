# Verification: extract-replay-evidence-core

## What landed

New crate `crates/chaoscontrol-replay-evidence-core`: the single Rust-owned
authority for replay/evidence DTOs and fail-closed validation decisions.

- `dto`: `ReplayVerdict`, `ReplayClass`, `SnapshotValidationStatus`,
  `ArtifactHash`, `ReplayParentSnapshotRef`, `ReplayCommandContext`,
  `ReplaySnapshotValidation`, schema/exit-status constants. Serde shapes are
  byte-compatible with the previous explorer definitions.
- `validate`: digest shape, snapshot ref shape/current checks, replay class
  and snapshot status parse/list, verdict consistency for every class,
  accepted-proof gate, stale-hash detection, public-path confinement.
- `classify`: pure classification kernel; the explorer shell supplies
  observed facts.
- `claims`: assertion anti-claim / overclaim fragment lists and pure checks.
- Purity: no `std::fs`, `std::process`, clock, env, or KVM use in the crate
  (grep-verified). Dependencies: `serde`, `chaoscontrol-protocol` only.

## Compatibility adapters

- `chaoscontrol-explore::replay_verdict` re-exports the core DTOs and keeps
  shell effects: `verdict_from_reproduce`, `snapshot_validation_from_error`,
  `write_verdict`, `hash_bytes`, `new_run_id`.
- `chaoscontrol-explore::snapshot_store` re-exports
  `ReplayParentSnapshotRef` and maps core admissibility constants onto the
  existing `SnapshotStoreError` variants.
- `chaoscontrol-evidence` re-exports core constants, `ArtifactHash`, and
  `SnapshotRef` (alias of `ReplayParentSnapshotRef`); its stricter
  accepted-proof `ReplayVerdict` view stays as a documented adapter.
- `evidence_contracts` delegates class lists, statuses, codec/schema
  admissibility, and digest shape to the core, and runs typed core
  consistency plus accepted-proof validation for current-schema verdicts.
- Public path confinement is a distinct gate
  (`validate_public_verdict_paths`) wired into `validate_replay_verdict`;
  local replay tooling with absolute paths is unaffected.

Public JSON field names are unchanged; a round-trip fixture test proves the
emitted verdict JSON keeps every public field.

## Evidence

- Baseline before change: `cargo test -p chaoscontrol-explore
  -p chaoscontrol-evidence --lib` green (195 explore lib tests).
- After change: `cargo test -p chaoscontrol-replay-evidence-core
  -p chaoscontrol-explore -p chaoscontrol-evidence` — 57 suites, 404 passed,
  0 failed. Core adds 18 tests (positive fixtures: snapshot-backed
  reproduced, schedule-only gaps, zero-depth valid snapshot, no-bug,
  missing-ref; negative fixtures: malformed hash, missing snapshot ref,
  invalid digest, path escape, absolute public path, unsupported class,
  stale hash, non-reproducing accepted claim, legacy schema promotion,
  forged exit status, null identity carrier, overclaim wording, stale
  codec).
- `cargo clippy -p chaoscontrol-replay-evidence-core -p chaoscontrol-explore
  -p chaoscontrol-evidence --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean for the three crates.
- `check-evidence-contracts`: "evidence contracts ok: Nickel profiles,
  projection freshness, receipts, positive fixtures, negative fixtures".
- `check-replay-proof-coverage` and `check-readiness-promotion-gate` run
  green with the expected current `blocked-assertion-identity` statuses.

## Behavior notes

- Digest validation is now strict lowercase hex everywhere (explorer always
  emits lowercase); one evidence test message expectation updated to the
  core diagnostic wording.
- Exit-status consistency requires exactly 0 (reproduced) or 1 (not
  reproduced), matching what the explorer emits.
- Legacy schema v1 verdicts remain readable through the existing JSON
  contract path; core typed validation applies to current-schema verdicts.
