# Tasks

## Phase 1: Baseline and precedence design

- [x] [serial] Record current fault variants, network-fabric direction behavior, stage ledger, and snapshot/replay binding before source changes. r[chaoscontrol.fault_mechanisms.directional_network] r[chaoscontrol.fault_mechanisms.evidence]
- [x] [serial] Inventory the fault enum, planner, scheduler, network queues, and SDK request surfaces that each mechanism will extend. r[chaoscontrol.fault_mechanisms.interactions]
- [ ] [serial] Publish the overlap/precedence table, directional link identity, baseline-profile admission, and explicit unsupported cases. r[chaoscontrol.fault_mechanisms.interactions] r[chaoscontrol.fault_mechanisms.nonclaims]

## Phase 2: Directional network and CPU throttle

- [x] [serial] Define pure directional link identity, disruption modes, scoped admission, and network overlap precedence. r[chaoscontrol.fault_mechanisms.directional_network] r[chaoscontrol.fault_mechanisms.interactions]
- [x] [serial] Add directional targets, modes, scoped heal, and the directional effect to the fault enum, planner, and network fabric. r[chaoscontrol.fault_mechanisms.directional_network]
- [x] [serial] Add baseline latency/low-loss profile facts to the directional network profile. r[chaoscontrol.fault_mechanisms.directional_network]
- [x] [serial] Apply directional network faults in the fabric with deterministic directional delays, drops, and counters. r[chaoscontrol.fault_mechanisms.directional_network]
- [x] [serial] Wire directional fault attempts and packet observations into the network stage ledger. r[chaoscontrol.fault_mechanisms.directional_network] r[chaoscontrol.fault_mechanisms.evidence]
- [x] [serial] Add the virtual quantum-budget CPU throttle, exact release, and scheduler enforcement. r[chaoscontrol.fault_mechanisms.cpu_throttle]
- [ ] [parallel] Add positive and negative fixtures for direction, modes, scoped heal, queue bounds, budget exhaustion, release, and host-throttle rejection. r[chaoscontrol.fault_mechanisms.fixtures]

## Phase 3: Custom effects and pause control

- [x] [serial] Define the typed custom-effect descriptor, version, parameter bounds, and ambient-command rejection. r[chaoscontrol.fault_mechanisms.custom_effect]
- [ ] [serial] Admit the Rust-owned custom-effect adapter and wire bounded execution. r[chaoscontrol.fault_mechanisms.custom_effect]
- [ ] [serial] Implement seeded custom-effect selection and bounded execution with typed rejection and application-failure paths. r[chaoscontrol.fault_mechanisms.custom_effect]
- [x] [serial] Implement bounded pause-control request admission with exact windows and duplicate rejection. r[chaoscontrol.fault_mechanisms.pause_control]
- [x] [serial] Wire pause control to active-effect continuation and replay binding. r[chaoscontrol.fault_mechanisms.pause_control]
- [ ] [parallel] Add negative fixtures for ambient commands, unadmitted adapters, malformed or overlapping pause windows, and out-of-horizon requests. r[chaoscontrol.fault_mechanisms.fixtures] r[chaoscontrol.fault_mechanisms.nonclaims]

## Phase 4: Interactions and evidence

- [x] [serial] Implement deterministic network overlap precedence. r[chaoscontrol.fault_mechanisms.interactions]
- [ ] [serial] Extend overlap and one-active-per-target precedence to the new and existing mechanisms. r[chaoscontrol.fault_mechanisms.interactions]
- [ ] [serial] Extend the stage ledger and receipt with mechanism identity, effective target, suppression, overlap, and per-stage counts. r[chaoscontrol.fault_mechanisms.evidence]
- [ ] [parallel] Add snapshot and exact-replay fixtures for each mechanism and for overlapping windows, including drift rejection. r[chaoscontrol.fault_mechanisms.fixtures]
- [ ] [serial] Exercise one bounded KVM campaign with a directional fault, a CPU throttle, a custom effect, and a pause window in one recorded run. r[chaoscontrol.fault_mechanisms.evidence] r[chaoscontrol.fault_mechanisms.interactions]

## Phase 5: Verification and evidence

- [ ] [serial] Run focused fault, network, VMM, SDK, sim-core, replay, and evidence tests; Clippy with warnings denied; octet; Cairn validation and gates; and relevant Nix checks. r[chaoscontrol.fault_mechanisms.fixtures]
- [x] [serial] Record the bounded KVM receipt, profile and artifact identities, positive and negative results, and remaining non-claims. r[chaoscontrol.fault_mechanisms.evidence] r[chaoscontrol.fault_mechanisms.nonclaims]
