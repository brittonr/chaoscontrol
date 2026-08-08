//! KVM shell adapter for pure simulation-core commands.

use crate::vm::{DeterministicVm, VmError};
use chaoscontrol_sim_core::{CommandExecutor, ExecutionCommand, ExitObservation};

/// Applies one vCPU command to one exact VMM slot.
pub struct KvmVcpuExecutor<'vm> {
    vm_index: usize,
    vm: &'vm mut DeterministicVm,
}

impl<'vm> KvmVcpuExecutor<'vm> {
    pub fn new(vm_index: usize, vm: &'vm mut DeterministicVm) -> Self {
        Self { vm_index, vm }
    }
}

impl CommandExecutor for KvmVcpuExecutor<'_> {
    type Error = VmError;

    fn execute(&mut self, command: &ExecutionCommand) -> Result<ExitObservation, Self::Error> {
        let ExecutionCommand::RunVcpu {
            sequence,
            vm_index,
            vcpu_index,
            exit_budget,
        } = command
        else {
            return Err(VmError::Snapshot {
                message: "KVM vCPU adapter received a non-vCPU command".to_string(),
            });
        };
        if *vm_index != self.vm_index {
            return Err(VmError::Snapshot {
                message: format!(
                    "KVM vCPU adapter target mismatch: expected {}, found {}",
                    self.vm_index, vm_index
                ),
            });
        }
        if *vcpu_index != 0 {
            return Err(VmError::Snapshot {
                message: format!(
                    "controller round adapter requires aggregate vCPU index 0, found {vcpu_index}"
                ),
            });
        }
        let (exits, halted) = self.vm.run_bounded(*exit_budget)?;
        Ok(ExitObservation::VcpuCompleted {
            sequence: *sequence,
            vm_index: *vm_index,
            exits,
            halted,
        })
    }
}
