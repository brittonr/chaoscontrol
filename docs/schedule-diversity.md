# Deterministic schedule diversity

ChaosControl can vary the deterministic SMP scheduler policy for each exploration branch.

When schedule diversity is enabled for more than one vCPU, normal and havoc mutation paths return a fault schedule plus an optional `ScheduleVariant`. Parallel and sequential execution carry that variant to the branch shell.

The shell restores the branch snapshot, applies the variant to every VM scheduler, and then starts the counterfactual fault run. An invalid strategy or quantum returns a typed branch error.

The scheduler state identity and compact fingerprint bind these variant facts:

- scheduler seed;
- strategy override and randomized quantum bounds;
- quantum override.

Bug reports retain the complete variant. Replay verdicts retain the seed, strategy projection, quantum override, and BLAKE3 policy identity. Replay construction rejects policy-identity drift.

## Mechanism gate

The focused race fixture models an initialization race. The default quantum keeps the first vCPU active beyond the race window. Generated variants include a shorter deterministic quantum that reaches the failing interleaving.

Negative fixtures cover disabled diversity, single-vCPU admission, invalid strategies, zero quantum, and replay identity drift.

## Claim boundary

A passing fixture proves only that the selected deterministic scheduler can vary one bounded interleaving model. It does not prove exhaustive interleaving coverage, application correctness, host determinism, or absence of races.
