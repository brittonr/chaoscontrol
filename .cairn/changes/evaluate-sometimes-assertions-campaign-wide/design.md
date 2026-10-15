# Design: Evaluate Sometimes Assertions Campaign-Wide

## Context

Each branch returns a merged `OracleReport`. Restored snapshots carry oracle state from the parent, so a branch report covers the lineage from bootstrap to the end of that branch. `verdict()` gives cross-run semantics for every kind. Only `always` and `unreachable` failures are safety evidence inside a single lineage.

## Decisions

### 1. A pure finding classifier

A pure function maps an assertion kind and a lineage verdict to one of `SafetyViolation`, `LivenessPending`, or `NoFinding`. `always` with a false observation and `unreachable` with a hit map to `SafetyViolation`. `sometimes` and `reachable` never map to `SafetyViolation` inside a lineage. `extract_bugs` creates bug records only for `SafetyViolation`.

### 2. Campaign liveness aggregation

The explorer keeps one campaign-wide record per assertion identity with hit and true counts across all branches. At the end of the run, a `sometimes` assertion with hits and no true observation, or a `reachable` assertion with no hits, becomes a `coverage_gap` entry in the report and assertion summary. Coverage gaps do not change the exit status.

### 3. Distinct counts

The report prints both the bug-record count and the distinct failing-assertion count. Bug records keep the existing dedup key so replay candidates stay unchanged.

### 4. Exit status

The CLI returns the bugs-found status only when at least one safety bug record exists. `distinguish-explorer-failure-outcomes` owns the error status.

## Risks

- An existing workload may use `sometimes` to encode a safety property. Validation lists every `sometimes` assertion in the maintained workloads and records any that must become `always`.
