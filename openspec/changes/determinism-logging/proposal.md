# Determinism Logging

## Problem Statement

ChaosControl provides deterministic hypervisor capabilities through controlled TSC, PIT, entropy, and scheduling. However, when subtle non-determinism bugs surface in complex workloads, debugging them requires manually correlating VMM logs, fault injection traces, and guest behavior. There's no systematic way to verify determinism or pinpoint the exact divergence point when two supposedly identical runs produce different outcomes.

Without high-fidelity logging of every decision point that affects guest execution, developers must rely on coarse-grained metrics and intuition to track down determinism violations. This makes debugging non-determinism regressions time-intensive and error-prone.

## Proposed Solution

Add a **paranoid determinism logging** mode that captures every significant event affecting guest execution in structured binary logs. This creates a deterministic equivalent of `strace` — when two runs diverge, diff the logs to find the exact divergence point.

The logging system will capture:
- VM exits with type, TSC, and exit count
- RNG draws with domain and generated values  
- Fault dispatches with type and timing
- SDK hypercalls with command and payload hashes
- Scheduler decisions with active vCPU and reasoning

This is explicitly a **debugging tool**, not for production use. The binary log format prioritizes throughput over human readability to handle millions of events per second without significantly impacting guest performance.

## Key Changes

### Core Infrastructure
- **chaoscontrol-dlog** crate for binary log format, ring buffer writer, and reader
- **chaoscontrol-vmm** integration in `run_bounded()` for VM exit logging
- **chaoscontrol-fault** integration for fault dispatch logging
- **chaoscontrol-replay** new `diff` subcommand for log comparison

### User Interface
- `VmConfig::paranoid_log` flag to enable per-VM logging
- CLI `--paranoid-log <path>` option for output directory
- `chaoscontrol-replay diff <log1> <log2>` for divergence analysis

### Log Management
- Per-VM binary log streams with ring buffer memory backing
- On-demand or end-of-run flush to persistent storage
- Structured diff output showing first divergence with context

## Capabilities

This change adds the following new capability:

- **determinism-log**: Paranoid logging mode for debugging non-determinism by capturing every significant execution event (VM exits, RNG, faults, hypercalls, scheduling) in high-throughput binary logs with diff analysis

## Success Criteria

- Enable determinism logging via config flag or CLI option
- Log VM exits, RNG draws, faults, SDK calls, and scheduler decisions
- Achieve minimal performance impact (< 5% overhead) in logging mode
- Diff tool identifies exact divergence point between two log streams
- Integration with existing chaoscontrol-trace BPF tooling
- Documentation and examples for debugging workflow