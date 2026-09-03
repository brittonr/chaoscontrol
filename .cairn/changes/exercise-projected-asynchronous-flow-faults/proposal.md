## Why

Choregraph will describe asynchronous edge semantics and Lattice will execute projected local flows. Neither repository owns deterministic fault schedules, cross-run observation completeness, independent result oracles, snapshots, replay, or fault evidence.

The existing projected role-protocol campaign validates finite transfers and choices. Asynchronous flows need another campaign because reordering and duplication can be valid for one operator and invalid for another.

ChaosControl already plans and records deterministic network and process faults. It needs a narrow consumer adapter and oracle for the published flow cohorts.

## What Changes

- Consume immutable Choregraph flow, Trellis law, and Lattice runtime cohorts through versioned narrow adapters.
- Depend on the local protocol-observation cohort mechanism for opaque bounded observations and completeness accounting.
- Add a typed Nickel campaign profile for flow fixtures, placements, operators, edges, law refs, assumptions, faults, bounds, assertions, and non-claims.
- Add independent expected-outcome fixtures and pure oracles for ACI convergence, monotone prefixes, closure, uncertainty, and effect-release denial.
- Exercise reordering, duplication, delay, loss, partition, heal, role termination, and restart.
- Distinguish complete convergence, valid incomplete prefix, expected block, explicit uncertainty, assertion violation, unsupported, and indeterminate outcomes.
- Add a cheap pure and in-process rail plus a separate selected KVM replay rail.
- Bind exact producer, runtime, proof, workload, fault, observation, oracle, snapshot, replay, and claim-boundary identities.

## Success Criteria

- Reordered or duplicated set-union inputs produce one canonical result when complete observation and closure hold.
- Loss or missing closure produces an incomplete prefix, not a false completed result.
- A partial monotone result cannot trigger a protected effect without exact prefix-safety evidence.
- Wrong-law, stale-cohort, non-ACI, forged-closure, incomplete-observation, and runtime-self-oracle fixtures fail.
- Every selected fault records selection, applicability, application, observation, healing, and failure facts.
- Snapshot replay can reproduce at least one exact selected asynchronous-flow outcome.

## False Completion

Comparing packet counts is not completion. Asking the Lattice runtime whether its own output is correct is not completion.

One fault-free set-union run is not convergence evidence. A missing assertion failure is not proof that observations were complete.

## Impact

- **Profiles**: typed asynchronous-flow campaign and exact consumer cohorts.
- **Core**: pure case expansion, expected-outcome comparison, observation classification, and oracle evaluation.
- **Shell**: adapters, guests, simulated network, process faults, snapshots, replay, and evidence publication.
- **Testing**: pure, in-process, KVM, replay, stale-cohort, false-oracle, closure, law, prefix, and overclaim cases.
- **Evidence**: complete identities and bounded non-claims for every selected run.

## Dependencies

- Local change `add-protocol-observation-cohorts` must publish its immutable envelope, completeness, novelty, oracle-handoff, and replay contract.
- Choregraph change `add-asynchronous-flow-profile` must publish an immutable global and local flow cohort.
- Trellis change `prove-asynchronous-flow-algebra` must publish the selected law-evidence cohort.
- Lattice change `execute-projected-asynchronous-flows` must publish an immutable runtime, observation, persistence, and outcome cohort.
- The existing projected role-protocol fault campaign remains separate and unchanged.

## Out of Scope

- Defining flow, algebra, runtime, placement, or authorization semantics.
- Treating packet order as a semantic total order.
- Proving all schedules, all failures, universal convergence, or universal determinism.
- Claiming transport delivery, exactly-once effects, production readiness, or release eligibility.
- Replacing the existing generic fault and protocol-observation mechanisms.

## Affected Specs

- `asynchronous-flow-fault-campaign`: profiles, cohorts, oracles, faults, observations, replay, evidence, validation, and non-claims.
