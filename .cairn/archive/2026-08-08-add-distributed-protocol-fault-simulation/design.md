## Goals

- Provide a deterministic, replayable protocol-simulation rail.
- Inject bounded faults through explicit hooks.
- Prove a failing schedule reproduces from a single seed and schedule.
- Keep protocol-simulation evidence separate from VMM and in-process evidence.

## Simulation contract

The rail accepts an adapter-based protocol. The adapter supplies deterministic transitions for ownership, replication, and reacquisition facts.

The run config binds:

- the seed;
- the schedule;
- the virtual clock policy;
- the RNG policy;
- the fault schedule reference;
- the protocol and adapter identities.

All sources of nondeterminism bind to the seed and schedule.

## Fault injection

The fault hooks cover bounded classes:

- node loss;
- message loss;
- message reorder;
- message duplication;
- partition.

An injected fault follows the declared schedule. An unregistered external effect fails the run as unsupported protocol-simulation evidence.

## Replay proof

A reproducibility receipt binds the config, the output history, the fault schedule, and the artifact digests.

Identical seed and schedule reproduce a matching history and digests. A divergent history fails and identifies the first bounded mismatch class.

## Evidence boundary

Protocol-simulation evidence stays separate from VMM snapshot replay proof and in-process simulator evidence.

The readiness surface labels this rail as adapter-based protocol-simulation evidence. It does not promote VM replay, arbitrary protocol correctness, or Celld-equivalent behavior.

## Functional core and shell

The pure core computes the transition and mismatch decisions from supplied facts.

The shell owns evidence emission and readiness surfaces. Consumers retain transport and protocol authority.

## Reference

The Celld protocol-simulation approach is a bounded reference input. It is a comparison source, not a ChaosControl requirement, parity claim, or equivalence claim.

## Verification

Positive coverage includes a deterministic replay of a matching schedule.

Negative coverage includes a divergent history and a nondeterministic run.

Boundary coverage rejects an unregistered external effect, an overclaim to VM or Celld parity, and arbitrary-protocol correctness claims.
