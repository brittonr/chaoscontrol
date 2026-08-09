# Tasks: Identity-aware connectivity campaigns

## Reference, dependency, and baseline

- [ ] [serial] Record the reviewed Cilium revision, selected concepts, Apache-2.0 boundary, rejected runtime dependencies, and rollback policy. r[chaoscontrol.identity_connectivity.boundary]
- [ ] [serial] Wait for a published immutable OnixOS network-policy contract, then pin its repository revision, schema identities, fixtures, and source-manifest BLAKE3. r[chaoscontrol.identity_connectivity.producer_adapter]
- [ ] [serial] Record baseline deterministic network, fault, snapshot, replay, eBPF trace, and campaign-profile results before core changes. r[chaoscontrol.identity_connectivity.verification]

## Profiles and adapter

- [ ] [serial] Define the typed Nickel campaign profile for producer contracts, workloads, topology, paths, protocols, expected decisions, faults, observations, named bounds, and non-claims. r[chaoscontrol.identity_connectivity.profile]
- [ ] [serial] Implement a pure versioned adapter for the frozen OnixOS workload, decision, explanation, flow-event, and completeness fields. r[chaoscontrol.identity_connectivity.producer_adapter]
- [ ] [parallel] Add positive adapter fixtures and negative stale revision, schema drift, unknown field, missing identity, unsafe metadata, and unsupported-version fixtures. r[chaoscontrol.identity_connectivity.producer_adapter] r[chaoscontrol.identity_connectivity.verification]

## Independent oracle and matrix

- [ ] [serial] Define frozen independently reviewed expected-decision fixtures that do not call the OnixOS production evaluator during campaign execution. r[chaoscontrol.identity_connectivity.oracle]
- [ ] [serial] Implement pure matrix expansion for same-VM, cross-VM, direct, relay, ingress, egress, expected-allow, expected-deny, and unsupported cases. r[chaoscontrol.identity_connectivity.matrix]
- [ ] [parallel] Add oracle mismatch fixtures for workload identity, tier, rule, direction, protocol, verdict, and unsupported conditions. r[chaoscontrol.identity_connectivity.oracle] r[chaoscontrol.identity_connectivity.verification]

## Execution, faults, and observations

- [ ] [serial] Add guest and harness support for bounded policy-path triggers and structured outcomes without raw-log or packet-absence oracles. r[chaoscontrol.identity_connectivity.outcomes]
- [ ] [parallel] Add deterministic partition, loss, corruption, duplication, reordering, delay, and heal cases where the selected network fabric supports them. r[chaoscontrol.identity_connectivity.faults]
- [ ] [serial] Implement pure classification that distinguishes policy deny, policy mismatch, transport drop, routing failure, guest failure, timeout, partial observation, unsupported, and indeterminate outcomes. r[chaoscontrol.identity_connectivity.outcomes]
- [ ] [serial] Reuse eBPF trace accounting for optional live capture and preserve sequence, loss, ordering, final-drain, detach, and cleanup limits. r[chaoscontrol.identity_connectivity.observation]

## Evidence and rails

- [ ] [serial] Add domain-separated BLAKE3 campaign, matrix, run, observation, replay, and receipt identities with exact producer, oracle, topology, fault, and artifact links. r[chaoscontrol.identity_connectivity.evidence]
- [ ] [parallel] Add a cheap pure, Nickel, adapter, fixture, identity, oracle, matrix, and evidence check that does not require KVM. r[chaoscontrol.identity_connectivity.verification]
- [ ] [serial] Add a separate KVM behavior rail covering one expected allow, expected deny, fault, heal, partial-observation, and unsupported case. r[chaoscontrol.identity_connectivity.verification]
- [ ] [parallel] Add negative delivered-deny, absent-packet-deny, wrong-path, loss, malformed-event, false-order, failed-cleanup, missing-KVM, and evidence-overclaim cases. r[chaoscontrol.identity_connectivity.observation] r[chaoscontrol.identity_connectivity.verification]

## Documentation and closeout

- [ ] [parallel] Document campaign authoring, independent oracle review, path and protocol support, fault meaning, loss accounting, replay scope, producer ownership, and non-claims. r[chaoscontrol.identity_connectivity.boundary]
- [ ] [serial] Run focused core, network, fault, trace, replay, evidence, formatting, Clippy, Cairn, and relevant Nix checks. r[chaoscontrol.identity_connectivity.verification]
- [ ] [serial] Sync and archive only after the OnixOS producer contract is immutable and the required KVM evidence is retained. r[chaoscontrol.identity_connectivity.verification]