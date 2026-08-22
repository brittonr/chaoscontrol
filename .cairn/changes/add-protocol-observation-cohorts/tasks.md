# Tasks: Protocol-observation cohorts

## Foundation and contracts

- [x] [serial] Record current free-form oracle events, process-local event hashing, first-party Raft protocol coverage, active storage, marker, role-protocol, history, benchmark, and Campaign boundaries. r[chaoscontrol.protocol_observation.profile] r[chaoscontrol.protocol_observation.novelty]
- [ ] [serial] Define a typed Nickel profile for protocol, projection schema, producers, participants, logical boundaries, oracle adapter, novelty fields, markers, bounds, and non-claims. r[chaoscontrol.protocol_observation.profile]
- [ ] [serial] Add pure domain types for opaque observations, canonical identities, cohort keys, completion rules, completeness results, novelty, and oracle handoff. r[chaoscontrol.protocol_observation.envelope] r[chaoscontrol.protocol_observation.cohort]
- [ ] [parallel] Add valid profile fixtures plus negative unknown-field, stale-schema, malformed-ref, duplicate-participant, impossible-bound, and unsupported-adapter fixtures. r[chaoscontrol.protocol_observation.profile] r[chaoscontrol.protocol_observation.validation]

## Pure admission and cohort core

- [ ] [serial] Implement canonical envelope admission and domain-separated BLAKE3 record identities without process-local hash dependence. r[chaoscontrol.protocol_observation.envelope]
- [ ] [serial] Implement cohort assembly by exact protocol cohort and consumer-defined logical-boundary refs. r[chaoscontrol.protocol_observation.cohort]
- [ ] [serial] Implement complete, incomplete, conflicting, and unsupported classifications over participants, generations, source sequences, losses, bounds, and final drain. r[chaoscontrol.protocol_observation.cohort]
- [ ] [serial] Implement stable BLAKE3 novelty identities from profile-selected canonical fields or refs. r[chaoscontrol.protocol_observation.novelty]
- [ ] [parallel] Add positive complete-cohort tests and negative gap, overflow, generation-drift, duplicate-sequence, unknown-participant, conflicting-projection, failed-drain, and unstable-novelty tests. r[chaoscontrol.protocol_observation.envelope] r[chaoscontrol.protocol_observation.cohort] r[chaoscontrol.protocol_observation.novelty] r[chaoscontrol.protocol_observation.validation]

## SDK, VMM, and oracle boundary

- [ ] [serial] Add the bounded SDK protocol-observation surface with producer sequence and loss accounting. r[chaoscontrol.protocol_observation.envelope]
- [ ] [serial] Add VMM collection and scheduler-position binding without parsing protocol projection meaning. r[chaoscontrol.protocol_observation.envelope] r[chaoscontrol.protocol_observation.cohort]
- [ ] [serial] Add a narrow consumer-owned pure oracle adapter boundary over admitted cohorts. r[chaoscontrol.protocol_observation.oracle_boundary]
- [ ] [parallel] Add a first-party Raft adapter fixture and a deliberately false runtime self-oracle fixture. r[chaoscontrol.protocol_observation.oracle_boundary] r[chaoscontrol.protocol_observation.validation]
- [ ] [serial] Map stable novelty identities into explorer guidance while retaining full identities in evidence. r[chaoscontrol.protocol_observation.novelty]

## Markers, snapshots, and consumers

- [ ] [depends:sut-declared-event-branching] Bind declared markers to logical boundaries, projection refs, cohort refs, and restorable parent snapshots. r[chaoscontrol.protocol_observation.snapshot_binding]
- [ ] [parallel] Add positive marker replay and negative stale-marker, wrong-cohort, missing-snapshot, incomplete-cohort, and identity-drift fixtures. r[chaoscontrol.protocol_observation.snapshot_binding] r[chaoscontrol.protocol_observation.validation]
- [ ] [serial] Publish the immutable envelope and cohort contract for storage-recovery, projected role-protocol, Molten, and later Campaign adapters. r[chaoscontrol.protocol_observation.oracle_boundary] r[chaoscontrol.protocol_observation.evidence]

## Evidence and closeout

- [ ] [serial] Bind profile, producer, participant, schema, record, cohort, completeness, oracle, novelty, marker, snapshot, scheduler, fault, replay, bound, and non-claim refs into receipts. r[chaoscontrol.protocol_observation.evidence]
- [ ] [parallel] Add bounded operator status for participant coverage, sequence gaps, cohort state, oracle result, novelty, marker reachability, and blockers. r[chaoscontrol.protocol_observation.evidence]
- [ ] [serial] Run focused pure, SDK, VMM, explorer, oracle, replay, evidence, positive, and negative tests. r[chaoscontrol.protocol_observation.validation]
- [ ] [serial] Run formatting, Clippy, Octet, Cairn gates, and the smallest relevant KVM or Nix checks. r[chaoscontrol.protocol_observation.validation]
- [ ] [serial] Retain protocol-semantic, universal-correctness, production, release, and total-order non-claims before sync or archive. r[chaoscontrol.protocol_observation.evidence] r[chaoscontrol.protocol_observation.validation]
