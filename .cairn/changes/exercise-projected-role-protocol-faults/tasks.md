# Tasks: Projected role-protocol fault campaigns

## Dependencies, adapters, and baseline

- [ ] [depends:protocol-observation-cohorts] Wait for immutable Choregraph and Lattice protocol cohorts, then pin the published observation contract, repository revisions, schemas, artifacts, fixtures, adapters, and source-manifest BLAKE3 identities. r[chaoscontrol.role_protocol.cohorts]
- [ ] [serial] Record baseline network, process-restart, snapshot, replay, assertion, observation, evidence, Cairn, and Nix results before core changes. r[chaoscontrol.role_protocol.validation]
- [ ] [serial] Define versioned narrow adapters for selected Choregraph global and local artifacts plus Lattice session, envelope, persistence, outcome, and recovery data. r[chaoscontrol.role_protocol.cohorts]
- [ ] [parallel] Add positive adapter fixtures and negative stale revision, schema drift, unknown field, missing identity, wrong role, and unsupported-version fixtures. r[chaoscontrol.role_protocol.cohorts] r[chaoscontrol.role_protocol.validation]

## Profile and independent outcomes

- [ ] [serial] Define the typed Nickel campaign profile for cohorts, artifacts, roles, placements, cases, faults, assertions, observations, bounds, and non-claims. r[chaoscontrol.role_protocol.profile]
- [ ] [serial] Define frozen independently reviewed expected-outcome fixtures without using the Lattice runtime under test as the only oracle. r[chaoscontrol.role_protocol.oracle]
- [ ] [parallel] Add oracle mismatch fixtures for session, role, peer, step, label, payload tag, value, attempt, artifact, outcome, and recovery eligibility. r[chaoscontrol.role_protocol.oracle] r[chaoscontrol.role_protocol.validation]
- [ ] [serial] Implement pure case expansion for fault-free transfer, fault-free choice, block, uncertainty, failure, heal, and replay scenarios. r[chaoscontrol.role_protocol.matrix]

## Protocol assertions and classifiers

- [ ] [serial] Register stable assertions for wrong-session, wrong-role, wrong-step, duplicate-commit, reordered-skip, stale-label, former-owner, replay-dispatch, unknown-outcome, and terminal-mutation violations. r[chaoscontrol.role_protocol.assertions]
- [ ] [serial] Implement pure assertion evaluation over admitted protocol state and runtime observations. r[chaoscontrol.role_protocol.assertions]
- [ ] [serial] Implement pure outcome classification for completion, block, unknown, terminal failure, assertion violation, protocol mismatch, transport outcome, runtime failure, partial observation, unsupported, and indeterminate. r[chaoscontrol.role_protocol.outcomes]
- [ ] [parallel] Add negative false-success, missing-message-safe-block, packet-count-only, process-exit-only, and terminal-mutation cases. r[chaoscontrol.role_protocol.outcomes] r[chaoscontrol.role_protocol.validation]

## Faults, observations, and replay

- [ ] [parallel] Add deterministic loss, delay, duplication, reordering, corruption, partition, bandwidth, and heal schedules where supported. r[chaoscontrol.role_protocol.faults]
- [ ] [parallel] Add role termination and restart schedules before persistence, before dispatch, before observation, before commit, around choice labels, and before recovery. r[chaoscontrol.role_protocol.faults]
- [ ] [serial] Preserve selected, applicable, applied, observed, healed, failed, and indeterminate stages for every selected fault. r[chaoscontrol.role_protocol.faults]
- [ ] [serial] Map required producer sequence, loss, overflow, truncation, final-drain, detach, and cleanup accounting into admitted protocol-observation cohorts. r[chaoscontrol.role_protocol.observation]
- [ ] [serial] Add snapshot-backed replay for at least one selected protocol fault outcome and reject replay that dispatches protocol effects. r[chaoscontrol.role_protocol.replay]

## Evidence and rails

- [ ] [serial] Add domain-separated BLAKE3 campaign, cohort, oracle, matrix, run, observation, assertion, snapshot, replay, and receipt identities. r[chaoscontrol.role_protocol.evidence]
- [ ] [parallel] Add a cheap pure, Nickel, adapter, oracle, assertion, classifier, fixture, identity, and in-process simulation rail without KVM. r[chaoscontrol.role_protocol.validation]
- [ ] [serial] Add a separate KVM rail covering fault-free transfer, labeled choice, duplicate or reorder rejection, crash uncertainty, partition, heal, and replay. r[chaoscontrol.role_protocol.validation]
- [ ] [parallel] Add negative missing-KVM, stale-cohort, tautological-oracle, incomplete-observation, failed-cleanup, false-total-order, and evidence-overclaim cases. r[chaoscontrol.role_protocol.observation] r[chaoscontrol.role_protocol.boundary] r[chaoscontrol.role_protocol.validation]

## Documentation and closeout

- [ ] [parallel] Document campaign authoring, cohort pinning, independent outcome review, assertion meaning, fault stages, observation limits, replay scope, ownership boundaries, and non-claims. r[chaoscontrol.role_protocol.boundary]
- [ ] [serial] Run focused core, simulator, network, fault, assertion, trace, replay, evidence, formatting, Clippy, Cairn, KVM, and relevant Nix checks. r[chaoscontrol.role_protocol.validation]
- [ ] [serial] Run the adversarial audit and block archive for forbidden advancement, false success, stale cohort, tautological oracle, incomplete accounting promotion, or replay dispatch. r[chaoscontrol.role_protocol.validation] r[chaoscontrol.role_protocol.boundary]
- [ ] [serial] Review sync and archive plans and archive only with immutable producer and runtime cohorts plus retained KVM and replay evidence. r[chaoscontrol.role_protocol.validation]
