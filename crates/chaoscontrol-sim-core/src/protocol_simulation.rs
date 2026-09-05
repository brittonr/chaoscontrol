//! Pure data-transfer types for adapter-based protocol simulation.
//!
//! These types bind deterministic run inputs and receipt facts. They do not
//! read configuration, execute a protocol, write receipts, or claim VM replay.

use serde::{Deserialize, Serialize};

use std::fmt;

/// Schema for an admitted adapter-based protocol-simulation run configuration.
pub const PROTOCOL_SIMULATION_CONFIG_SCHEMA: &str = "chaoscontrol.protocol-simulation-config.v1";
/// Schema for a runtime-derived adapter-based protocol-simulation receipt.
pub const PROTOCOL_SIMULATION_RECEIPT_SCHEMA: &str = "chaoscontrol.protocol-simulation-receipt.v1";

/// Evidence class for this rail. It is separate from VM and in-process evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolSimulationEvidenceClass {
    #[serde(rename = "adapter-protocol-simulation")]
    AdapterProtocolSimulation,
}

/// r[protocol-fault-sim.contract]
/// r[protocol-fault-sim.contract.config]
/// Complete deterministic input binding for one protocol-simulation run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolSimulationConfig {
    pub schema: String,
    pub seed: u64,
    pub schedule: ProtocolScheduleRef,
    pub scheduler: ProtocolSchedulerPolicy,
    pub virtual_clock: ProtocolVirtualClockPolicy,
    pub rng: ProtocolRngPolicy,
    pub protocol: ProtocolIdentity,
    pub artifact_digests: std::collections::BTreeMap<String, String>,
}

/// Identity of the exact deterministic schedule supplied to the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolScheduleRef {
    pub schedule_id: String,
    pub digest: String,
}

/// Deterministic scheduler policy selected for the protocol adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolSchedulerPolicy {
    pub policy_id: String,
    pub maximum_steps: u64,
}

/// Virtual clock policy selected for the protocol adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolVirtualClockPolicy {
    pub policy_id: String,
    pub initial_tick: u64,
    pub tick_quantum: u64,
}

/// Deterministic random-number policy selected for the protocol adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRngPolicy {
    pub algorithm: String,
    pub seed_derivation: String,
}

/// Identity of the protocol and the adapter that supplies its transitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolIdentity {
    pub protocol_id: String,
    pub protocol_version: String,
    pub adapter_id: String,
    pub adapter_version: String,
}

/// Runtime-derived receipt facts for one bounded protocol-simulation run.
///
/// The shell emits this DTO. The embedded configuration makes the seed,
/// schedule, clock, RNG, protocol, adapter, and artifact bindings explicit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolSimulationReceipt {
    pub schema: String,
    pub run_id: String,
    pub config_digest: String,
    pub config: ProtocolSimulationConfig,
    pub fault_schedule_digest: String,
    pub history_digest: String,
    pub output_digest: String,
    pub evidence_class: ProtocolSimulationEvidenceClass,
}

/// One deterministic adapter input at an exact protocol-simulation step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolTransitionInput<I> {
    pub step: u64,
    pub virtual_tick: u64,
    pub event: I,
}

/// One effect request surfaced by a protocol adapter.
///
/// Admitted requests bind effects to simulation-owned sources. The legacy host
/// variants exist only so validation and negative fixtures can fail closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProtocolEffectRequest {
    VirtualClockRead,
    SeededRandomRead { stream_id: String },
    RegisteredIo { hook_id: String },
    DeclaredFault { hook_id: String },
    HostWallClockRead,
    HostRandomRead,
    UnregisteredExternalIo { operation: String },
}

/// Forbidden source reported for unsupported protocol-simulation evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolUnboundNondeterminism {
    HostWallClock,
    HostRandomness,
    UnregisteredExternalIo,
}

/// Fail-closed effect-request validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolEffectValidationError {
    EmptyBinding {
        request_index: usize,
        field: &'static str,
    },
    UnboundNondeterminism {
        request_index: usize,
        source: ProtocolUnboundNondeterminism,
    },
}

impl fmt::Display for ProtocolEffectValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ProtocolEffectValidationError {}

