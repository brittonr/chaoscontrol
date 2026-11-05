# Protocol observation cohorts

The protocol-observation contract transports opaque, bounded observations from
multi-node guests and assembles complete cohorts at consumer-defined logical
boundaries. ChaosControl does not learn protocol semantics.

## Contract surface

`chaoscontrol_smr::observation` publishes:

| Item | Role |
| --- | --- |
| `ProtocolProfile` | Typed profile identity: protocol, projection, boundary schema, oracle adapter, independent oracle, novelty selection, participants, finite bounds, marker policy, non-claims. Rust admission mirrors `contracts/evidence/protocol-observation-profile.ncl`. |
| `Bounds` | Finite records, bytes per record, total bytes, boundaries, cohort backlog, and oracle work. |
| `ProtocolObservation` | Opaque envelope: profile, protocol, producer, participant, guest, generation, source sequence, transition class, logical boundary, projection schema and ref, optional canonical projection bytes, scheduler position, loss counter. |
| `assemble_cohort` | Pure assembly by exact boundary and profile. Returns `complete`, `incomplete`, `conflicting`, or `unsupported` with record refs, a cohort ref, a novelty ref, and blockers. |
| `MarkerBinding` | Binds a declared marker to an admitted record, boundary, projection, cohort, and restorable parent snapshot. Retains cohort completeness; never upgrades it. |
| `ProtocolOracle` / `evaluate_oracle` | Consumer-owned pure evaluation over admitted cohorts with typed `OracleResult` and exact oracle identity. Incomplete cohorts are recorded without invoking the oracle; a runtime self-oracle is rejected. |
| `ProtocolReceipt` | Binds profile, bounds, boundary, cohort, completeness, records, novelty, oracle adapter/result, marker, snapshot, scheduler, fault, replay, and non-claim facts. |
| `protocol_status` | Bounded operator summary: participant coverage, sequence gaps, blockers, oracle verdict, marker reachability, novelty ref. |
| `protocol_novelty` (`chaoscontrol-explore`) | Maps a full BLAKE3 novelty ref to a compact coverage slot while retaining every full identity. |
| `ProtocolObservationCollector` (`chaoscontrol-vmm`) | Bounded per-VM VMM retention at the lifecycle hypercall. Binds the deterministic scheduler position, rejects producer scheduler claims and malformed or oversized envelopes, and keeps projection bytes opaque. |

## Invariants

- Record, cohort, novelty, oracle-result, binding, and receipt identities are domain-separated BLAKE3 over canonical serialization. Process-local hashes never become durable identity.
- The VMM binds the scheduler position at the lifecycle hypercall from the deterministic vCPU scheduler state, retains envelopes only after the oracle admits the event for an active run, and clears retained observations on snapshot or incremental restore. A producer-supplied scheduler position is rejected.
- Admission is fail-closed on schema drift, unknown participants, generation mismatch, projection digest mismatch, duplicate source sequences, sequence gaps, loss counters, failed final drains, and exceeded bounds.
- Cohort completeness is retained verbatim. Markers, snapshots, and replay cannot complete an incomplete cohort or claim one cross-participant wall-clock instant.
- A passing oracle result requires a complete admitted cohort. A receipt rechecks that rule and rejects promotions.
- Profile, receipt, and status text is bounded; every participant, record, boundary, and oracle-work count is finite.

## Adapter recipe

1. Author a Nickel profile under `contracts/evidence/` and admit its JSON projection with `ProtocolProfile::admit`.
2. Emit one envelope per observation at a consumer-defined logical boundary. Provide canonical projection bytes or an external projection ref. Source sequences start at 1 and increase by one; the VMM binds the scheduler position.
3. Emit one final drain per producer with the last source sequence and loss total.
4. Assemble the cohort in the pure core; do not reconstruct order from timestamps or arrival order.
5. Map the admitted cohort into your own pure oracle and return a typed result with your immutable oracle identity.
6. Bind markers and restorable snapshots only through `MarkerBinding`, after the declared-event branching contract is admitted.
7. Build a `ProtocolReceipt` and publish it with the full novelty identity and every applicable ref.

The storage-recovery, projected role-protocol, Molten, and later Campaign
adapters own their projection schemas, oracle semantics, and protocol claims.
ChaosControl owns transport identity, bounds, completeness, and receipt linkage.

## Non-claims

Admission, cohort assembly, oracle results, receipts, and operator status are
bounded observations over supplied facts. They do not prove protocol
correctness, universal or cross-participant total order, determinism or
snapshot-backed replay, production readiness, release eligibility, or security.
