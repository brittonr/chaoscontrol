## ADDED Requirements

### Requirement: perf-stat wrapper script
`scripts/perf-stat.sh` SHALL launch a given `chaoscontrol-explore` command in the background, attach `perf stat` to the process, wait for completion, and print hardware counter results.

#### Scenario: Basic usage
- **WHEN** `scripts/perf-stat.sh cargo run --release --bin chaoscontrol-explore -- run --kernel vmlinux --initrd initrd.gz --rounds 5` is executed
- **THEN** the exploration runs to completion and `perf stat` output (cycles, instructions, IPC, cache-misses, branch-misses, context-switches) is printed to stderr

#### Scenario: Process not found
- **WHEN** the exploration binary fails to start (e.g., missing kernel file)
- **THEN** the script SHALL exit with a non-zero status and print an error message

### Requirement: Default hardware counters
The script SHALL collect at minimum: `cycles`, `instructions`, `cache-references`, `cache-misses`, `branch-instructions`, `branch-misses`, `context-switches`.

#### Scenario: Counter output format
- **WHEN** `scripts/perf-stat.sh` completes
- **THEN** output SHALL include IPC (instructions per cycle) as computed by `perf stat`

### Requirement: Optional counter override
The script SHALL accept a `PERF_EVENTS` environment variable to override the default counter list.

#### Scenario: Custom counters
- **WHEN** `PERF_EVENTS="cycles,instructions,L1-dcache-load-misses" scripts/perf-stat.sh ...` is executed
- **THEN** `perf stat` SHALL collect only the specified events instead of the defaults

### Requirement: No Rust code changes
The script SHALL operate entirely through `perf stat -p <pid>` attachment. No modifications to Rust source code SHALL be required for the script to function.

#### Scenario: Works with any binary
- **WHEN** `scripts/perf-stat.sh` is invoked with a non-chaoscontrol binary
- **THEN** it SHALL still attach `perf stat` and report counters (general-purpose wrapper)
