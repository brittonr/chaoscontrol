# Process Restart Reboot Specification

## Purpose

Defines the canonical ChaosControl requirements for process restart reboot.

## Requirements
### Requirement: ProcessRestart reboots the VM
When a `ProcessRestart` fault is dispatched, the controller SHALL reload the kernel and initrd into the target VM's memory, reset CPU state to boot entry, and run the VM until `setup_complete` is signaled by the guest. The VM SHALL retain its virtio-blk disk state (CoW dirty pages) across the reboot so that crash-recovery testing observes persistent data from before the kill.

#### Scenario: Restart after kill preserves disk
- **WHEN** VM 0 has written data to its virtio-blk device
- **AND** `ProcessKill { target: 0 }` is applied followed by `ProcessRestart { target: 0 }`
- **THEN** VM 0 reboots with a fresh kernel and the virtio-blk device retains its dirty pages from before the kill

#### Scenario: Restart runs until setup_complete
- **WHEN** `ProcessRestart { target: 0 }` fires
- **THEN** the controller runs VM 0 until `setup_complete` or the bootstrap budget is exhausted
- **AND** VM 0's status transitions from `Restarting` → `Running`

#### Scenario: Restart resets CPU and memory but not block device
- **WHEN** VM 0 is restarted
- **THEN** CPU registers are set to boot entry point, RAM is reloaded with the kernel/initrd
- **AND** the virtio-blk `DeterministicBlock` retains its CoW overlay
- **AND** the virtio-net device is re-initialized (fresh MAC, empty queues)
- **AND** the coverage bitmap is cleared

#### Scenario: Restart when VM is already running
- **WHEN** `ProcessRestart { target: 0 }` fires and VM 0 is in `Running` status (not killed)
- **THEN** the controller treats it as a kill-then-restart: halts the VM, then reboots

### Requirement: Restart budget
The controller SHALL use the configured `bootstrap_budget` as the maximum tick count for the restart's boot sequence. If `setup_complete` is not received within the budget, the VM's status SHALL be set to `Crashed`.

#### Scenario: Boot within budget
- **WHEN** the guest signals `setup_complete` at tick 800 and the bootstrap budget is 10000
- **THEN** the VM status is `Running` and the controller resumes normal stepping

#### Scenario: Boot exceeds budget
- **WHEN** the guest does not signal `setup_complete` within 10000 ticks
- **THEN** the VM status is set to `Crashed` and the VM is excluded from further stepping

### Requirement: Deterministic restart
The restart sequence SHALL be deterministic given the same VM state (disk contents, seed). The kernel reload, memory initialization, and boot sequence SHALL use the same deterministic mechanisms as the initial bootstrap (sync_tsc_to_guest, deterministic PIT, etc.).

#### Scenario: Two restarts from same state produce same result
- **WHEN** VM 0 is snapshot'd, killed, restarted, and run for N ticks
- **AND** VM 0 is restored from the snapshot, killed again, restarted, and run for N ticks
- **THEN** both runs produce identical serial output and coverage bitmaps
