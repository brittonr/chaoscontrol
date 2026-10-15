# Distinguish Explorer Failure Outcomes

## Why

Operators and scripts cannot tell an explorer failure from a finding.

- `chaoscontrol-explore run` and `resume` exit 1 when exploration fails (`EXIT_ERROR`, `chaoscontrol-explore.rs:64`, `:1151`, `:1985`). They also exit 1 when the run found bugs (`:1209`, `:2011`). On 2026-09-25, `scripts/bug-hunt.sh` reported six crashed Raft campaigns as `FOUND`.
- `ExploreError::Vm` displays as `VM error` and hides its source (`explorer.rs`, `#[snafu(display("VM error"))]`). `minimize` printed `Minimization failed: VM error` (`chaoscontrol-explore.rs:2197`) for a snapshot that `reproduce` rejected with `virtio transport restore failed: Queue(AvailableDelta { delta: 65533, capacity: 256 })`.
- The worker pool turns every non-watchdog branch error and panic into a zero-coverage placeholder result (`worker.rs`, `empty_branch_result`). The explorer counts, enriches, and merges that placeholder as a branch. No report field records it.

## What Changes

- Give explorer failures a dedicated exit status that differs from clean, bugs-found, and assertion-floor statuses. r[chaoscontrol.explorer_outcomes.exit_status]
- Print the full error source chain for every explorer command failure. r[chaoscontrol.explorer_outcomes.error_chain]
- Report failed worker-pool branches as failed branches instead of placeholder results, and exclude them from coverage, bugs, and branch totals. r[chaoscontrol.explorer_outcomes.failed_branches]
- Classify `scripts/bug-hunt.sh` results by exit status instead of by the presence of `report.txt`. r[chaoscontrol.explorer_outcomes.exit_status]

## Impact

- **Code**: CLI exit handling for `run`, `resume`, `campaign`, `minimize`, and `export-bugs`; `ExploreError` display; worker-pool branch outcomes; report and round history; `scripts/bug-hunt.sh`.
- **Compatibility**: exit 0 (clean), exit 1 (bugs found), and exit 3 (assertion floor) keep their meanings. Consumers that treat every nonzero status as bugs found must read the new error status. The `reproduce` exit contract used by replay evidence is unchanged.

## Non-Goals

- No change to `reproduce` exit semantics or replay verdict contracts.
- No change to watchdog recovery, which `isolate-branch-watchdog-timeouts` owns.
- No change to which assertions count as bugs, which `evaluate-sometimes-assertions-campaign-wide` owns.
