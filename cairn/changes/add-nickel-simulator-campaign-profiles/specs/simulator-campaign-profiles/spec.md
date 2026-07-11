# Simulator and Campaign Profile Specification

## ADDED Requirements

### Requirement: Configuration ownership is explicit

r[chaoscontrol.simulator_campaign_profiles.ownership_registry] ChaosControl MUST register VM run, in-process simulator, campaign, and finite fault-schedule profiles as Nickel-authored inputs and MUST register progress, traces, checkpoints, outcomes, reports, and receipts as Rust-derived or explicitly excluded artifacts.

#### Scenario: Runtime observation is proposed as authored configuration

- GIVEN a contract change attempts to make completed seeds, observed faults, assertion hits, replay outcomes, or runtime traces Nickel-authored
- WHEN the ownership registry is validated
- THEN validation MUST fail and identify the Rust-derived ownership boundary.

### Requirement: Configuration families share hardened contracts

r[chaoscontrol.simulator_campaign_profiles.shared_contracts] ChaosControl MUST use shared Nickel contracts for exact schema identity, named integer bounds, closed enums, BLAKE3 identities, required legacy digest formats, typed path/reference classes, uniqueness, and deterministic diagnostics.

#### Scenario: Equivalent primitive constraints disagree

- GIVEN two profile families validate the same identity, digest, or positive-budget class differently
- WHEN contract registry validation runs
- THEN validation MUST fail until both use the shared primitive or document an interoperability-specific exception.

### Requirement: VM run profiles reject ambiguous intent

r[chaoscontrol.simulator_campaign_profiles.run_profile] VM run profiles MUST constrain modes, artifact/path reference classes, integer topology and exploration budgets, coverage mode, and raw-log policy before export.

#### Scenario: Broad fields hide an invalid run

- GIVEN a run profile uses an unknown mode, a zero required budget, an unsafe artifact reference, or a coverage address inconsistent with its declared coverage mode
- WHEN Nickel evaluates the profile
- THEN export MUST fail before VM construction.

### Requirement: In-process simulator profiles match the supported boundary

r[chaoscontrol.simulator_campaign_profiles.simulator_profile] In-process simulator profiles MUST validate workload identity, scheduler, virtual clock, RNG, simulated network and disk profiles, schedule reference, artifact bindings, seed, and required scope non-claims against the supported Rust DTO boundary.

#### Scenario: Simulator profile implies unsupported entropy or I/O

- GIVEN a profile selects an unsupported RNG/scheduler, a non-simulated network or disk, an empty artifact set, a malformed schedule digest, or omits the simulator-local non-claim
- WHEN the simulator profile is validated
- THEN export MUST fail and no `SimulatorConfig` MUST be constructed.

### Requirement: Campaign profiles are finite and cross-field coherent

r[chaoscontrol.simulator_campaign_profiles.campaign_profile] Campaign profiles MUST validate non-empty unique seed sets, VM/vCPU topology, scheduling strategy, exploration mode, branch/round/frontier/quantum/bootstrap limits, worker plan, mutation and havoc ranges, coverage mode, scenario reference, logging/metrics policy, output layout, and named resource bounds.

#### Scenario: Campaign fields conflict

- GIVEN a campaign has duplicate seeds, an incompatible vCPU/scheduling combination, an inverted mutation range, an invalid worker plan, implicit blind coverage, or colliding per-seed output identities
- WHEN Nickel evaluates the whole profile
- THEN validation MUST reject it before any campaign thread or VM is started.

### Requirement: Fault schedules are authored as closed finite descriptors

r[chaoscontrol.simulator_campaign_profiles.fault_schedule_profile] Nickel-authored fault schedules MUST use closed descriptor alternatives and MUST validate finite ordering, action-specific fields, VM/link target ranges, partition-set shape, and profile bounds without claiming application or observation.

#### Scenario: Fault target is outside campaign topology

- GIVEN a schedule references a VM or link outside the declared topology or supplies malformed action parameters
- WHEN profile validation runs
- THEN export MUST fail
- AND the failure MUST NOT be represented as a runtime attempted, applied, or observed fault.

### Requirement: Profile projection is explicit and freshness-bound

r[chaoscontrol.simulator_campaign_profiles.projection_boundary] Profile generation MUST be an explicit shell workflow that binds source, imports, contract, evaluator/profile, and deterministic JSON projection identities with BLAKE3, while runtime Rust MUST revalidate external JSON and MUST NOT invoke Nickel in simulator, campaign, or replay hot paths.

#### Scenario: Checked source and runtime JSON diverge

- GIVEN a profile source, imported contract, evaluator identity, or generated JSON changes without a matching generation receipt
- WHEN preparation or package validation runs
- THEN validation MUST fail before runtime config admission with deterministic drift classification.

### Requirement: Profile suites include rejection evidence

r[chaoscontrol.simulator_campaign_profiles.fixtures] Each profile family MUST include positive fixtures and negative fixtures for vocabulary, bounds, identity, path/reference, uniqueness, topology, range, schedule, digest, and scope failures.

#### Scenario: New profile field has no invalid case

- GIVEN a new identity-affecting or safety-relevant field is added to a profile
- WHEN fixture coverage is checked
- THEN the change MUST include a valid example and at least one deterministic rejecting example.

### Requirement: Runtime and evidence claims remain Rust-owned

r[chaoscontrol.simulator_campaign_profiles.runtime_boundary] Passing Nickel profile validation MUST establish only reviewed pre-run shape and declared invariants; Rust MUST retain construction, external-input validation, execution, fault applicability/application/observation, checkpointing, replay, progress, reports, and receipt authority.

#### Scenario: Valid profile is summarized

- GIVEN a profile passes contract and projection parity checks
- WHEN a readiness summary is emitted
- THEN it MUST NOT claim KVM availability, guest correctness, deterministic replay, fault effect, campaign completion, or accepted evidence.
