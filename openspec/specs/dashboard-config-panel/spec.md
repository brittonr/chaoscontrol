## ADDED Requirements

### Requirement: Configuration info panel
The UI SHALL display a panel showing the exploration configuration parameters: number of VMs, seed, exploration mode, max rounds, branches per round, ticks per branch, and kernel path (basename only).

#### Scenario: Config panel during live exploration
- **WHEN** the dashboard is connected to a running exploration
- **THEN** the config panel shows the parameters from the Started event

#### Scenario: Config panel in standalone mode
- **WHEN** the dashboard loads a completed exploration from a checkpoint
- **THEN** the config panel shows the parameters from the checkpoint's config section

#### Scenario: Kernel path display
- **WHEN** the kernel path is `/home/user/git/project/result-dev/vmlinux`
- **THEN** the config panel displays only the basename `vmlinux`
