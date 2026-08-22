## Why

ChaosControl owns deterministic simulation evidence at the VMM layer. This layer includes deterministic SMP, seeded entropy, fault injection, exploration, and replay.

`AGENTS.md` names Antithesis as a comparison source for simulation methods. It does not name a smaller in-process store simulation.

WalTier provides that second mechanism layer. Its tests interleave writers, replicas, compactors, faults, and crash/reopen cycles from one seed. An oracle checks invariants after each step. A failed case reproduces from the same seed.

The named invariants are history monotonicity, exact-prefix instance state, and snapshot-object retention. These invariants can inform object-store simulations without moving VMM or workload authority.

## What Changes

- Record WalTier DST beside Antithesis as a bounded comparison source.
- Name history monotonicity, exact-prefix state, and object-retention invariants.
- Name seeded store faults and crash/reopen cycles as comparison inputs.
- Keep VMM execution, guest evidence, and workload authority unchanged.

## Impact

- **Files**: ChaosControl reference documentation and the deterministic simulation design record.
- **Testing**: Existing suites remain authoritative. The record adds no implementation claim.
- **Architecture**: In-process store simulation and KVM guest simulation remain separate evidence rails.
- **Claims**: The record does not prove WalTier parity, store correctness, or ChaosControl equivalence.

## Non-goals

- Do not add WalTier as a dependency.
- Do not port its log, compaction, or reconciliation mechanisms.
- Do not claim parity with, or equivalence to, WalTier.
