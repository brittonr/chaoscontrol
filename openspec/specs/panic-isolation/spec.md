# Panic Isolation Specification

## Purpose

Defines the canonical ChaosControl requirements for panic isolation.

## Requirements
### Requirement: Campaign seed panic isolation
Each seed thread in `CampaignRunner::run()` SHALL wrap its `Explorer::new().run()` call in `std::panic::catch_unwind`. If a seed panics, the campaign runner SHALL log the seed number and panic message, mark the seed as failed in `CampaignProgress`, and continue running remaining seeds.

#### Scenario: One seed panics, others continue
- **WHEN** seed 44 panics due to a KVM ioctl error and seeds 42, 43, 45, 46 run normally
- **THEN** seeds 42, 43, 45, 46 complete successfully, seed 44 is marked failed, the campaign report includes results from the 4 successful seeds, and a warning is logged for seed 44

#### Scenario: All seeds panic
- **WHEN** all 5 seeds panic (e.g., kernel path is invalid)
- **THEN** the campaign runner logs 5 failures and returns a campaign report with zero results

#### Scenario: Failed seed recorded in progress checkpoint
- **WHEN** seed 44 panics and the campaign progress is saved
- **THEN** `campaign_progress.json` marks seed 44 as failed with an error message, and `campaign resume` skips it

### Requirement: Worker branch panic isolation
Each branch execution in `WorkerPool::run_branches()` SHALL be wrapped in `std::panic::catch_unwind`. If a branch panics, the worker SHALL return a zero-coverage `BranchResult` with no bugs and no snapshot for that branch. Other branches in the same round SHALL continue executing.

#### Scenario: One branch panics in a parallel round
- **WHEN** branch 3 of 8 panics due to snapshot restore failure
- **THEN** branches 1-2 and 4-8 complete normally, branch 3 returns a zero-coverage result, and the round report reflects results from all 8 branches (7 real + 1 empty)

#### Scenario: Branch panic does not corrupt worker state
- **WHEN** branch 3 panics on worker 1 and then worker 1 is assigned branch 7
- **THEN** branch 7 executes correctly because the controller was restored from snapshot before the panic occurred (the panic happened after restore, during run)

### Requirement: Panic backtrace logging
When a panic is caught, the panic message and location (file:line) SHALL be logged at `error` level. If `RUST_BACKTRACE=1` is set, the full backtrace SHALL be included in the log output.

#### Scenario: Panic logged with location
- **WHEN** a seed panics at `controller.rs:450`
- **THEN** the log contains `"Seed 44 panicked at controller.rs:450: index out of bounds"`

### Requirement: Campaign report includes failed seed count
The `CampaignReport` SHALL include a count of failed seeds alongside successful seeds. The human-readable report SHALL note which seeds failed and why.

#### Scenario: Report shows failed seeds
- **WHEN** 1 of 5 seeds panicked
- **THEN** the campaign report header shows "Seeds: 5 (4 successful, 1 failed)" and lists the failed seed with its error