/// r[protocol-fault-sim.contract.nondeterminism-fails]
/// Reject effect requests that bypass simulation-owned deterministic sources.
pub fn validate_protocol_effect_requests(
    requests: &[ProtocolEffectRequest],
) -> Result<(), ProtocolEffectValidationError> {
    for (request_index, request) in requests.iter().enumerate() {
        match request {
            ProtocolEffectRequest::VirtualClockRead => {}
            ProtocolEffectRequest::SeededRandomRead { stream_id } => {
                require_effect_binding(request_index, "stream_id", stream_id)?;
            }
            ProtocolEffectRequest::RegisteredIo { hook_id }
            | ProtocolEffectRequest::DeclaredFault { hook_id } => {
                require_effect_binding(request_index, "hook_id", hook_id)?;
            }
            ProtocolEffectRequest::HostWallClockRead => {
                return Err(ProtocolEffectValidationError::UnboundNondeterminism {
                    request_index,
                    source: ProtocolUnboundNondeterminism::HostWallClock,
                });
            }
            ProtocolEffectRequest::HostRandomRead => {
                return Err(ProtocolEffectValidationError::UnboundNondeterminism {
                    request_index,
                    source: ProtocolUnboundNondeterminism::HostRandomness,
                });
            }
            ProtocolEffectRequest::UnregisteredExternalIo { .. } => {
                return Err(ProtocolEffectValidationError::UnboundNondeterminism {
                    request_index,
                    source: ProtocolUnboundNondeterminism::UnregisteredExternalIo,
                });
            }
        }
    }
    Ok(())
}

fn require_effect_binding(
    request_index: usize,
    field: &'static str,
    value: &str,
) -> Result<(), ProtocolEffectValidationError> {
    if value.is_empty() {
        return Err(ProtocolEffectValidationError::EmptyBinding {
            request_index,
            field,
        });
    }
    Ok(())
}

/// A typed protocol fact emitted by a deterministic adapter transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolFact {
    pub kind: ProtocolFactKind,
    pub subject: String,
    pub value_ref: String,
}

/// Supported fact classes for ownership, replication, and reacquisition adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolFactKind {
    Ownership,
    Replication,
    Reacquisition,
}

/// Pure result of one adapter transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolTransition<S, E> {
    pub next_state: S,
    pub emitted_events: Vec<E>,
    pub facts: Vec<ProtocolFact>,
    pub effect_requests: Vec<ProtocolEffectRequest>,
}

/// r[protocol-fault-sim.contract]
/// Pure adapter boundary for one supported distributed protocol.
///
/// Implementations must depend only on `state` and `input`. They must not read
/// clocks, randomness, files, environment variables, or external transports.
/// They must surface each requested effect in `effect_requests`. Validation is
/// bounded to those surfaced requests and does not detect a hidden side effect.
pub trait ProtocolAdapter {
    type State: Clone + PartialEq + Eq;
    type Input: Clone + PartialEq + Eq;
    type EmittedEvent: Clone + PartialEq + Eq;
    type Error;

    fn transition(
        &self,
        state: &Self::State,
        input: &ProtocolTransitionInput<Self::Input>,
    ) -> Result<ProtocolTransition<Self::State, Self::EmittedEvent>, Self::Error>;
}

/// Failure from the bounded two-run adapter repeatability check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolTransitionCheckError<E> {
    FirstRun(E),
    FirstRunEffect(ProtocolEffectValidationError),
    SecondRun(E),
    SecondRunEffect(ProtocolEffectValidationError),
    DivergentOutput,
}

/// Result of one bounded adapter repeatability check.
pub type ProtocolTransitionCheckResult<A> = Result<
    ProtocolTransition<<A as ProtocolAdapter>::State, <A as ProtocolAdapter>::EmittedEvent>,
    ProtocolTransitionCheckError<<A as ProtocolAdapter>::Error>,
>;

/// Run one transition twice and reject an adapter that changes its output.
///
/// This check rejects surfaced host effects and detects visible nondeterminism
/// for the supplied state and input. It does not detect hidden adapter effects
/// or prove that an adapter is deterministic for all possible inputs.
pub fn verify_repeatable_transition<A: ProtocolAdapter>(
    adapter: &A,
    state: &A::State,
    input: &ProtocolTransitionInput<A::Input>,
) -> ProtocolTransitionCheckResult<A> {
    let first = adapter
        .transition(state, input)
        .map_err(ProtocolTransitionCheckError::FirstRun)?;
    validate_protocol_effect_requests(&first.effect_requests)
        .map_err(ProtocolTransitionCheckError::FirstRunEffect)?;
    let second = adapter
        .transition(state, input)
        .map_err(ProtocolTransitionCheckError::SecondRun)?;
    validate_protocol_effect_requests(&second.effect_requests)
        .map_err(ProtocolTransitionCheckError::SecondRunEffect)?;
    if first != second {
        return Err(ProtocolTransitionCheckError::DivergentOutput);
    }
    Ok(first)
}

