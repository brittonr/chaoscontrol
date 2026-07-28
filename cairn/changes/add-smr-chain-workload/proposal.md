## Why

ChaosControl has Raft dogfood, point assertions, fault exploration, and replay evidence. It does not have an implementation-neutral workload for state-machine replication.

The current Raft guest can expose protocol defects in its selected model. It does not produce a reusable command-indexed history for arbitrary Rust SMR consumers.

A small chain workload can expose replica divergence, lost committed transitions, duplicate application, rollback, and stalled recovery. It can also preserve the boundary between safety and liveness.

## What Changes

- Add a versioned, bounded SMR chain-workload profile for Rust guests and local simulator adapters.
- Define canonical domain-separated BLAKE3 genesis and transitions over initial state, prior digest, command index, and framed command bytes.
- Add a pure history validator for replica observations, proposal outcomes, safety, and conditional liveness.
- Treat timeouts and other indefinite proposal outcomes as unknown execution results instead of definite rejection.
- Add no-fault controls, deterministic fault campaigns, swarm feature subsets, replay, and causal reduction inputs.
- Emit typed workload evidence that binds build, profile, seed, schedule, fault outcomes, observations, verdicts, and replay scope.
- Provide one first-party Raft guest integration without making the workload depend on Raft internals.

## Impact

- **Files**: a new SMR workload core, Rust guest adapter, Raft guest integration, profile contracts, report and evidence models, fixtures, docs, and Nix checks.
- **Testing**: pure transition properties, positive convergence, deliberate divergence, duplicate application, malformed history, partition recovery, stalled progress, replay, and bounded KVM smoke tests.
- **Compatibility**: the workload is additive. Existing Raft assertion and replay classes remain unchanged.
- **Dependencies**: observed-fault claims depend on `verify-fault-application-outcomes`. Snapshot-backed replay claims depend on the existing replay-readiness contracts.
- **Claims**: a pass supports only the declared SMR workload, bounds, fault profile, and observation path. It does not prove consensus correctness or production readiness.
