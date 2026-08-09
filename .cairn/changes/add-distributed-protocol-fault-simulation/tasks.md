## Phase 1: Spec foundation

- [x] [serial] Create the Cairn package foundation for the distributed-protocol fault-simulation rail. r[protocol-fault-sim.contract]

## Phase 2: Simulation and fault core

- [x] [serial] Add protocol-simulation config and receipt DTOs for seed, schedule, clock, RNG, protocol, and artifact digests. r[protocol-fault-sim.contract]
- [x] [serial] Add pure deterministic transition and scheduling interfaces with repeatability tests. r[protocol-fault-sim.contract]
- [x] [parallel] Add fault hooks for node loss, message loss, reorder, duplication, and partition. r[protocol-fault-sim.faults]
- [x] [parallel] Add negative nondeterminism fixtures proving failure on unbound entropy or wall-clock use. r[protocol-fault-sim.contract]

## Phase 3: Replay and evidence

- [x] [serial] Emit reproducibility receipts binding config, history, fault schedule, and digests. r[protocol-fault-sim.replay]
- [x] [serial] Prove a single failing schedule reproduces from seed and schedule. r[protocol-fault-sim.replay]
- [x] [serial] Add readiness wording and gates that keep protocol-simulation evidence separate from VMM and in-process evidence. r[protocol-fault-sim.evidence-boundary]

## Phase 4: Verification

- [ ] [serial] Verify with pure simulation tests, fault-cover fixtures, Cairn validation, and Nix checks. r[protocol-fault-sim.replay]
