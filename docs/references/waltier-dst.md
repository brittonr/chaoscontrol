# WalTier Deterministic Simulation Reference

WalTier is a bounded comparison source for object-store coordination tests. This record uses revision `d5dda89fb176d590d03c7812d047ced2712bba94`.

The upstream repository is [`danthegoodman1/waltier`](https://github.com/danthegoodman1/waltier/tree/d5dda89fb176d590d03c7812d047ced2712bba94).

## Mechanism layer

WalTier runs deterministic, in-process simulations at an object-store seam. A seeded run interleaves writers, replicas, compactors, injected store faults, latency, and crash or reopen cycles.

ChaosControl runs deterministic VMM and guest simulations. Its KVM, guest, workload, replay, and evidence boundaries remain authoritative for those claims.

The two mechanisms operate at different layers. This reference creates no new ChaosControl gate or product requirement.

## Oracle invariants

The WalTier simulation oracle checks these bounded invariants after each step:

- The committed history grows monotonically.
- Each instance state is an exact prefix of the committed history.
- Each live snapshot object stays reachable, and stale snapshot objects do not leak.

The upstream documentation calls the third invariant snapshot-object retention. This record uses conservation to include both missing and leaked objects.

## Reproduction inputs

WalTier binds a failed simulation to its seed. The simulation includes store faults and crash or reopen cycles as explicit generated inputs.

These inputs can inform object-store simulation design. They do not transfer WalTier implementation behavior into ChaosControl.

## Claim boundary

This record does not add WalTier as a dependency. It does not port log, compaction, or reconciliation code.

This record does not prove WalTier correctness, ChaosControl correctness, parity, equivalence, KVM replay, or release readiness.
