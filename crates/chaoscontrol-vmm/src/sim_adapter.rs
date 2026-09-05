//! KVM shell adapter for pure simulation-core commands.

use chaoscontrol_sim_core::CommandExecutor;

/// Applies one vCPU command to one exact VMM slot.
pub struct KvmVcpuExecutor<'vm> {
    vm_index: usize,
    vm: &'vm mut crate::vm::DeterministicVm,
}

impl<'vm> KvmVcpuExecutor<'vm> {
    pub fn new(vm_index: usize, vm: &'vm mut crate::vm::DeterministicVm) -> Self {
        Self { vm_index, vm }
    }
}

impl CommandExecutor for KvmVcpuExecutor<'_> {
    type Error = crate::vm::VmError;

    fn execute(
        &mut self,
        command: &::chaoscontrol_sim_core::ExecutionCommand,
    ) -> Result<::chaoscontrol_sim_core::ExitObservation, Self::Error> {
        let ::chaoscontrol_sim_core::ExecutionCommand::RunVcpu {
            sequence,
            vm_index,
            vcpu_index,
            exit_budget,
        } = command
        else {
            return Err(crate::vm::VmError::Snapshot {
                message: "KVM vCPU adapter received a non-vCPU command".to_string(),
            });
        };
        if *vm_index != self.vm_index {
            return Err(crate::vm::VmError::Snapshot {
                message: format!(
                    "KVM vCPU adapter target mismatch: expected {}, found {}",
                    self.vm_index, vm_index
                ),
            });
        }
        if *vcpu_index != 0 {
            return Err(crate::vm::VmError::Snapshot {
                message: format!(
                    "controller round adapter requires aggregate vCPU index 0, found {vcpu_index}"
                ),
            });
        }
        let (exits, halted) = self.vm.run_bounded(*exit_budget)?;
        Ok(::chaoscontrol_sim_core::ExitObservation::VcpuCompleted {
            sequence: *sequence,
            vm_index: *vm_index,
            exits,
            halted,
        })
    }
}
