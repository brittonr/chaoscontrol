use chaoscontrol_sim_core::{
    complete_round, guest_artifact_set_identity, plan_round, simulation_config_identity,
    BoundaryState, CanonicalEvent, CanonicalTrace, CoreVmStatus, ExecutionCommand, ExitObservation,
    RoundInput, RoundObservation, RoundPlan,
};

const TEST_TICK: u64 = 11;
const TEST_SEED: u64 = 23;
const TEST_EXIT_BUDGET: u64 = 5;
const VIRTUAL_TICK_NS: u64 = 1_000_000;
const ARTIFACT_BYTE: u8 = 0xA5;
const CHANGED_ARTIFACT_BYTE: u8 = 0x5A;
const BLAKE3_BYTES: usize = 32;

fn fixed_input() -> RoundInput {
    let vm_statuses = vec![CoreVmStatus::Running, CoreVmStatus::Paused];
    RoundInput {
        current_tick: TEST_TICK,
        seed: TEST_SEED,
        config_id: simulation_config_identity(vm_statuses.len(), TEST_SEED, TEST_EXIT_BUDGET),
        guest_artifact_ids: vec![[ARTIFACT_BYTE; BLAKE3_BYTES]],
        vm_statuses,
        exit_budget: TEST_EXIT_BUDGET,
    }
}

fn fixed_observations() -> Vec<RoundObservation> {
    vec![RoundObservation {
        observation: ExitObservation::VcpuCompleted {
            sequence: 0,
            vm_index: 0,
            exits: TEST_EXIT_BUDGET,
            halted: false,
        },
    }]
}

fn pre_split_plan(input: &RoundInput) -> RoundPlan {
    let next_tick = input.current_tick + 1;
    RoundPlan {
        next_tick,
        virtual_time_ns: next_tick * VIRTUAL_TICK_NS,
        boundary_state: BoundaryState {
            next_sequence: 0,
            vm_count: input.vm_statuses.len(),
        },
        commands: vec![ExecutionCommand::RunVcpu {
            sequence: 0,
            vm_index: 0,
            vcpu_index: 0,
            exit_budget: input.exit_budget,
        }],
        inactive_vms: vec![1],
    }
}

fn pre_split_trace(input: &RoundInput) -> CanonicalTrace {
    let next_tick = input.current_tick + 1;
    CanonicalTrace {
        events: vec![
            CanonicalEvent::RoundPlanned {
                tick: next_tick,
                virtual_time_ns: next_tick * VIRTUAL_TICK_NS,
                seed: input.seed,
                config_id: input.config_id,
                guest_artifact_set_id: guest_artifact_set_identity(&input.guest_artifact_ids),
            },
            CanonicalEvent::VmScheduled {
                sequence: 0,
                vm_index: 0,
                exit_budget: input.exit_budget,
            },
            CanonicalEvent::VmObserved {
                sequence: 0,
                vm_index: 0,
                exits: TEST_EXIT_BUDGET,
                halted: false,
            },
            CanonicalEvent::VmInactive {
                vm_index: 1,
                status: CoreVmStatus::Paused,
            },
            CanonicalEvent::RoundCompleted { tick: next_tick },
        ],
    }
}

#[test]
fn extracted_core_matches_the_pre_split_round_and_trace() {
    let input = fixed_input();
    let plan = plan_round(&input).unwrap();
    assert_eq!(plan, pre_split_plan(&input));
    let trace = complete_round(&input, plan, &fixed_observations()).unwrap();
    let expected = pre_split_trace(&input);
    assert_eq!(trace, expected);
    assert_eq!(trace.identity(), expected.identity());
}

#[test]
fn changed_seed_config_or_artifact_diverges_at_the_first_event() {
    let baseline_input = fixed_input();
    let baseline = pre_split_trace(&baseline_input);

    let mut changed_seed = fixed_input();
    changed_seed.seed += 1;
    changed_seed.config_id = simulation_config_identity(
        changed_seed.vm_statuses.len(),
        changed_seed.seed,
        changed_seed.exit_budget,
    );

    let mut changed_config = fixed_input();
    changed_config.exit_budget += 1;
    changed_config.config_id = simulation_config_identity(
        changed_config.vm_statuses.len(),
        changed_config.seed,
        changed_config.exit_budget,
    );

    let mut changed_artifact = fixed_input();
    changed_artifact.guest_artifact_ids = vec![[CHANGED_ARTIFACT_BYTE; BLAKE3_BYTES]];

    for changed in [changed_seed, changed_config, changed_artifact] {
        let candidate = complete_round(
            &changed,
            plan_round(&changed).unwrap(),
            &fixed_observations(),
        )
        .unwrap();
        let first_difference = baseline
            .events
            .iter()
            .zip(&candidate.events)
            .position(|(left, right)| left != right);
        assert_eq!(first_difference, Some(0));
    }
}
