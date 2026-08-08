# Memory Guard Specification

## Purpose

Defines the canonical ChaosControl requirements for memory guard.

## Requirements
### Requirement: Pre-flight memory estimate
At startup, both `run` and `campaign` subcommands SHALL compute an estimated peak memory usage based on: `num_seeds × num_vms × vm_memory_mb` for VM memory, plus `num_seeds × max_frontier × num_vms × vm_memory_mb` as a worst-case frontier snapshot estimate.

#### Scenario: Memory estimate computed for campaign
- **WHEN** `campaign --campaign-seeds 4 --vms 3 --max-frontier 50` is launched with 256 MB VMs
- **THEN** VM memory estimate is 4×3×256 = 3072 MB and frontier estimate is 4×50×3×256 = 153600 MB (pessimistic)

#### Scenario: Memory estimate computed for single run
- **WHEN** `run --vms 3 --max-frontier 50` is launched with 256 MB VMs
- **THEN** VM memory estimate is 1×3×256 = 768 MB

### Requirement: Available memory check
The explorer SHALL read `MemAvailable` from `/proc/meminfo` and compare the VM memory estimate (not the pessimistic frontier estimate) against it. If the VM memory estimate exceeds 80% of available memory, a warning SHALL be printed to stderr.

#### Scenario: Sufficient memory
- **WHEN** VM memory estimate is 3 GB and system has 16 GB available
- **THEN** no warning is printed, exploration starts normally

#### Scenario: Marginal memory
- **WHEN** VM memory estimate is 6 GB and system has 7 GB available (85%)
- **THEN** a warning is printed: "Warning: estimated VM memory (6.0 GB) exceeds 80% of available memory (7.0 GB). Consider reducing seeds or VMs."

#### Scenario: /proc/meminfo unavailable
- **WHEN** `/proc/meminfo` cannot be read (non-Linux host or restricted container)
- **THEN** no check is performed, exploration starts with a debug-level log noting the skip

### Requirement: Strict memory mode
When `--strict-memory` is passed, the explorer SHALL exit with an error if the VM memory estimate exceeds 80% of available memory instead of printing a warning.

#### Scenario: Strict mode blocks overcommit
- **WHEN** `--strict-memory` is passed and VM memory estimate exceeds 80% of available
- **THEN** the process exits with code 1 and an error message, no VMs are created

#### Scenario: Strict mode allows sufficient memory
- **WHEN** `--strict-memory` is passed and VM memory estimate is under 80%
- **THEN** exploration starts normally

### Requirement: Memory estimate in log output
The memory estimate and available memory SHALL be printed at `info` level at startup, regardless of whether the threshold is exceeded. The format SHALL show the calculation components (seeds, VMs, VM size).

#### Scenario: Memory info logged
- **WHEN** any exploration starts
- **THEN** an info-level log line shows "Memory: 3.0 GB estimated (4 seeds × 3 VMs × 256 MB), 15.2 GB available"
