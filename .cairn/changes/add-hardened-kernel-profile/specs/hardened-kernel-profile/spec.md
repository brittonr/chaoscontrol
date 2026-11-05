## ADDED Requirements

### Requirement: Hardened kernel profile is opt-in and config-checked

r[chaoscontrol.hardened_kernel.profile]

The kernel builder MUST keep the default profile unchanged and MUST expose a
separate hardening profile that enables the pinned memory/control-flow options and
disables only options that do not fight the nixpkgs common config. A flake check
MUST assert the pinned values and the retained virtio/EXT4 support. No existing
check, matrix row, or default package may select the hardened kernel, and the
profile MUST NOT be exposed together with KCOV.

#### Scenario: Default unaffected
- GIVEN an existing kernel consumer
- WHEN it builds without the hardening option
- THEN the resulting config and artifact are unchanged

#### Scenario: Hardened config pinned
- GIVEN the hardening profile
- WHEN the config check runs
- THEN the pinned enabled/disabled symbols and virtio/EXT4 support are verified

### Requirement: Whole-subsystem removal needs an allnoconfig base

r[chaoscontrol.hardened_kernel.allnoconfig_gate]

Disabling modules, debugfs, kallsyms, BPF, io_uring, userfaultfd, or legacy ABI
MUST NOT be attempted through overrides on the common config. Those removals
require a separate `allnoconfig` base with an explicit required-symbols list and
their own evidence.

#### Scenario: Override rejection
- GIVEN a proposed change that forces one of those symbols off on the common config
- WHEN the kernel config is generated
- THEN the change is rejected or deferred rather than accepted with a conflicting config
