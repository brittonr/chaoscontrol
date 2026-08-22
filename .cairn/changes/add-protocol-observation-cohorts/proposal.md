# Proposal: Add protocol-observation cohorts

## Why

ChaosControl can record workload events and use their structured details as coverage guidance. First-party Raft fixtures also emit protocol-specific state coverage.

These paths do not provide one reusable typed mechanism for coordinated cross-participant observations. They do not assemble bounded cohorts at consumer-defined logical positions. Durable protocol novelty can also depend on process-local hashing.

Protocol-aware campaigns need an opaque transport and cohort mechanism. Workloads must retain protocol meaning and oracle authority. ChaosControl must retain deterministic execution, bounds, observation accounting, snapshots, replay, and evidence.

## What Changes

- Add a typed Nickel profile for protocol-observation envelopes, participants, logical boundaries, bounds, oracle adapters, novelty fields, markers, and non-claims.
- Add canonical Rust runtime records with domain-separated BLAKE3 identities.
- Add a pure core that admits observations and assembles complete, incomplete, conflicting, or unsupported cohorts.
- Add consumer-owned pure oracle adapters over admitted cohorts.
- Add stable novelty identities without using process-local hashes as durable evidence.
- Bind optional declared markers and parent snapshots to protocol observations.
- Emit complete observation, cohort, oracle, replay, and claim-boundary evidence.

## Impact

- **Files**: SDK events, oracle types, explorer guidance, VMM observation collection, evidence records, Nickel contracts, first-party fixtures, and documentation.
- **Testing**: valid cohorts, sequence gaps, loss counters, generation drift, conflicting projections, false self-oracles, novelty stability, marker replay, bounds, and unsupported adapters.
- **Architecture**: ChaosControl owns the bounded mechanism. Workloads and consumers own projection schemas, protocol semantics, and oracle verdict meaning.
- **Consumers**: the active storage-recovery and role-protocol campaigns can adopt the published mechanism. Molten can consume it through an immutable adapter contract.
- **Claims**: cohort assembly proves observation structure and completeness only. It does not prove a protocol invariant.
