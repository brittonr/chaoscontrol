## Phase 1: Dependencies and baseline

- [ ] [depends:protocol-observation-cohorts] Wait for the local protocol-observation cohort change, then pin its immutable profile, envelope, completeness, oracle-handoff, novelty, snapshot, and evidence identities. r[chaoscontrol.async_flow.observations]
- [ ] [depends:choregraph-add-asynchronous-flow-profile] Pin immutable Choregraph global, complete local-set, projection, assumption, schema, fixture, and source identities. r[chaoscontrol.async_flow.cohorts]
- [ ] [depends:trellis-prove-asynchronous-flow-algebra] Pin immutable Trellis operation, domain, law, proof, verifier, assumption, manifest, and source identities. r[chaoscontrol.async_flow.cohorts]
- [ ] [depends:lattice-execute-projected-asynchronous-flows] Pin immutable Lattice session, edge, observation, persistence, outcome, replay, adapter, fixture, and source identities. r[chaoscontrol.async_flow.cohorts]
- [ ] [serial] Record current network, process, snapshot, replay, assertion, observation, evidence, Cairn, Octet, and Nix baselines. r[chaoscontrol.async_flow.validation]

## Phase 2: Profiles, adapters, and cases

- [ ] [serial] Define the typed Nickel campaign profile for cohorts, roles, placements, flows, laws, assumptions, faults, assertions, observations, bounds, and non-claims. r[chaoscontrol.async_flow.profile]
- [ ] [serial] Define versioned narrow adapters for selected Choregraph, Trellis, Lattice, and protocol-observation artifacts. r[chaoscontrol.async_flow.cohorts]
- [ ] [parallel] Add positive adapter fixtures and negative stale-revision, schema-drift, wrong-law, wrong-domain, missing-identity, unsupported-version, and partial-cohort fixtures. r[chaoscontrol.async_flow.cohorts] r[chaoscontrol.async_flow.validation]
- [ ] [serial] Define frozen independently reviewed expected-outcome fixtures without using the Lattice runtime under test as the only oracle. r[chaoscontrol.async_flow.oracle]
- [ ] [serial] Implement pure case expansion for fault-free, reorder, duplicate, delay, loss, partition, heal, termination, restart, uncertainty, closure, prefix, and replay cases. r[chaoscontrol.async_flow.matrix]

## Phase 3: Oracles and assertions

- [ ] [serial] Implement the independent canonical set-union oracle over the admitted logical item cohort. r[chaoscontrol.async_flow.oracle]
- [ ] [serial] Implement complete, valid-prefix, expected-block, uncertain, assertion-violation, unsupported, incomplete, and indeterminate classification. r[chaoscontrol.async_flow.outcomes]
- [ ] [serial] Implement prefix-subset, closure, final-drain, cleanup, and observation-completeness evaluation. r[chaoscontrol.async_flow.observations]
- [ ] [serial] Register stable assertions for wrong item, wrong edge, wrong operator, duplicate application, false order, forged closure, missing closure, early protected effect, hidden retry, stale law, erased assumption, and replay dispatch. r[chaoscontrol.async_flow.assertions]
- [ ] [parallel] Add false-runtime-self-oracle, packet-count-only, missing-item-pass, missing-closure-pass, duplicate-count, concatenation, and early-effect negative cases. r[chaoscontrol.async_flow.oracle] r[chaoscontrol.async_flow.outcomes] r[chaoscontrol.async_flow.assertions] r[chaoscontrol.async_flow.validation]

## Phase 4: Faults and observations

- [ ] [parallel] Add deterministic duplication, reordering, delay, loss, partition, and heal schedules with exact activation points. r[chaoscontrol.async_flow.faults]
- [ ] [parallel] Add role termination and restart before persistence, after persistence, before dispatch, after possible dispatch, before observation, before closure, and before outcome commit. r[chaoscontrol.async_flow.faults]
- [ ] [serial] Preserve selected, applicable, rejected, applied, application-failed, observed, healed, and indeterminate facts for each selected fault. r[chaoscontrol.async_flow.faults]
- [ ] [serial] Map participant generation, source sequence, logical boundary, edge, operator, item, result, closure, window, attempt, outcome, loss, final-drain, and cleanup records into admitted protocol-observation cohorts. r[chaoscontrol.async_flow.observations]
- [ ] [serial] Evaluate each Choregraph nondeterminism assumption against its exact expected relation and selected bounded campaign profile. r[chaoscontrol.async_flow.assumptions]

## Phase 5: Rails, replay, and evidence

- [ ] [parallel] Add a cheap pure and in-process rail for profiles, adapters, matrices, oracles, assertions, faults, observations, assumptions, classifiers, identities, and negative fixtures. r[chaoscontrol.async_flow.validation]
- [ ] [serial] Add a separate KVM rail for fault-free union, reorder, duplicate, loss, partition, heal, crash uncertainty, closure, effect denial, and replay. r[chaoscontrol.async_flow.kvm]
- [ ] [serial] Add snapshot-backed replay for at least one selected asynchronous-flow outcome and reject replay that dispatches flow or protected effects. r[chaoscontrol.async_flow.replay]
- [ ] [parallel] Add negative missing-KVM, stale-snapshot, wrong-parent, incomplete-observation, failed-cleanup, false-total-order, and evidence-overclaim cases. r[chaoscontrol.async_flow.kvm] r[chaoscontrol.async_flow.replay] r[chaoscontrol.async_flow.evidence] r[chaoscontrol.async_flow.validation]
- [ ] [serial] Add domain-separated BLAKE3 campaign, cohort, oracle, matrix, run, observation, assertion, assumption, snapshot, replay, and receipt identities. r[chaoscontrol.async_flow.evidence]
- [ ] [serial] Bind exact cohorts, faults, observations, outcomes, blockers, replay, bounds, and non-claims into one bounded receipt. r[chaoscontrol.async_flow.evidence]

## Phase 6: Documentation and closeout

- [ ] [parallel] Document campaign authoring, cohort pinning, independent oracle review, flow assertions, fault stages, observation limits, assumptions, replay scope, ownership boundaries, and non-claims. r[chaoscontrol.async_flow.boundary]
- [ ] [serial] Run focused core, simulator, network, fault, assertion, protocol-observation, replay, evidence, formatting, Clippy, Octet, Cairn, KVM, and relevant Nix checks. r[chaoscontrol.async_flow.validation]
- [ ] [serial] Run the adversarial audit and block archive for semantic corruption, false completion, early effects, stale laws, self-oracles, incomplete accounting promotion, erased assumptions, or unsupported promotion. r[chaoscontrol.async_flow.validation] r[chaoscontrol.async_flow.boundary]
- [ ] [serial] Review sync and archive plans and archive only with immutable producer, proof, runtime, observation, KVM, and replay evidence. r[chaoscontrol.async_flow.validation]
