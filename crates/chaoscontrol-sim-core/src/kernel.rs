use crate::boundary::{
    validate_exchange, BoundaryError, BoundaryState, ExecutionCommand, ExitObservation,
};
use std::fmt;

/// Virtual nanoseconds represented by one simulation tick.
pub const NANOSECONDS_PER_SIMULATION_TICK: u64 = 1_000_000;
/// Maximum VMs represented in one pure round plan.
pub const MAX_SIMULATION_VMS: usize = 256;
/// Maximum guest artifacts bound to one simulation cohort.
pub const MAX_GUEST_ARTIFACTS: usize = 16;
const EVENTS_PER_RUNNING_VM: usize = 2;
const ROUND_FIXED_EVENTS: usize = 2;
/// Maximum events retained in one canonical round trace.
pub const MAX_ROUND_TRACE_EVENTS: usize =
    MAX_SIMULATION_VMS * EVENTS_PER_RUNNING_VM + ROUND_FIXED_EVENTS;
const TRACE_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.sim-core.trace.v1";
const CONFIG_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.sim-core.config.v1";
const ARTIFACT_SET_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.sim-core.artifact-set.v1";

/// Shell-neutral VM state observed before a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreVmStatus {
    Running,
    Paused,
    Crashed,
    Restarting,
    Resuming,
}

/// Explicit input to one deterministic scheduling round.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RoundInput {
    pub current_tick: u64,
    pub seed: u64,
    pub config_id: [u8; 32],
    pub guest_artifact_ids: Vec<[u8; 32]>,
    pub vm_statuses: Vec<CoreVmStatus>,
    pub exit_budget: u64,
}

/// Pure plan produced before any machine effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundPlan {
    pub next_tick: u64,
    pub virtual_time_ns: u64,
    pub boundary_state: BoundaryState,
    pub commands: Vec<ExecutionCommand>,
    pub inactive_vms: Vec<usize>,
}

/// Shell result for one planned VM command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundObservation {
    pub observation: ExitObservation,
}

/// One canonical decision or accepted observation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CanonicalEvent {
    RoundPlanned {
        tick: u64,
        virtual_time_ns: u64,
        seed: u64,
        config_id: [u8; 32],
        guest_artifact_set_id: [u8; 32],
    },
    VmScheduled {
        sequence: u64,
        vm_index: usize,
        exit_budget: u64,
    },
    VmInactive {
        vm_index: usize,
        status: CoreVmStatus,
    },
    VmObserved {
        sequence: u64,
        vm_index: usize,
        exits: u64,
        halted: bool,
    },
    RoundCompleted {
        tick: u64,
    },
}

/// Bounded canonical trace for one pure round.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CanonicalTrace {
    pub events: Vec<CanonicalEvent>,
}

impl CanonicalTrace {
    /// Domain-separated BLAKE3 identity of the canonical event sequence.
    pub fn identity(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(TRACE_IDENTITY_DOMAIN);
        for event in &self.events {
            hash_event(&mut hasher, event);
        }
        *hasher.finalize().as_bytes()
    }
}

/// Pure simulation-kernel failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulationKernelError {
    NoVirtualMachines,
    TooManyVirtualMachines { found: usize, maximum: usize },
    InvalidExitBudget,
    TooManyGuestArtifacts { found: usize, maximum: usize },
    TickExhausted,
    VirtualTimeExhausted,
    SequenceExhausted,
    ObservationCountMismatch { expected: usize, found: usize },
    UnexpectedObservation { index: usize },
    TraceCapacityExceeded { found: usize, maximum: usize },
    Boundary(BoundaryError),
}

impl fmt::Display for SimulationKernelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SimulationKernelError {}

impl From<BoundaryError> for SimulationKernelError {
    fn from(error: BoundaryError) -> Self {
        Self::Boundary(error)
    }
}

/// Select one deterministic round without machine access.
pub fn simulation_config_identity(vm_count: usize, seed: u64, exit_budget: u64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CONFIG_IDENTITY_DOMAIN);
    hasher.update(&vm_count.to_le_bytes());
    hasher.update(&seed.to_le_bytes());
    hasher.update(&exit_budget.to_le_bytes());
    *hasher.finalize().as_bytes()
}

