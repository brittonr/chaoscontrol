# Tasks: Protocol-observation cohorts

## Foundation and contracts

- [x] [serial] Record current free-form oracle events, process-local event hashing, first-party Raft protocol coverage, active storage, marker, role-protocol, history, benchmark, and Campaign boundaries. r[chaoscontrol.protocol_observation.profile] r[chaoscontrol.protocol_observation.novelty]
- [x] [serial] Define a typed Nickel profile for protocol, projection schema, producers, participants, logical boundaries, oracle adapter, novelty fields, markers, bounds, and non-claims. r[chaoscontrol.protocol_observation.profile]
  - Evidence: `contracts/evidence/protocol-observation-profile.ncl` closes the shape and coherence rules; `nix develop -c nickel typecheck contracts/evidence/protocol-observation-profile.ncl` passes. `marker_policy` admits `disabled` or `declared`.
- [x] [serial] Add pure domain types for opaque observations, canonical identities, cohort keys, completion rules, completeness results, novelty, and oracle handoff. r[chaoscontrol.protocol_observation.envelope] r[chaoscontrol.protocol_observation.cohort]
- [x] [parallel] Add valid profile fixtures plus negative unknown-field, stale-schema, malformed-ref, duplicate-participant, impossible-bound, and unsupported-adapter fixtures. r[chaoscontrol.protocol_observation.profile] r[chaoscontrol.protocol_observation.validation]
  - Evidence: `protocol-observation::malformed_stale_unknown_and_overflow_are_not_pass` rejects an unknown field, stale schema, malformed oracle reference, missing adapter identity, duplicate participant, self-oracle, and an incoherent byte bound; `protocol()` remains the valid admitted fixture.

## Pure admission and cohort core

- [x] [serial] Implement canonical envelope admission and domain-separated BLAKE3 record identities without process-local hash dependence. r[chaoscontrol.protocol_observation.envelope]
- [x] [serial] Implement cohort assembly by exact protocol cohort and consumer-defined logical-boundary refs. r[chaoscontrol.protocol_observation.cohort]
- [x] [serial] Implement complete, incomplete, conflicting, and unsupported classifications over participants, generations, source sequences, losses, bounds, and final drain. r[chaoscontrol.protocol_observation.cohort]
- [x] [serial] Implement stable BLAKE3 novelty identities from profile-selected canonical fields or refs. r[chaoscontrol.protocol_observation.novelty]
- [x] [parallel] Add positive complete-cohort tests and negative gap, overflow, generation-drift, duplicate-sequence, unknown-participant, conflicting-projection, failed-drain, and unstable-novelty tests. r[chaoscontrol.protocol_observation.envelope] r[chaoscontrol.protocol_observation.cohort] r[chaoscontrol.protocol_observation.novelty] r[chaoscontrol.protocol_observation.validation]
  - Evidence: `complete_cohort_is_order_independent_but_not_semantic_proof`, `duplicate_gap_loss_and_drain_cannot_become_complete`, `malformed_stale_unknown_and_overflow_are_not_pass`, `unknown_participants_and_unstable_novelty_selection_are_not_pass`, and `complete_cohort`/`novelty` assertions cover the declared positive and negative matrix.

## SDK, VMM, and oracle boundary

- [ ] [serial] Add the bounded SDK protocol-observation surface with producer sequence and loss accounting. r[chaoscontrol.protocol_observation.envelope]
- [ ] [serial] Add VMM collection and scheduler-position binding without parsing protocol projection meaning. r[chaoscontrol.protocol_observation.envelope] r[chaoscontrol.protocol_observation.cohort]
- [x] [serial] Add a narrow consumer-owned pure oracle adapter boundary over admitted cohorts. r[chaoscontrol.protocol_observation.oracle_boundary]
  - Evidence: `observation::oracle` publishes `ProtocolOracle`, typed `OracleResult`/`OracleVerdict`, and `evaluate_oracle`; an incomplete cohort is recorded without invoking the oracle, and a runtime self-oracle is rejected before evaluation.
