# Bounded SMR chain workload

ChaosControl provides an implementation-neutral workload for selected Rust state-machine replication consumers. The workload checks bounded application observations. It does not inspect terms, elections, quorums, or protocol messages.

## Profile and identity

The source profile is `contracts/evidence/smr-workload-profile.ncl`. Export it to JSON before execution. Rust rejects unknown fields and checks each finite count, byte, progress, trace, fault, replay, evidence, and reduction bound again.

The genesis digest uses the domain `chaoscontrol.smr-chain.genesis.v1`. It binds the profile reference and the canonical initial-state reference.

Each transition uses the domain `chaoscontrol.smr-chain.transition.v1`. It binds these fields with length framing:

1. Profile reference.
2. Command index in network byte order.
3. Prior digest.
4. Command length in network byte order.
5. Exact command bytes.

The workload uses BLAKE3 because these identities are stack-owned. It does not replace an interoperability hash.

## Consumer handoff

A consumer adapter supplies these semantic facts:

- bounded proposal requests and outcomes;
- committed application transitions;
- canonical application-state references;
- lifecycle and terminal states;
- observation mode and dropped-event count.

The adapter must observe the committed application path. It must not generate expected digests without that path. The first-party Raft adapter projects committed application values through `Node::committed_application_values`. Existing Raft assertions and replay classes stay separate.

External consumers retain executable authority, state-machine meaning, secrets policy, adapter review, and release decisions. Use immutable package, schema, build, adapter, and observer references. Do not use workspace-relative paths as durable handoff identities.

## Proposal outcomes

An outcome is `acknowledged`, `definitely-rejected`, or `indefinite`. A timeout or lost connection is indefinite. It does not prove that the command was absent.

Retries keep one operation identity and one command identity. A later committed observation can resolve an earlier indefinite outcome.

## Safety and liveness

Safety runs on every accepted prefix. Different digests or application-state references at one command index are failures. Later agreement does not remove an earlier failure.

A shorter valid prefix means that a replica lags. It does not mean that the replica diverged. A lossless gap is an observer-conformance failure. A sampled gap reduces coverage.

Liveness runs only when the named stabilization facts show all these conditions:

- an available quorum;
- a ready consumer lifecycle;
- no active disruptive fault;
- a finite virtual progress horizon.

An active partition can block progress. ChaosControl does not relabel this expected unavailability as a safety failure. Wall-clock delay never decides the semantic result.

## Faults, replay, and reduction

Every evidence campaign includes a no-fault control. Seeded swarm selection retains selected and unexplored features, selected and unexplored fault classes, and declared weights.

A selected fault is not an effect. An effect claim needs both `applied` and `observed` stages plus an admitted BLAKE3 effect-record reference from the fault-outcome rail.

Semantic replay compares operation identities, proposal outcomes, observations, safety prefixes, liveness facts, and the terminal result. The first mismatch denies replay acceptance.

Bounded reduction can remove commands, clients, fault actions, and schedule actions. It keeps a candidate only when the supplied pure predicate preserves the same failure class. A reached attempt bound reports `bound-reached`; it does not claim a minimum reproducer.

## Evidence limits

A receipt binds the profile, build, adapter, observer, mode, dropped-event count, seed, swarm choices, fault outcomes, observations, bounds, verdicts, and replay result.

A passing receipt proves only its declared workload, consumer, observer path, bounds, schedule, and retained facts. It does not prove:

- universal SMR correctness;
- consensus correctness;
- durability or linearizability;
- Byzantine tolerance or security;
- production readiness or release eligibility.

## Validation

Run the focused pure and adapter rails:

```text
cargo test -p chaoscontrol-smr -p chaoscontrol-raft-guest
cargo run -p chaoscontrol-evidence --bin check-smr-chain-fixtures
cargo run -p chaoscontrol-evidence --bin check-evidence-contracts -- --root .
```

The workspace test and Nix rails provide broader regression evidence. KVM results remain scoped to the selected guest image, host capability, profile, and run receipt.
