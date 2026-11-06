# Design: Restore Explorer Seed Repeatability

## Context

Explorer inputs are the seed, the configuration, the kernel, the initrd, and the VMM build. The mutator and frontier use seeded `ChaCha8Rng`. Branches run in index order on the sequential path. Divergence already appears in the first round, whose branches all start from the bootstrap snapshot. So the cause lies in bootstrap, in branch execution, or in state carried between sibling branches. It does not lie in frontier selection.

## Decisions

### 1. Branch digests

Each completed branch records a BLAKE3 digest over its coverage bitmap, canonical oracle report bytes, per-VM exit counts, total ticks, and schedule fingerprint. Round history stores the ordered digest list. The digests use the stack-wide BLAKE3 convention.

### 2. Pure comparison core

A pure function compares two run histories. It returns the first mismatching round and branch with the mismatching digest components, or it returns equal. Wall-clock timings, watchdog counts, and failed-branch counts are excluded from comparison and checked separately for operational exclusion.

### 3. Diagnosis order

The diagnosis compares, in order: the bootstrap snapshot digest, the first branch digest, and the second branch digest. Candidate causes are host-timing leaks into guest-visible time or idle detection, device backend state, and page state carried between sibling branches. Each candidate gets a focused KVM regression before it is fixed.

### 4. Gate

A runner executes the same configuration twice in fresh processes and passes the histories to the comparison core. The gate records its inputs and the comparison result in a receipt. The Raft control workload and one planted variant are the maintained gate cases.

## Risks

- Several causes may combine. The gate stays failing until every divergence in the maintained cases is gone.