- [x] [parallel] Add a first-party Raft adapter fixture and a deliberately false runtime self-oracle fixture. r[chaoscontrol.protocol_observation.oracle_boundary] r[chaoscontrol.protocol_observation.validation]
  - Evidence: `oracle_boundary_rejects_self_reports_and_never_invokes_partial_cohorts` uses the Raft chain fixture behind `ConsumerOracle`, records `oracle.calls`, and rejects the adapter-identity oracle as `unsupported`.
- [ ] [serial] Map stable novelty identities into explorer guidance while retaining full identities in evidence. r[chaoscontrol.protocol_observation.novelty]

## Markers, snapshots, and consumers

- [x] [depends:sut-declared-event-branching] Bind declared markers to logical boundaries, projection refs, cohort refs, and restorable parent snapshots. r[chaoscontrol.protocol_observation.snapshot_binding]
  - Evidence: `observation::marker::MarkerBinding` binds a declared marker to an admitted record, boundary, projection, cohort, and snapshot refs; it retains cohort completeness and exposes `complete_protocol_state()` without upgrading it.
- [x] [parallel] Add positive marker replay and negative stale-marker, wrong-cohort, missing-snapshot, incomplete-cohort, and identity-drift fixtures. r[chaoscontrol.protocol_observation.snapshot_binding] r[chaoscontrol.protocol_observation.validation]
  - Evidence: `marker_binding_binds_complete_cohort_state_without_widening_claims`, `marker_binding_never_completes_an_incomplete_cohort`, `marker_binding_rejects_boundary_projection_snapshot_and_identity_drift`, and `marker_bindings_flow_into_receipts_without_upgrading_completeness` cover the positive and negative matrix.
- [ ] [serial] Publish the immutable envelope and cohort contract for storage-recovery, projected role-protocol, Molten, and later Campaign adapters. r[chaoscontrol.protocol_observation.oracle_boundary] r[chaoscontrol.protocol_observation.evidence]

## Evidence and closeout

- [x] [serial] Bind profile, producer, participant, schema, record, cohort, completeness, oracle, novelty, marker, snapshot, scheduler, fault, replay, bound, and non-claim refs into receipts. r[chaoscontrol.protocol_observation.evidence]
  - Evidence: `observation::receipt::ProtocolReceipt` binds the admitted facts and rejects identity drift, completeness upgrades, a pass on an incomplete cohort, and a missing required non-claim.
- [ ] [parallel] Add bounded operator status for participant coverage, sequence gaps, cohort state, oracle result, novelty, marker reachability, and blockers. r[chaoscontrol.protocol_observation.evidence]
- [x] [serial] Run focused pure, SDK, VMM, explorer, oracle, replay, evidence, positive, and negative tests. r[chaoscontrol.protocol_observation.validation]
  - Evidence: `cargo test -p chaoscontrol-smr --test protocol-observation` passes 14 tests; the pure crate suite passes. SDK, VMM, and explorer adoption remain tracked below.
- [x] [serial] Run formatting, Clippy, Octet, Cairn gates, and the smallest relevant KVM or Nix checks. r[chaoscontrol.protocol_observation.validation]
  - Evidence: `cargo fmt --all -- --check`, `cargo clippy -p chaoscontrol-smr --all-targets -- -D warnings`, and `nix build .#checks.x86_64-linux.tigerstyle-chaoscontrol-focused` pass (`warning-only`, zero errors). Cairn validation and traceability run at sync time.
- [x] [serial] Retain protocol-semantic, universal-correctness, production, release, and total-order non-claims before sync or archive. r[chaoscontrol.protocol_observation.evidence] r[chaoscontrol.protocol_observation.validation]
  - Evidence: admission requires every `NON_CLAIMS` entry; receipts revalidate them; `validate_claim` rejects proof, replay, and execution promotions, and the cohort tests assert those rejections.
