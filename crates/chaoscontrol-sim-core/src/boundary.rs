use std::fmt;

/// Maximum payload bytes carried by one boundary DTO.
pub const MAX_BOUNDARY_PAYLOAD_BYTES: usize = 1_048_576;

/// A machine effect selected by the deterministic core.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionCommand {
    RunVcpu {
        sequence: u64,
        vm_index: usize,
        vcpu_index: usize,
        exit_budget: u64,
    },
    WriteRegisters {
        sequence: u64,
        vm_index: usize,
        register_bytes: Vec<u8>,
    },
    DeliverInterrupt {
        sequence: u64,
        vm_index: usize,
        vector: u32,
    },
    DeliverNetwork {
        sequence: u64,
        from_vm: usize,
        to_vm: usize,
        packet: Vec<u8>,
    },
    ApplyFault {
        sequence: u64,
        target_vm: usize,
        fault_ref: String,
    },
    CaptureSnapshot {
        sequence: u64,
        snapshot_sequence: u64,
    },
}

impl ExecutionCommand {
    pub fn sequence(&self) -> u64 {
        match self {
            Self::RunVcpu { sequence, .. }
            | Self::WriteRegisters { sequence, .. }
            | Self::DeliverInterrupt { sequence, .. }
            | Self::DeliverNetwork { sequence, .. }
            | Self::ApplyFault { sequence, .. }
            | Self::CaptureSnapshot { sequence, .. } => *sequence,
        }
    }
}

/// A typed shell observation returned after one command.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExitObservation {
    VcpuCompleted {
        sequence: u64,
        vm_index: usize,
        exits: u64,
        halted: bool,
    },
    RegistersWritten {
        sequence: u64,
        vm_index: usize,
    },
    InterruptDelivered {
        sequence: u64,
        vm_index: usize,
        vector: u32,
    },
    NetworkDelivered {
        sequence: u64,
        from_vm: usize,
        to_vm: usize,
        bytes: usize,
    },
    FaultApplied {
        sequence: u64,
        target_vm: usize,
        fault_ref: String,
    },
    SnapshotCaptured {
        sequence: u64,
        snapshot_sequence: u64,
    },
    Mmio {
        sequence: u64,
        vm_index: usize,
        address: u64,
        bytes: Vec<u8>,
    },
    IoPort {
        sequence: u64,
        vm_index: usize,
        port: u16,
        bytes: Vec<u8>,
    },
    Assertion {
        sequence: u64,
        vm_index: usize,
        assertion_ref: String,
        passed: bool,
    },
}

/// Imperative shell contract for applying one core command.
///
/// The pure core does not own an executor or call this trait. Shell code drives
/// the request/response loop and returns observations for pure validation.
pub trait CommandExecutor {
    type Error;

    fn execute(&mut self, command: &ExecutionCommand) -> Result<ExitObservation, Self::Error>;
}

impl ExitObservation {
    pub fn sequence(&self) -> u64 {
        match self {
            Self::VcpuCompleted { sequence, .. }
            | Self::RegistersWritten { sequence, .. }
            | Self::InterruptDelivered { sequence, .. }
            | Self::NetworkDelivered { sequence, .. }
            | Self::FaultApplied { sequence, .. }
            | Self::SnapshotCaptured { sequence, .. }
            | Self::Mmio { sequence, .. }
            | Self::IoPort { sequence, .. }
            | Self::Assertion { sequence, .. } => *sequence,
        }
    }
}

/// Sequence state owned by the pure command/observation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BoundaryState {
    pub next_sequence: u64,
    pub vm_count: usize,
}

/// One accepted command and its shell observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedExchange {
    pub next_state: BoundaryState,
    pub command: ExecutionCommand,
    pub observation: ExitObservation,
}

/// Pure boundary validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryError {
    NoVirtualMachines,
    SequenceMismatch {
        field: &'static str,
        expected: u64,
        found: u64,
    },
    SequenceExhausted,
    InvalidVm {
        field: &'static str,
        vm_index: usize,
        vm_count: usize,
    },
    InvalidExitBudget,
    PayloadTooLarge {
        field: &'static str,
        found: usize,
        maximum: usize,
    },
    EmptyReference {
        field: &'static str,
    },
    ObservationMismatch {
        field: &'static str,
    },
}

impl fmt::Display for BoundaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for BoundaryError {}

/// Validate one command/observation pair without changing external state.
pub fn validate_exchange(
    state: BoundaryState,
    command: ExecutionCommand,
    observation: ExitObservation,
) -> Result<ValidatedExchange, BoundaryError> {
    if state.vm_count == 0 {
        return Err(BoundaryError::NoVirtualMachines);
    }
    validate_sequence("command.sequence", state.next_sequence, command.sequence())?;
    validate_sequence(
        "observation.sequence",
        state.next_sequence,
        observation.sequence(),
    )?;
    validate_command(&state, &command)?;
    validate_observation_shape(&state, &observation)?;
    validate_pair(&command, &observation)?;
    let next_sequence = state
        .next_sequence
        .checked_add(1)
        .ok_or(BoundaryError::SequenceExhausted)?;
    Ok(ValidatedExchange {
        next_state: BoundaryState {
            next_sequence,
            vm_count: state.vm_count,
        },
        command,
        observation,
    })
}

