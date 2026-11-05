# Design: Distinguish Explorer Failure Outcomes

## Context

The explorer CLI uses exit 0 for a clean run, exit 1 for bugs found, and exit 3 when the assertion-exercise floor fails. It uses `EXIT_ERROR = 1` for failures. Replay evidence contracts fix `reproduce` exit 0 to mean reproduced. The accepted-verdict dogfood wrapper defines its own no-bug status.

## Decisions

### 1. Named outcome statuses

A pure mapping from a command outcome to an exit status replaces literal `exit(1)` calls in exploration commands. Values 0, 1, and 3 keep their meanings. Failure gets a new named constant whose value no other outcome uses. CLI help and the README list every status.

### 2. Error chains

Explorer command failures print the error and each `source()` in order. `ExploreError::Vm` keeps its variant and gains a display string that includes the VM error. Checkpoint and configuration errors follow the same rule.

### 3. Failed worker branches

`BranchOutcome` gains a `Failed { reason }` variant. The worker pool returns it for non-watchdog errors and panics. The round ledger counts failed branches and excludes them from coverage, bug extraction, frontier, corpus, and branch totals. `RoundHistory` gains `failed_branches` with `#[serde(default)]`. A round in which every branch failed or was discarded fails closed with the first failure.

### 4. Script classification

`scripts/bug-hunt.sh` maps each named status to `NOT FOUND`, `FOUND`, `ASSERTION FLOOR`, or `ERROR`. It stops inferring errors from missing files.

## Risks

- External automation that treats every nonzero exit as bugs found will see a new status. The README change lists the mapping. Validation searches this repository's flake, scripts, and tools for exit-status consumers.
