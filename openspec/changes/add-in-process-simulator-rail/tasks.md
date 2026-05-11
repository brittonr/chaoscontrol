## Phase 1: Spec foundation

- [x] [serial] Create the OpenSpec package for the in-process deterministic simulator gap.

## Phase 2: Simulator core contracts

- [x] [serial] Add simulator config and receipt DTOs for seed, schedule, time, RNG, network, disk, workload, and artifact digests.
- [x] [parallel] Add pure deterministic scheduler/clock/RNG interfaces with repeatability tests.
- [x] [parallel] Add negative nondeterminism fixtures proving receipt/checker failure on unbound entropy or wall-clock use.

## Phase 3: Workload adapter and faults

- [x] [depends:core-contracts] Add a first explicit Rust workload/model adapter for in-process simulation.
- [x] [depends:adapter] Add bounded simulated network/disk/fault hooks sufficient for the first workload.

## Phase 4: Evidence and readiness boundary

- [ ] [depends:simulator-run] Emit reproducibility receipts and summaries for deterministic simulator runs.
- [ ] [depends:receipts] Add readiness wording and gates that keep simulator evidence separate from VMM replay proof and full FoundationDB parity.
- [ ] [depends:verification] Verify with pure simulator tests, negative fixtures, OpenSpec validation, and relevant Nix checks.
