## Why

ProcessKill faults cause kernel panics. With `panic=-1` (reboot
immediately) and no BIOS, the kernel enters an infinite
GPF→exception-handler→stack-overflow→GPF cascade that generates
unlimited serial I/O. The idle detector never fires because exits keep
accumulating, so the VMM loops forever on the "crashed" VM. This blocks
exploration runs that inject ProcessKill or NMI faults.

## What Changes

- Switch the default kernel command line from `panic=-1` to `panic=0`
  so the kernel halts (HLT) after a panic instead of attempting to
  reboot.
- Add a serial-based panic detector in the VMM: when the serial output
  contains the string `Kernel panic`, mark the VM as crashed
  immediately regardless of the idle counter.
- When the VMM detects `VcpuExit::Shutdown` (triple fault) or the
  serial panic string, return the "halted" signal to `run_bounded` so
  the controller can skip the VM for the rest of the branch.
- Reset the panic detector state on snapshot restore so it doesn't
  carry false positives across branches.

## Capabilities

### New Capabilities
- `panic-detection`: Detect kernel panics via serial output and
  VcpuExit::Shutdown, mark VMs as crashed, and halt execution
  promptly instead of spinning on the dead VM.

### Modified Capabilities

## Impact

- `chaoscontrol-vmm/src/vm.rs`: change `panic=-1` to `panic=0` in
  both cmdline templates, add serial panic detection in `step()`,
  reset panic state in `restore()`.
- `chaoscontrol-vmm/src/controller.rs`: controller already handles
  `VmStatus::Crashed` — no changes needed there.
- All existing integration tests: `panic=0` is strictly better than
  `panic=-1` for deterministic VMs (HLT is a clean exit, reboot with
  no BIOS is not).
