# Tasks

## Phase 1: Baseline and precedence design

- [x] [serial] Record current fault variants, network-fabric direction behavior, stage ledger, and snapshot/replay binding before source changes. r[chaoscontrol.fault_mechanisms.directional_network] r[chaoscontrol.fault_mechanisms.evidence]
- [x] [serial] Inventory the fault enum, planner, scheduler, network queues, and SDK request surfaces that each mechanism will extend. r[chaoscontrol.fault_mechanisms.interactions]
- [x] [serial] Publish the overlap/precedence table, directional link identity, baseline-profile admission, and explicit unsupported cases. r[chaoscontrol.fault_mechanisms.interactions] r[chaoscontrol.fault_mechanisms.nonclaims]

## Phase 2: Directional network and CPU throttle

- [x] [serial] Define pure directional link identity, disruption modes, scoped admission, and network overlap precedence. r[chaoscontrol.fault_mechanisms.directional_network] r[chaoscontrol.fault_mechanisms.interactions]
- [x] [serial] Add directional targets, modes, scoped heal, and the directional effect to the fault enum, planner, and network fabric. r[chaoscontrol.fault_mechanisms.directional_network]
- [x] [serial] Add baseline latency/low-loss profile facts to the directional network profile. r[chaoscontrol.fault_mechanisms.directional_network]
- [x] [serial] Apply directional network faults in the fabric with deterministic directional delays, drops, and counters. r[chaoscontrol.fault_mechanisms.directional_network]
- [x] [serial] Wire directional fault attempts and packet observations into the network stage ledger. r[chaoscontrol.fault_mechanisms.directional_network] r[chaoscontrol.fault_mechanisms.evidence]
- [x] [serial] Add the virtual quantum-budget CPU throttle, exact release, and scheduler enforcement. r[chaoscontrol.fault_mechanisms.cpu_throttle]
- [x] [parallel] Add positive and negative fixtures for direction, modes, scoped heal, queue bounds, budget exhaustion, release, and host-throttle rejection. r[chaoscontrol.fault_mechanisms.fixtures]
  - Evidence: [fixture audit](evidence/2026-11-06-fixture-audit.md). Direction, mode parameters, scoped heal, queue bounds, and throttle clamp/exhaustion/release are covered by existing mechanism and controller tests; host-style targets are unrepresentable and an unnamed process target is rejected by `malformed_pause_and_throttle_identities_are_rejected`.

## Phase 3: Custom effects and pause control

- [x] [serial] Define the typed custom-effect descriptor, version, parameter bounds, and ambient-command rejection. r[chaoscontrol.fault_mechanisms.custom_effect]
- [x] [serial] Admit the Rust-owned custom-effect adapter and wire bounded execution. r[chaoscontrol.fault_mechanisms.custom_effect]
- [x] [serial] Implement seeded custom-effect selection and bounded execution with typed rejection and application-failure paths. r[chaoscontrol.fault_mechanisms.custom_effect]
- [x] [serial] Implement bounded pause-control request admission with exact windows and duplicate rejection. r[chaoscontrol.fault_mechanisms.pause_control]
- [x] [serial] Wire pause control to active-effect continuation and replay binding. r[chaoscontrol.fault_mechanisms.pause_control]
- [x] [parallel] Add negative fixtures for ambient commands, unadmitted adapters, malformed or overlapping pause windows, and out-of-horizon requests. r[chaoscontrol.fault_mechanisms.fixtures] r[chaoscontrol.fault_mechanisms.nonclaims]
  - Evidence: [fixture audit](evidence/2026-11-06-fixture-audit.md). Existing tests reject ambient command strings, missing or invalid adapters, malformed and overlapping pause windows, and over-horizon windows; `malformed_pause_and_throttle_identities_are_rejected` adds empty and over-bound requester identities.

## Phase 4: Interactions and evidence

- [x] [serial] Implement deterministic network overlap precedence. r[chaoscontrol.fault_mechanisms.interactions]
- [x] [serial] Extend overlap and one-active-per-target precedence to the new and existing mechanisms. r[chaoscontrol.fault_mechanisms.interactions]
  - Evidence: [fixture audit](evidence/2026-11-06-fixture-audit.md) and the published table in `docs/fault-surface.md`. Directional windows on one link overlap with most-restrictive precedence and scoped heal; CPU throttle, stall, process fault/restart, clock freeze, memory pressure, and thread pause admit one active effect per target (`overlapping_*` controller tests); pause control suppresses only new selection; custom effects have no target and require an exact admitted adapter identity.
- [x] [serial] Extend the stage ledger and receipt with mechanism identity, effective target, suppression, overlap, and per-stage counts. r[chaoscontrol.fault_mechanisms.evidence]
  - Evidence: validated mechanism receipts and replay-checked bounded control events in `chaoscontrol-fault::evidence`; 155 fault tests pass, including nine receipt/control positive and negative fixtures.
- [x] [parallel] Add snapshot and exact-replay fixtures for each mechanism and for overlapping windows, including drift rejection. r[chaoscontrol.fault_mechanisms.fixtures]
  - Evidence: six CBOR-backed VMM snapshot fixtures and five exact-replay fixtures pass; adapter lifetime and oversized rejected-request replay remain explicit non-claims in the fixture audit.
- [ ] [serial] Exercise one bounded KVM campaign with a directional fault, a CPU throttle, a custom effect, and a pause window in one recorded run. r[chaoscontrol.fault_mechanisms.evidence] r[chaoscontrol.fault_mechanisms.interactions]

## Phase 5: Verification and evidence

- [ ] [serial] Run focused fault, network, VMM, SDK, sim-core, replay, and evidence tests; Clippy with warnings denied; octet; Cairn validation and gates; and relevant Nix checks. r[chaoscontrol.fault_mechanisms.fixtures]
- [x] [serial] Record the bounded KVM receipt, profile and artifact identities, positive and negative results, and remaining non-claims. r[chaoscontrol.fault_mechanisms.evidence] r[chaoscontrol.fault_mechanisms.nonclaims]

## Blocked prerequisite

A single KVM interaction campaign could not be completed through the controller
harness: direct controller rounds do not reach guest setup within the bounded
test window, and the repository's local multi-hypervisor KVM smoke currently
fails the rust-workload replay expectation (`attempts=6`, `bug_count=0`). The
in-process four-mechanism fixture is present, but it is not KVM campaign evidence.