fn validate_sequence(field: &'static str, expected: u64, found: u64) -> Result<(), BoundaryError> {
    if found != expected {
        return Err(BoundaryError::SequenceMismatch {
            field,
            expected,
            found,
        });
    }
    Ok(())
}

fn validate_vm(
    state: &BoundaryState,
    field: &'static str,
    vm_index: usize,
) -> Result<(), BoundaryError> {
    if vm_index >= state.vm_count {
        return Err(BoundaryError::InvalidVm {
            field,
            vm_index,
            vm_count: state.vm_count,
        });
    }
    Ok(())
}

fn validate_payload(field: &'static str, payload: &[u8]) -> Result<(), BoundaryError> {
    if payload.len() > MAX_BOUNDARY_PAYLOAD_BYTES {
        return Err(BoundaryError::PayloadTooLarge {
            field,
            found: payload.len(),
            maximum: MAX_BOUNDARY_PAYLOAD_BYTES,
        });
    }
    Ok(())
}

fn validate_reference(field: &'static str, value: &str) -> Result<(), BoundaryError> {
    if value.is_empty() {
        return Err(BoundaryError::EmptyReference { field });
    }
    Ok(())
}

fn validate_command(
    state: &BoundaryState,
    command: &ExecutionCommand,
) -> Result<(), BoundaryError> {
    match command {
        ExecutionCommand::RunVcpu {
            vm_index,
            exit_budget,
            ..
        } => {
            validate_vm(state, "command.vm_index", *vm_index)?;
            if *exit_budget == 0 {
                return Err(BoundaryError::InvalidExitBudget);
            }
        }
        ExecutionCommand::WriteRegisters {
            vm_index,
            register_bytes,
            ..
        } => {
            validate_vm(state, "command.vm_index", *vm_index)?;
            validate_payload("command.register_bytes", register_bytes)?;
        }
        ExecutionCommand::DeliverInterrupt { vm_index, .. } => {
            validate_vm(state, "command.vm_index", *vm_index)?;
        }
        ExecutionCommand::DeliverNetwork {
            from_vm,
            to_vm,
            packet,
            ..
        } => {
            validate_vm(state, "command.from_vm", *from_vm)?;
            validate_vm(state, "command.to_vm", *to_vm)?;
            validate_payload("command.packet", packet)?;
        }
        ExecutionCommand::ApplyFault {
            target_vm,
            fault_ref,
            ..
        } => {
            validate_vm(state, "command.target_vm", *target_vm)?;
            validate_reference("command.fault_ref", fault_ref)?;
        }
        ExecutionCommand::CaptureSnapshot { .. } => {}
    }
    Ok(())
}

fn validate_observation_shape(
    state: &BoundaryState,
    observation: &ExitObservation,
) -> Result<(), BoundaryError> {
    match observation {
        ExitObservation::VcpuCompleted { vm_index, .. }
        | ExitObservation::RegistersWritten { vm_index, .. }
        | ExitObservation::InterruptDelivered { vm_index, .. }
        | ExitObservation::Mmio { vm_index, .. }
        | ExitObservation::IoPort { vm_index, .. }
        | ExitObservation::Assertion { vm_index, .. } => {
            validate_vm(state, "observation.vm_index", *vm_index)?;
        }
        ExitObservation::NetworkDelivered { from_vm, to_vm, .. } => {
            validate_vm(state, "observation.from_vm", *from_vm)?;
            validate_vm(state, "observation.to_vm", *to_vm)?;
        }
        ExitObservation::FaultApplied {
            target_vm,
            fault_ref,
            ..
        } => {
            validate_vm(state, "observation.target_vm", *target_vm)?;
            validate_reference("observation.fault_ref", fault_ref)?;
        }
        ExitObservation::SnapshotCaptured { .. } => {}
    }
    match observation {
        ExitObservation::Mmio { bytes, .. } => validate_payload("observation.mmio.bytes", bytes),
        ExitObservation::IoPort { bytes, .. } => {
            validate_payload("observation.io_port.bytes", bytes)
        }
        ExitObservation::Assertion { assertion_ref, .. } => {
            validate_reference("observation.assertion_ref", assertion_ref)
        }
        _ => Ok(()),
    }
}

