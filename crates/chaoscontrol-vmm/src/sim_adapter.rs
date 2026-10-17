//! Complete imperative executor for the simulation core's typed effect contract.

use crate::controller::{SimulationController, SimulationSnapshot};
use crate::vm::VmError;
use chaoscontrol_sim_core::boundary::{admit_command, validate_exchange_facts};
use chaoscontrol_sim_core::{BoundaryState, CommandExecutor, ExecutionCommand, ExitObservation};

/// Drives an ordered command session. Captured snapshots are retained until taken;
/// a second capture is rejected instead of silently discarding its payload.
pub struct KvmExecutor<'controller> {
    controller: &'controller mut SimulationController,
    state: BoundaryState,
    captured: Option<(u64, SimulationSnapshot)>,
}

impl<'controller> KvmExecutor<'controller> {
    pub fn new(controller: &'controller mut SimulationController, next_sequence: u64) -> Self {
        let state = BoundaryState {
            next_sequence,
            vm_count: controller.num_vms(),
        };
        Self {
            controller,
            state,
            captured: None,
        }
    }

    pub fn take_snapshot(&mut self) -> Option<(u64, SimulationSnapshot)> {
        self.captured.take()
    }

    pub fn state(&self) -> BoundaryState {
        self.state
    }
}

impl CommandExecutor for KvmExecutor<'_> {
    type Error = VmError;

    fn execute(&mut self, command: &ExecutionCommand) -> Result<ExitObservation, Self::Error> {
        admit_command(self.state, command).map_err(|error| VmError::Snapshot {
            message: format!("execution command preflight: {error}"),
        })?;
        if matches!(command, ExecutionCommand::CaptureSnapshot { .. }) && self.captured.is_some() {
            return Err(VmError::Snapshot {
                message: "execution snapshot payload must be taken before another capture"
                    .to_string(),
            });
        }
        let (observation, snapshot) = self.controller.execute_core_command(command)?;
        self.state = validate_exchange_facts(self.state, command, &observation)
            .map_err(|error| self.controller.poison_execution_exchange(error.to_string()))?;
        if let Some(snapshot) = snapshot {
            let ExecutionCommand::CaptureSnapshot {
                snapshot_sequence, ..
            } = command
            else {
                unreachable!("only the snapshot command returns a snapshot");
            };
            self.captured = Some((*snapshot_sequence, snapshot));
        }
        Ok(observation)
    }
}
