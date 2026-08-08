## Why

ChaosControl now has accepted bounded replay proofs for `raft`, `redb`, `net`, and `rust-workload`, plus a generated assertion-readiness report. That report shows meaningful instrumentation gaps: uncategorized assertions across all accepted workloads, unhit assertions for `raft`/`redb`, and non-passing assertions in every accepted workload row. Those gaps are useful operator guidance, but they can be overclaimed if a workload is promoted beyond bounded replay proof without an explicit assertion-readiness decision.

## What Changes

- Add an assertion-readiness promotion gate that separates bounded replay proof from richer workload instrumentation readiness.
- Require generated assertion-readiness reports to preserve gap summaries and anti-claim language.
- Require negative fixtures/self-tests for report edits that hide uncategorized, unhit, or non-passing assertion gaps.
- Wire the gate into the cheap static readiness path before any stronger workload-support claim can be made.

## Capabilities

### Modified Capabilities
- `assertion-catalog`: adds promotion-boundary requirements for assertion-readiness reports and workload instrumentation claims.

## Impact

- **Files**: expected implementation touches `scripts/generate-assertion-readiness-report.py`, a new or extended assertion-readiness checker, fixtures, `flake.nix`, and canonical OpenSpec specs when archived.
- **APIs**: no runtime SDK or guest API changes.
- **Testing**: strict OpenSpec validation, report `--check`, negative fixture/self-test coverage, and focused Nix readiness/evidence checks.