/// One protocol event admitted to the deterministic scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingProtocolEvent<E> {
    pub sequence: u64,
    pub ready_tick: u64,
    pub target: String,
    pub event: E,
}

/// Pure scheduler state. The shell retains clocks, queues, and transport effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolEventSchedulerState<E> {
    pub next_step: u64,
    pub current_tick: u64,
    pub policy: ProtocolSchedulerPolicy,
    pub pending: Vec<PendingProtocolEvent<E>>,
}

/// One deterministic scheduler selection and its resulting pure state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolScheduleDecision<E> {
    pub selected: PendingProtocolEvent<E>,
    pub next_state: ProtocolEventSchedulerState<E>,
}

/// Fail-closed scheduler errors for malformed or exhausted state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolScheduleError {
    InvalidMaximumSteps,
    StepLimitReached { completed: u64, maximum: u64 },
    PendingEventLimitExceeded { found: usize, maximum: u64 },
    NoPendingEvents,
    DuplicateSequence { sequence: u64 },
    EmptyTarget { sequence: u64 },
    StepCounterExhausted,
}

impl fmt::Display for ProtocolScheduleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ProtocolScheduleError {}

/// r[protocol-fault-sim.contract]
/// Select the earliest event by ready tick, sequence, and target.
pub fn schedule_next_protocol_event<E: Clone>(
    state: &ProtocolEventSchedulerState<E>,
) -> Result<ProtocolScheduleDecision<E>, ProtocolScheduleError> {
    let maximum_steps = state.policy.maximum_steps;
    if maximum_steps == 0 {
        return Err(ProtocolScheduleError::InvalidMaximumSteps);
    }
    if state.next_step >= maximum_steps {
        return Err(ProtocolScheduleError::StepLimitReached {
            completed: state.next_step,
            maximum: maximum_steps,
        });
    }
    if u64::try_from(state.pending.len()).unwrap_or(u64::MAX) > maximum_steps {
        return Err(ProtocolScheduleError::PendingEventLimitExceeded {
            found: state.pending.len(),
            maximum: maximum_steps,
        });
    }
    if state.pending.is_empty() {
        return Err(ProtocolScheduleError::NoPendingEvents);
    }

    let mut sequences = std::collections::BTreeSet::new();
    for pending in &state.pending {
        if pending.target.is_empty() {
            return Err(ProtocolScheduleError::EmptyTarget {
                sequence: pending.sequence,
            });
        }
        if !sequences.insert(pending.sequence) {
            return Err(ProtocolScheduleError::DuplicateSequence {
                sequence: pending.sequence,
            });
        }
    }

    let selected_index = state
        .pending
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            (left.ready_tick, left.sequence, left.target.as_str()).cmp(&(
                right.ready_tick,
                right.sequence,
                right.target.as_str(),
            ))
        })
        .map(|(index, _)| index)
        .ok_or(ProtocolScheduleError::NoPendingEvents)?;
    let next_step = state
        .next_step
        .checked_add(1)
        .ok_or(ProtocolScheduleError::StepCounterExhausted)?;
    let mut pending = state.pending.clone();
    let selected = pending.remove(selected_index);
    let next_state = ProtocolEventSchedulerState {
        next_step,
        current_tick: state.current_tick.max(selected.ready_tick),
        policy: state.policy.clone(),
        pending,
    };
    Ok(ProtocolScheduleDecision {
        selected,
        next_state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SEED: u64 = 41;
    const TEST_MAXIMUM_STEPS: u64 = 128;
    const TEST_INITIAL_TICK: u64 = 7;
    const TEST_TICK_QUANTUM: u64 = 3;
    const TEST_ARTIFACT_COUNT: usize = 2;
    const TEST_DIGEST_HEX: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn digest() -> String {
        format!("blake3:{TEST_DIGEST_HEX}")
    }

    fn config() -> ProtocolSimulationConfig {
        ProtocolSimulationConfig {
            schema: PROTOCOL_SIMULATION_CONFIG_SCHEMA.to_string(),
            seed: TEST_SEED,
            schedule: ProtocolScheduleRef {
                schedule_id: "ownership-reacquisition".to_string(),
                digest: digest(),
            },
            scheduler: ProtocolSchedulerPolicy {
                policy_id: "deterministic-round-robin-v1".to_string(),
                maximum_steps: TEST_MAXIMUM_STEPS,
            },
            virtual_clock: ProtocolVirtualClockPolicy {
                policy_id: "simulation-ticks-v1".to_string(),
                initial_tick: TEST_INITIAL_TICK,
                tick_quantum: TEST_TICK_QUANTUM,
            },
            rng: ProtocolRngPolicy {
                algorithm: "chacha20-v1".to_string(),
                seed_derivation: "config-seed-v1".to_string(),
            },
            protocol: ProtocolIdentity {
                protocol_id: "lease-replication".to_string(),
                protocol_version: "v1".to_string(),
                adapter_id: "lease-replication-test-adapter".to_string(),
                adapter_version: "v1".to_string(),
            },
            artifact_digests: std::collections::BTreeMap::from([
                ("adapter".to_string(), digest()),
                ("protocol".to_string(), digest()),
            ]),
        }
    }

    fn receipt() -> ProtocolSimulationReceipt {
        ProtocolSimulationReceipt {
            schema: PROTOCOL_SIMULATION_RECEIPT_SCHEMA.to_string(),
            run_id: "protocol-run-1".to_string(),
            config_digest: digest(),
            config: config(),
            fault_schedule_digest: digest(),
            history_digest: digest(),
            output_digest: digest(),
            evidence_class: ProtocolSimulationEvidenceClass::AdapterProtocolSimulation,
        }
    }

    #[test]
    fn config_and_receipt_round_trip_without_losing_bindings() {
        let expected_config = config();
        let encoded_config = serde_json::to_vec(&expected_config).expect("encode config");
        let decoded_config: ProtocolSimulationConfig =
            serde_json::from_slice(&encoded_config).expect("decode config");
        assert_eq!(decoded_config, expected_config);

        let expected_receipt = receipt();
        let encoded_receipt = serde_json::to_vec(&expected_receipt).expect("encode receipt");
        let decoded_receipt: ProtocolSimulationReceipt =
            serde_json::from_slice(&encoded_receipt).expect("decode receipt");
        assert_eq!(decoded_receipt, expected_receipt);
        assert_eq!(decoded_receipt.config.seed, TEST_SEED);
        assert_eq!(decoded_receipt.config.schedule.digest, digest());
        assert_eq!(
            decoded_receipt.config.artifact_digests.len(),
            TEST_ARTIFACT_COUNT
        );
    }

    #[test]
    fn unknown_missing_and_mistyped_fields_fail_closed() {
        let mut unknown = serde_json::to_value(config()).expect("config value");
        unknown
            .as_object_mut()
            .expect("config object")
            .insert("host_clock".to_string(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<ProtocolSimulationConfig>(unknown).is_err());

        let mut missing = serde_json::to_value(receipt()).expect("receipt value");
        missing
            .as_object_mut()
            .expect("receipt object")
            .remove("history_digest");
        assert!(serde_json::from_value::<ProtocolSimulationReceipt>(missing).is_err());

        let mut mistyped = serde_json::to_value(config()).expect("config value");
        mistyped["rng"]["algorithm"] = serde_json::Value::Bool(false);
        assert!(serde_json::from_value::<ProtocolSimulationConfig>(mistyped).is_err());

        let mut overclaim = serde_json::to_value(receipt()).expect("receipt value");
        overclaim["evidence_class"] = serde_json::Value::String("vm_snapshot_replay".to_string());
        assert!(serde_json::from_value::<ProtocolSimulationReceipt>(overclaim).is_err());
    }

    use std::cell::Cell;
    use std::convert::Infallible;

    const TEST_NODE_ID: u64 = 5;
    const TEST_INITIAL_GENERATION: u64 = 11;
    const TEST_EARLY_TICK: u64 = 3;
    const TEST_LATE_TICK: u64 = 9;
    const TEST_EARLY_SEQUENCE: u64 = 1;
    const TEST_LATE_SEQUENCE: u64 = 2;
    const TEST_MAXIMUM_SCHEDULE_STEPS: u64 = 4;
    const TEST_FACT_COUNT: usize = 1;
    const TEST_FORBIDDEN_EFFECT_COUNT: usize = 3;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct LeaseState {
        owner: Option<u64>,
        generation: u64,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum LeaseInput {
        Acquire { node_id: u64 },
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum LeaseEffect {
        BroadcastOwner { node_id: u64 },
    }

    struct LeaseAdapter;

    impl ProtocolAdapter for LeaseAdapter {
        type State = LeaseState;
        type Input = LeaseInput;
        type EmittedEvent = LeaseEffect;
        type Error = Infallible;

        fn transition(
            &self,
            state: &Self::State,
            input: &ProtocolTransitionInput<Self::Input>,
        ) -> Result<ProtocolTransition<Self::State, Self::EmittedEvent>, Self::Error> {
            let LeaseInput::Acquire { node_id } = &input.event;
            let node_id = *node_id;
            let next_generation = state
                .generation
                .checked_add(1)
                .expect("fixture generation stays bounded");
            Ok(ProtocolTransition {
                next_state: LeaseState {
                    owner: Some(node_id),
                    generation: next_generation,
                },
                emitted_events: vec![LeaseEffect::BroadcastOwner { node_id }],
                facts: vec![ProtocolFact {
                    kind: ProtocolFactKind::Ownership,
                    subject: format!("lease-generation-{next_generation}"),
                    value_ref: format!("node-{node_id}"),
                }],
                effect_requests: vec![
                    ProtocolEffectRequest::VirtualClockRead,
                    ProtocolEffectRequest::SeededRandomRead {
                        stream_id: "lease-election".to_string(),
                    },
                    ProtocolEffectRequest::RegisteredIo {
                        hook_id: "protocol-message".to_string(),
                    },
                    ProtocolEffectRequest::DeclaredFault {
                        hook_id: "fault-schedule".to_string(),
                    },
                ],
            })
        }
    }

    struct AlternatingAdapter {
        calls: Cell<u64>,
    }

    impl ProtocolAdapter for AlternatingAdapter {
        type State = u64;
        type Input = ();
        type EmittedEvent = ();
        type Error = Infallible;

        fn transition(
            &self,
            state: &Self::State,
            _input: &ProtocolTransitionInput<Self::Input>,
        ) -> Result<ProtocolTransition<Self::State, Self::EmittedEvent>, Self::Error> {
            let calls = self.calls.get();
            self.calls.set(calls.checked_add(1).expect("call count"));
            Ok(ProtocolTransition {
                next_state: state.checked_add(calls).expect("fixture state"),
                emitted_events: Vec::new(),
                facts: Vec::new(),
                effect_requests: Vec::new(),
            })
        }
    }

    struct EffectRequestAdapter {
        request: ProtocolEffectRequest,
    }

    impl ProtocolAdapter for EffectRequestAdapter {
        type State = u64;
        type Input = ();
        type EmittedEvent = ();
        type Error = Infallible;

        fn transition(
            &self,
            state: &Self::State,
            _input: &ProtocolTransitionInput<Self::Input>,
        ) -> Result<ProtocolTransition<Self::State, Self::EmittedEvent>, Self::Error> {
            Ok(ProtocolTransition {
                next_state: *state,
                emitted_events: Vec::new(),
                facts: Vec::new(),
                effect_requests: vec![self.request.clone()],
            })
        }
    }

    fn scheduler_state() -> ProtocolEventSchedulerState<String> {
        ProtocolEventSchedulerState {
            next_step: 0,
            current_tick: 0,
            policy: ProtocolSchedulerPolicy {
                policy_id: "deterministic-ready-order-v1".to_string(),
                maximum_steps: TEST_MAXIMUM_SCHEDULE_STEPS,
            },
            pending: vec![
                PendingProtocolEvent {
                    sequence: TEST_LATE_SEQUENCE,
                    ready_tick: TEST_LATE_TICK,
                    target: "node-b".to_string(),
                    event: "replicate".to_string(),
                },
                PendingProtocolEvent {
                    sequence: TEST_EARLY_SEQUENCE,
                    ready_tick: TEST_EARLY_TICK,
                    target: "node-a".to_string(),
                    event: "acquire".to_string(),
                },
            ],
        }
    }

    #[test]
    fn identical_transition_and_schedule_inputs_repeat_exactly() {
        let state = LeaseState {
            owner: None,
            generation: TEST_INITIAL_GENERATION,
        };
        let input = ProtocolTransitionInput {
            step: 0,
            virtual_tick: TEST_EARLY_TICK,
            event: LeaseInput::Acquire {
                node_id: TEST_NODE_ID,
            },
        };
        let first_transition = verify_repeatable_transition(&LeaseAdapter, &state, &input)
            .expect("pure lease transition");
        let second_transition = verify_repeatable_transition(&LeaseAdapter, &state, &input)
            .expect("repeated pure lease transition");
        assert_eq!(first_transition, second_transition);
        assert_eq!(first_transition.next_state.owner, Some(TEST_NODE_ID));
        assert_eq!(first_transition.facts.len(), TEST_FACT_COUNT);

        let schedule = scheduler_state();
        let first_schedule =
            schedule_next_protocol_event(&schedule).expect("first schedule decision");
        let second_schedule =
            schedule_next_protocol_event(&schedule).expect("repeated schedule decision");
        assert_eq!(first_schedule, second_schedule);
        assert_eq!(first_schedule.selected.sequence, TEST_EARLY_SEQUENCE);
        assert_eq!(first_schedule.next_state.current_tick, TEST_EARLY_TICK);
    }

    #[test]
    fn registered_simulation_effects_are_admitted() {
        let requests = [
            ProtocolEffectRequest::VirtualClockRead,
            ProtocolEffectRequest::SeededRandomRead {
                stream_id: "election-timeout".to_string(),
            },
            ProtocolEffectRequest::RegisteredIo {
                hook_id: "protocol-message".to_string(),
            },
            ProtocolEffectRequest::DeclaredFault {
                hook_id: "fault-schedule".to_string(),
            },
        ];
        validate_protocol_effect_requests(&requests).expect("registered effects validate");

        let malformed = [ProtocolEffectRequest::RegisteredIo {
            hook_id: String::new(),
        }];
        assert_eq!(
            validate_protocol_effect_requests(&malformed),
            Err(ProtocolEffectValidationError::EmptyBinding {
                request_index: 0,
                field: "hook_id"
            })
        );
    }

    #[test]
    fn unbound_nondeterminism_fixtures_fail_as_unsupported_effects() {
        let fixtures = [
            (
                ProtocolEffectRequest::HostWallClockRead,
                ProtocolUnboundNondeterminism::HostWallClock,
            ),
            (
                ProtocolEffectRequest::HostRandomRead,
                ProtocolUnboundNondeterminism::HostRandomness,
            ),
            (
                ProtocolEffectRequest::UnregisteredExternalIo {
                    operation: "host-socket-read".to_string(),
                },
                ProtocolUnboundNondeterminism::UnregisteredExternalIo,
            ),
        ];
        assert_eq!(fixtures.len(), TEST_FORBIDDEN_EFFECT_COUNT);
        let input = ProtocolTransitionInput {
            step: 0,
            virtual_tick: 0,
            event: (),
        };
        for (request, source) in fixtures {
            let adapter = EffectRequestAdapter { request };
            assert_eq!(
                verify_repeatable_transition(&adapter, &0, &input),
                Err(ProtocolTransitionCheckError::FirstRunEffect(
                    ProtocolEffectValidationError::UnboundNondeterminism {
                        request_index: 0,
                        source,
                    }
                ))
            );
        }
    }

    #[test]
    fn nondeterministic_adapter_and_malformed_schedule_fail_closed() {
        let adapter = AlternatingAdapter {
            calls: Cell::new(0),
        };
        let input = ProtocolTransitionInput {
            step: 0,
            virtual_tick: 0,
            event: (),
        };
        assert_eq!(
            verify_repeatable_transition(&adapter, &0, &input),
            Err(ProtocolTransitionCheckError::DivergentOutput)
        );

        let mut duplicate = scheduler_state();
        duplicate.pending[1].sequence = TEST_LATE_SEQUENCE;
        assert_eq!(
            schedule_next_protocol_event(&duplicate),
            Err(ProtocolScheduleError::DuplicateSequence {
                sequence: TEST_LATE_SEQUENCE
            })
        );

        let mut exhausted = scheduler_state();
        exhausted.next_step = TEST_MAXIMUM_SCHEDULE_STEPS;
        assert_eq!(
            schedule_next_protocol_event(&exhausted),
            Err(ProtocolScheduleError::StepLimitReached {
                completed: TEST_MAXIMUM_SCHEDULE_STEPS,
                maximum: TEST_MAXIMUM_SCHEDULE_STEPS
            })
        );

        let mut too_many = scheduler_state();
        too_many.policy.maximum_steps = 1;
        assert_eq!(
            schedule_next_protocol_event(&too_many),
            Err(ProtocolScheduleError::PendingEventLimitExceeded {
                found: too_many.pending.len(),
                maximum: 1
            })
        );

        let mut empty_target = scheduler_state();
        empty_target.pending[0].target.clear();
        assert_eq!(
            schedule_next_protocol_event(&empty_target),
            Err(ProtocolScheduleError::EmptyTarget {
                sequence: TEST_LATE_SEQUENCE
            })
        );
    }
}
