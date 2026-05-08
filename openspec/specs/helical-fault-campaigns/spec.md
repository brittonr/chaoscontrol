# Helical Fault Campaigns Specification

## Purpose

Defines the canonical ChaosControl requirements for helical fault campaigns.

## Requirements
### Requirement: Named helical scenarios materialize deterministically
The explorer SHALL support named helical scenario families that deterministically materialize a concrete `FaultSchedule` and phase summary from scenario configuration plus seed. Re-running the same scenario family with the same configuration and seed SHALL produce the same schedule.

#### Scenario: Same seed yields same materialization
- **WHEN** the `network-ring` scenario is generated twice with the same seed, `num_vms`, `phase_ticks`, and `turns`
- **THEN** both runs produce identical phase summaries
- **AND** both runs produce identical concrete fault schedules

#### Scenario: Different seed changes the materialization
- **WHEN** the same helical scenario family is generated with a different seed
- **THEN** the resulting concrete fault schedule differs in at least one phase parameter or fault placement

### Requirement: Helical phases rotate the primary target across the cluster
A helical scenario SHALL rotate its primary target or partition focus across VM indices from one phase to the next, wrapping only after it has traversed the configured cluster. Rotating a scenario SHALL not repeatedly target the same VM on consecutive turns unless the cluster size is one.

#### Scenario: Three-node ring rotates targets
- **WHEN** a helical scenario runs on 3 VMs for 3 turns
- **THEN** the primary target sequence is `0`, then `1`, then `2` before wrapping

#### Scenario: Rotating partition sets follow the target
- **WHEN** a helical scenario phase includes a partition around the current target VM
- **THEN** the partition membership for the next turn is recomputed around the next target VM rather than reusing the previous phase verbatim

### Requirement: Scenario metadata survives checkpoint, bug report, replay, and minimization flows
The system SHALL persist both the high-level scenario configuration and the materialized phase summary alongside the concrete fault schedule anywhere a bug or checkpoint artifact is written.

#### Scenario: Checkpoint resume keeps the same scenario config
- **WHEN** a campaign with a helical scenario is checkpointed and later resumed
- **THEN** the resumed run uses the same stored scenario family and configuration
- **AND** the report continues to reference the same materialized phase summary

#### Scenario: Minimized bug keeps helical provenance
- **WHEN** a bug found under a helical scenario is minimized
- **THEN** the minimized artifact still records the original scenario family and phase summary
- **AND** the minimized concrete schedule remains sufficient for exact replay