pub fn guest_artifact_set_identity(artifact_ids: &[[u8; 32]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(ARTIFACT_SET_IDENTITY_DOMAIN);
    hasher.update(&artifact_ids.len().to_le_bytes());
    for artifact_id in artifact_ids {
        hasher.update(artifact_id);
    }
    *hasher.finalize().as_bytes()
}

pub fn plan_round(input: &RoundInput) -> Result<RoundPlan, SimulationKernelError> {
    if input.vm_statuses.is_empty() {
        return Err(SimulationKernelError::NoVirtualMachines);
    }
    if input.vm_statuses.len() > MAX_SIMULATION_VMS {
        return Err(SimulationKernelError::TooManyVirtualMachines {
            found: input.vm_statuses.len(),
            maximum: MAX_SIMULATION_VMS,
        });
    }
    if input.exit_budget == 0 {
        return Err(SimulationKernelError::InvalidExitBudget);
    }
    if input.guest_artifact_ids.len() > MAX_GUEST_ARTIFACTS {
        return Err(SimulationKernelError::TooManyGuestArtifacts {
            found: input.guest_artifact_ids.len(),
            maximum: MAX_GUEST_ARTIFACTS,
        });
    }
    let expected_config_id =
        simulation_config_identity(input.vm_statuses.len(), input.seed, input.exit_budget);
    if input.config_id != expected_config_id {
        return Err(SimulationKernelError::Boundary(
            BoundaryError::ObservationMismatch { field: "config_id" },
        ));
    }
    let next_tick = input
        .current_tick
        .checked_add(1)
        .ok_or(SimulationKernelError::TickExhausted)?;
    let virtual_time_ns = next_tick
        .checked_mul(NANOSECONDS_PER_SIMULATION_TICK)
        .ok_or(SimulationKernelError::VirtualTimeExhausted)?;

    let mut commands = Vec::new();
    let mut inactive_vms = Vec::new();
    for (vm_index, status) in input.vm_statuses.iter().copied().enumerate() {
        if status == CoreVmStatus::Running {
            let sequence = u64::try_from(commands.len())
                .map_err(|_| SimulationKernelError::SequenceExhausted)?;
            commands.push(ExecutionCommand::RunVcpu {
                sequence,
                vm_index,
                vcpu_index: 0,
                exit_budget: input.exit_budget,
            });
        } else {
            inactive_vms.push(vm_index);
        }
    }
    Ok(RoundPlan {
        next_tick,
        virtual_time_ns,
        boundary_state: BoundaryState {
            next_sequence: 0,
            vm_count: input.vm_statuses.len(),
        },
        commands,
        inactive_vms,
    })
}

/// Validate shell observations and emit a canonical round trace.
pub fn complete_round(
    input: &RoundInput,
    plan: RoundPlan,
    observations: &[RoundObservation],
) -> Result<CanonicalTrace, SimulationKernelError> {
    let expected = plan.commands.len();
    if observations.len() != expected {
        return Err(SimulationKernelError::ObservationCountMismatch {
            expected,
            found: observations.len(),
        });
    }
    let mut events = Vec::with_capacity(
        MAX_ROUND_TRACE_EVENTS.min(expected + input.vm_statuses.len() + ROUND_FIXED_EVENTS),
    );
    events.push(CanonicalEvent::RoundPlanned {
        tick: plan.next_tick,
        virtual_time_ns: plan.virtual_time_ns,
        seed: input.seed,
        config_id: input.config_id,
        guest_artifact_set_id: guest_artifact_set_identity(&input.guest_artifact_ids),
    });
    let mut boundary_state = plan.boundary_state;
    for (index, (command, observed)) in plan
        .commands
        .into_iter()
        .zip(observations.iter())
        .enumerate()
    {
        let (sequence, vm_index, exit_budget) = match &command {
            ExecutionCommand::RunVcpu {
                sequence,
                vm_index,
                exit_budget,
                ..
            } => (*sequence, *vm_index, *exit_budget),
            _ => return Err(SimulationKernelError::UnexpectedObservation { index }),
        };
        events.push(CanonicalEvent::VmScheduled {
            sequence,
            vm_index,
            exit_budget,
        });
        let validated = validate_exchange(boundary_state, command, observed.observation.clone())?;
        boundary_state = validated.next_state;
        let ExitObservation::VcpuCompleted {
            sequence,
            vm_index,
            exits,
            halted,
        } = observed.observation
        else {
            return Err(SimulationKernelError::UnexpectedObservation { index });
        };
        events.push(CanonicalEvent::VmObserved {
            sequence,
            vm_index,
            exits,
            halted,
        });
    }
    for vm_index in plan.inactive_vms {
        events.push(CanonicalEvent::VmInactive {
            vm_index,
            status: input.vm_statuses[vm_index],
        });
    }
    events.push(CanonicalEvent::RoundCompleted {
        tick: plan.next_tick,
    });
    if events.len() > MAX_ROUND_TRACE_EVENTS {
        return Err(SimulationKernelError::TraceCapacityExceeded {
            found: events.len(),
            maximum: MAX_ROUND_TRACE_EVENTS,
        });
    }
    Ok(CanonicalTrace { events })
}

fn hash_event(hasher: &mut blake3::Hasher, event: &CanonicalEvent) {
    match event {
        CanonicalEvent::RoundPlanned {
            tick,
            virtual_time_ns,
            seed,
            config_id,
            guest_artifact_set_id,
        } => {
            hasher.update(b"round-planned");
            hasher.update(&tick.to_le_bytes());
            hasher.update(&virtual_time_ns.to_le_bytes());
            hasher.update(&seed.to_le_bytes());
            hasher.update(config_id);
            hasher.update(guest_artifact_set_id);
        }
        CanonicalEvent::VmScheduled {
            sequence,
            vm_index,
            exit_budget,
        } => {
            hasher.update(b"vm-scheduled");
            hasher.update(&sequence.to_le_bytes());
            hasher.update(&vm_index.to_le_bytes());
            hasher.update(&exit_budget.to_le_bytes());
        }
        CanonicalEvent::VmInactive { vm_index, status } => {
            hasher.update(b"vm-inactive");
            hasher.update(&vm_index.to_le_bytes());
            hasher.update(&[*status as u8]);
        }
        CanonicalEvent::VmObserved {
            sequence,
            vm_index,
            exits,
            halted,
        } => {
            hasher.update(b"vm-observed");
            hasher.update(&sequence.to_le_bytes());
            hasher.update(&vm_index.to_le_bytes());
            hasher.update(&exits.to_le_bytes());
            hasher.update(&[u8::from(*halted)]);
        }
        CanonicalEvent::RoundCompleted { tick } => {
            hasher.update(b"round-completed");
            hasher.update(&tick.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TICK: u64 = 4;
    const TEST_EXIT_BUDGET: u64 = 3;
    const TEST_SEED: u64 = 7;
    const CHANGED_SEED: u64 = TEST_SEED + 1;
    const TEST_ARTIFACT_BYTE: u8 = 9;
    const CHANGED_ARTIFACT_BYTE: u8 = TEST_ARTIFACT_BYTE + 1;
    const BLAKE3_BYTES: usize = 32;

    fn input() -> RoundInput {
        let vm_statuses = vec![CoreVmStatus::Running, CoreVmStatus::Paused];
        RoundInput {
            current_tick: TEST_TICK,
            seed: TEST_SEED,
            config_id: simulation_config_identity(vm_statuses.len(), TEST_SEED, TEST_EXIT_BUDGET),
            guest_artifact_ids: vec![[TEST_ARTIFACT_BYTE; BLAKE3_BYTES]],
            vm_statuses,
            exit_budget: TEST_EXIT_BUDGET,
        }
    }

    fn observations() -> Vec<RoundObservation> {
        vec![RoundObservation {
            observation: ExitObservation::VcpuCompleted {
                sequence: 0,
                vm_index: 0,
                exits: TEST_EXIT_BUDGET,
                halted: false,
            },
        }]
    }

    #[test]
    fn fixed_input_produces_identical_trace_and_identity() {
        let first =
            complete_round(&input(), plan_round(&input()).unwrap(), &observations()).unwrap();
        let second =
            complete_round(&input(), plan_round(&input()).unwrap(), &observations()).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.identity(), second.identity());
    }

    #[test]
    fn changed_seed_config_or_artifact_has_a_first_trace_divergence() {
        let baseline_input = input();
        let baseline = complete_round(
            &baseline_input,
            plan_round(&baseline_input).unwrap(),
            &observations(),
        )
        .unwrap();

        let mut changed_seed = input();
        changed_seed.seed = CHANGED_SEED;
        changed_seed.config_id = simulation_config_identity(
            changed_seed.vm_statuses.len(),
            changed_seed.seed,
            changed_seed.exit_budget,
        );

        let mut changed_config = input();
        changed_config.exit_budget += 1;
        changed_config.config_id = simulation_config_identity(
            changed_config.vm_statuses.len(),
            changed_config.seed,
            changed_config.exit_budget,
        );

        let mut changed_artifact = input();
        changed_artifact.guest_artifact_ids = vec![[CHANGED_ARTIFACT_BYTE; BLAKE3_BYTES]];

        for changed in [changed_seed, changed_config, changed_artifact] {
            let candidate =
                complete_round(&changed, plan_round(&changed).unwrap(), &observations()).unwrap();
            let first_difference = baseline
                .events
                .iter()
                .zip(&candidate.events)
                .position(|(left, right)| left != right);
            assert_eq!(first_difference, Some(0));
            assert_ne!(baseline.identity(), candidate.identity());
        }
    }

    #[test]
    fn missing_observation_is_rejected() {
        let error = complete_round(&input(), plan_round(&input()).unwrap(), &[]).unwrap_err();
        assert_eq!(
            error,
            SimulationKernelError::ObservationCountMismatch {
                expected: 1,
                found: 0,
            }
        );
    }

    #[test]
    fn tick_and_virtual_time_overflow_fail_closed() {
        let tick_overflow = RoundInput {
            current_tick: u64::MAX,
            ..input()
        };
        assert_eq!(
            plan_round(&tick_overflow),
            Err(SimulationKernelError::TickExhausted)
        );

        let virtual_time_tick = u64::MAX / NANOSECONDS_PER_SIMULATION_TICK;
        let time_overflow = RoundInput {
            current_tick: virtual_time_tick,
            ..input()
        };
        assert_eq!(
            plan_round(&time_overflow),
            Err(SimulationKernelError::VirtualTimeExhausted)
        );
    }
}
