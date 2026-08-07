## Phase 1: Ownership and field inventory

- [x] [serial] Inventory `run-config.ncl`, `SimulatorConfig`, `ExplorerConfig`, `CampaignConfig`, serializable resume config, CLI defaults, fault descriptors, and every identity-affecting conversion field. r[chaoscontrol.simulator_campaign_profiles.ownership_registry]
- [x] [serial] Extend the evidence contract registry with Nickel-authored run/simulator/campaign/schedule families and Rust-derived progress, trace, outcome, checkpoint, report, and receipt families. r[chaoscontrol.simulator_campaign_profiles.ownership_registry]
- [x] [serial] Add shared exact-schema, integer/bound, enum, identity, digest, path/reference, uniqueness, and diagnostic contracts. r[chaoscontrol.simulator_campaign_profiles.shared_contracts]

## Phase 2: Profile contracts

- [x] [serial] Harden the VM run profile with closed modes, typed artifact/path references, integer budgets, and explicit coverage/log policy. r[chaoscontrol.simulator_campaign_profiles.run_profile]
- [x] [serial] Add the in-process simulator profile for workload, scheduler, virtual clock, RNG, simulated network/disk, schedule reference, artifacts, seed, and scope. r[chaoscontrol.simulator_campaign_profiles.simulator_profile]
- [x] [serial] Add the campaign profile for seed sets, topology, scheduler, exploration, workers, mutation/havoc, coverage, scenario, logging/metrics, output layout, and named resource bounds. r[chaoscontrol.simulator_campaign_profiles.campaign_profile]
- [x] [serial] Add closed finite fault-schedule descriptor alternatives with ordering, target, partition, and action-specific authoring checks. r[chaoscontrol.simulator_campaign_profiles.fault_schedule_profile]

## Phase 3: Checked projection boundary

- [x] [serial] Implement an explicit generate/check shell that binds source, imports, contract, evaluator/profile, and output with BLAKE3 and never runs in simulator, campaign, or replay hot paths. r[chaoscontrol.simulator_campaign_profiles.projection_boundary]
- [x] [serial] Implement pure Rust DTO-to-runtime-config conversion and preserve runtime revalidation for externally supplied JSON. r[chaoscontrol.simulator_campaign_profiles.projection_boundary] r[chaoscontrol.simulator_campaign_profiles.runtime_boundary]
- [x] [parallel] Add positive profiles for VM exploration, in-process simulation, multi-seed campaign, and finite scheduled faults. r[chaoscontrol.simulator_campaign_profiles.fixtures]
- [x] [parallel] Add negative profiles for unknown modes, zero/excessive bounds, duplicate seeds, incompatible topology/scheduling, unordered mutation ranges, implicit blind coverage, unsafe references, out-of-range fault targets, malformed digests, and weakened scope. r[chaoscontrol.simulator_campaign_profiles.fixtures]

## Phase 4: Verification

- [x] [serial] Run Nickel contract/fixture checks, projection freshness, Rust conversion and external-JSON rejection tests, focused simulator/campaign tests, Cairn validation, and proposal/design/tasks gates. r[chaoscontrol.simulator_campaign_profiles.fixtures] r[chaoscontrol.simulator_campaign_profiles.projection_boundary]
- [x] [serial] Document that profile conformance does not establish KVM/guest correctness, deterministic replay, fault application or observation, campaign completion, or evidence acceptance. r[chaoscontrol.simulator_campaign_profiles.runtime_boundary]
