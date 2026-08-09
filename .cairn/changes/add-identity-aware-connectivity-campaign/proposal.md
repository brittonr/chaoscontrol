# Proposal: Add identity-aware connectivity campaigns

## Summary

Add bounded deterministic campaigns that exercise identity-aware network-policy paths across ChaosControl VMs and simulated network links. Consume a published OnixOS policy and flow contract through a frozen adapter, use independent expected-decision fixtures, inject network faults, and produce scoped connectivity evidence.

Use Cilium connectivity checks and Hubble loss accounting as design references at revision `8c0423e970e62706bcd5dd3a57e1ffaee697439c`. Keep ChaosControl simulation, fault, replay, trace, and evidence semantics independent.

## Motivation

ChaosControl already owns deterministic network transitions, fault schedules, replay, BPF trace evidence, and exact artifact identities. It does not define a campaign model for checking expected policy allow and deny paths across a workload topology.

A campaign that asks only whether packets arrived can confuse policy denial, transport loss, routing failure, guest failure, and missing observation. A campaign that imports the producer policy evaluator as its oracle can also repeat the same defect and report a false success.

The campaign needs explicit workload identities, expected decisions, path variants, faults, observation accounting, and non-claims.

## Scope

- Define a typed Nickel campaign profile that binds exact workload, policy-schema, adapter, guest, topology, protocol, path, fault, oracle, bound, and non-claim identities.
- Consume a frozen published OnixOS policy and flow contract through a versioned adapter.
- Define an independent expected-decision fixture format instead of calling the OnixOS policy evaluator as the campaign oracle.
- Cover same-node, cross-node, direct, relay, ingress, egress, expected-allow, and expected-deny paths where the selected guest and simulator support them.
- Distinguish policy denial, transport drop, routing failure, guest failure, timeout, unsupported path, and observation loss.
- Use the existing deterministic network fabric for partition, loss, corruption, duplication, reordering, delay, and heal schedules.
- Bind optional live eBPF observations through the existing `ebpf-trace-evidence` contract and preserve its loss and ordering limits.
- Emit domain-separated BLAKE3 campaign, matrix, run, observation, replay, and receipt identities.
- Add cheap pure and fixture checks plus a separate KVM behavior rail.

## Affected specs

- `identity-aware-connectivity`: campaign profiles, independent oracles, path outcomes, faults, evidence, boundaries, and verification.

## Dependencies

- A published immutable OnixOS revision containing the `network-policy.identity-aware.*` contract and fixtures.
- Existing ChaosControl deterministic simulation kernel, network fabric, fault outcomes, snapshots, replay, campaign profiles, and eBPF trace evidence.
- The OnixOS `add-identity-aware-network-policy` change remains producer-owned and must publish its contract before this consumer can close.

## Non-goals

- Do not make ChaosControl the owner of OnixOS network-policy, workload-identity, BPF Pack, authority, or readiness semantics.
- Do not use the OnixOS production evaluator as the only expected-decision oracle.
- Do not claim complete traffic history when capture accounting is partial or unavailable.
- Do not claim that simulated delivery proves physical networks, switches, NICs, firmware, clocks, encryption, or production availability.
- Do not require Kubernetes, Cilium, Hubble, CNI, Envoy, etcd, or ClusterMesh.
- Do not add arbitrary Layer 7 parsing. Unsupported protocols remain explicit.

## Compatibility

Existing ChaosControl campaigns, network simulation, fault schedules, replay classes, eBPF trace receipts, and evidence gates remain unchanged. The new campaign family is additive.

The cheap default rail does not require KVM. Missing KVM, guest support, exact BPF cohort, relay support, or a published OnixOS contract produces blocked or unsupported evidence, not a pass.

## Completion

The change is ready to sync and archive when one expected-allow path, one expected-deny path, one faulted path, one healed path, and all required negative oracle and observation cases pass. At least one KVM run must bind the exact consumer and producer contract identities.