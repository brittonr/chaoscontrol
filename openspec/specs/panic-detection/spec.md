### Requirement: Kernel halts on panic
The default kernel command line SHALL use `panic=0` so the kernel
executes HLT after a panic instead of attempting to reboot. This
produces a clean `VcpuExit::Hlt` that the VMM already handles.

#### Scenario: ProcessKill causes halt not reboot loop
- **WHEN** a ProcessKill fault crashes a VM
- **THEN** the kernel panics, executes HLT, and the VMM receives
  `VcpuExit::Hlt` within a bounded number of exits (not an infinite
  serial I/O loop)

#### Scenario: Existing tests unaffected
- **WHEN** the integration test suite runs with `panic=0`
- **THEN** all previously-passing tests still pass

### Requirement: Serial panic detection
The VMM SHALL monitor serial output for the string `Kernel panic`.
When detected, the VM SHALL be marked as crashed and `step()` SHALL
return the halted signal on the next iteration.

#### Scenario: Panic string detected during run_bounded
- **WHEN** a VM outputs `Kernel panic - not syncing:` to the serial
  port during `run_bounded`
- **THEN** the VM is marked as panicked and `run_bounded` returns
  within a small number of additional exits

#### Scenario: No false positives on normal output
- **WHEN** a VM runs normally and never outputs `Kernel panic`
- **THEN** the panic detector does not trigger

### Requirement: Panic state reset on restore
The panic detector state SHALL be cleared when a snapshot is restored
so that a panic from a previous branch does not carry into a new
branch.

#### Scenario: Restore clears panic flag
- **WHEN** a VM panics in branch A and then branch B restores from
  the pre-panic snapshot
- **THEN** the panic flag is clear and branch B runs normally

### Requirement: VcpuExit::Shutdown marks VM crashed
When the VMM receives `VcpuExit::Shutdown` (triple fault), the VM
SHALL be treated as crashed. The controller SHALL skip further
scheduling of that VM for the remainder of the branch.

#### Scenario: Triple fault halts VM
- **WHEN** a kernel panic causes a triple fault (Shutdown exit)
- **THEN** `run_bounded` returns immediately with the halted flag set