fn validate_pair(
    command: &ExecutionCommand,
    observation: &ExitObservation,
) -> Result<(), BoundaryError> {
    let matches = match (command, observation) {
        (
            ExecutionCommand::RunVcpu {
                vm_index: expected,
                exit_budget,
                ..
            },
            ExitObservation::VcpuCompleted {
                vm_index, exits, ..
            },
        ) => vm_index == expected && exits <= exit_budget,
        (
            ExecutionCommand::WriteRegisters {
                vm_index: expected, ..
            },
            ExitObservation::RegistersWritten { vm_index, .. },
        ) => vm_index == expected,
        (
            ExecutionCommand::DeliverInterrupt {
                vm_index: expected_vm,
                vector: expected_vector,
                ..
            },
            ExitObservation::InterruptDelivered {
                vm_index, vector, ..
            },
        ) => vm_index == expected_vm && vector == expected_vector,
        (
            ExecutionCommand::DeliverNetwork {
                from_vm: expected_from,
                to_vm: expected_to,
                packet,
                ..
            },
            ExitObservation::NetworkDelivered {
                from_vm,
                to_vm,
                bytes,
                ..
            },
        ) => from_vm == expected_from && to_vm == expected_to && *bytes == packet.len(),
        (
            ExecutionCommand::ApplyFault {
                target_vm: expected_vm,
                fault_ref: expected_ref,
                ..
            },
            ExitObservation::FaultApplied {
                target_vm,
                fault_ref,
                ..
            },
        ) => target_vm == expected_vm && fault_ref == expected_ref,
        (
            ExecutionCommand::CaptureSnapshot {
                snapshot_sequence: expected,
                ..
            },
            ExitObservation::SnapshotCaptured {
                snapshot_sequence, ..
            },
        ) => snapshot_sequence == expected,
        _ => false,
    };
    if !matches {
        return Err(BoundaryError::ObservationMismatch {
            field: "command/observation",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VM_COUNT: usize = 2;
    const FIRST_SEQUENCE: u64 = 7;
    const EXIT_BUDGET: u64 = 3;

    fn state() -> BoundaryState {
        BoundaryState {
            next_sequence: FIRST_SEQUENCE,
            vm_count: VM_COUNT,
        }
    }

    #[test]
    fn canonical_boot_network_fault_and_snapshot_exchanges_are_accepted() {
        let pairs = [
            (
                ExecutionCommand::RunVcpu {
                    sequence: FIRST_SEQUENCE,
                    vm_index: 0,
                    vcpu_index: 0,
                    exit_budget: EXIT_BUDGET,
                },
                ExitObservation::VcpuCompleted {
                    sequence: FIRST_SEQUENCE,
                    vm_index: 0,
                    exits: EXIT_BUDGET,
                    halted: false,
                },
            ),
            (
                ExecutionCommand::DeliverNetwork {
                    sequence: FIRST_SEQUENCE + 1,
                    from_vm: 0,
                    to_vm: 1,
                    packet: vec![1, 2],
                },
                ExitObservation::NetworkDelivered {
                    sequence: FIRST_SEQUENCE + 1,
                    from_vm: 0,
                    to_vm: 1,
                    bytes: 2,
                },
            ),
            (
                ExecutionCommand::ApplyFault {
                    sequence: FIRST_SEQUENCE + 2,
                    target_vm: 1,
                    fault_ref: "fault:pause".into(),
                },
                ExitObservation::FaultApplied {
                    sequence: FIRST_SEQUENCE + 2,
                    target_vm: 1,
                    fault_ref: "fault:pause".into(),
                },
            ),
            (
                ExecutionCommand::CaptureSnapshot {
                    sequence: FIRST_SEQUENCE + 3,
                    snapshot_sequence: 9,
                },
                ExitObservation::SnapshotCaptured {
                    sequence: FIRST_SEQUENCE + 3,
                    snapshot_sequence: 9,
                },
            ),
        ];
        let mut current = state();
        for (command, observation) in pairs {
            current = validate_exchange(current, command, observation)
                .unwrap()
                .next_state;
        }
        assert_eq!(current.next_sequence, FIRST_SEQUENCE + 4);
    }

    #[test]
    fn out_of_order_observation_is_rejected() {
        let error = validate_exchange(
            state(),
            ExecutionCommand::RunVcpu {
                sequence: FIRST_SEQUENCE,
                vm_index: 0,
                vcpu_index: 0,
                exit_budget: EXIT_BUDGET,
            },
            ExitObservation::VcpuCompleted {
                sequence: FIRST_SEQUENCE + 1,
                vm_index: 0,
                exits: 1,
                halted: false,
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            BoundaryError::SequenceMismatch {
                field: "observation.sequence",
                ..
            }
        ));
    }

    #[test]
    fn malformed_or_unknown_dto_is_rejected_by_serde() {
        let malformed = r#"{"kind":"run_vcpu","sequence":0,"vm_index":"bad"}"#;
        let unknown = r#"{"kind":"unknown_observation","sequence":0}"#;
        assert!(serde_json::from_str::<ExecutionCommand>(malformed).is_err());
        assert!(serde_json::from_str::<ExitObservation>(unknown).is_err());
    }

    #[test]
    fn mismatched_command_and_observation_are_rejected() {
        let error = validate_exchange(
            state(),
            ExecutionCommand::CaptureSnapshot {
                sequence: FIRST_SEQUENCE,
                snapshot_sequence: 1,
            },
            ExitObservation::SnapshotCaptured {
                sequence: FIRST_SEQUENCE,
                snapshot_sequence: 2,
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            BoundaryError::ObservationMismatch {
                field: "command/observation"
            }
        );
    }
}
