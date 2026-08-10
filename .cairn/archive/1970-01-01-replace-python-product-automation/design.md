## Context

Rust owns runtime evidence DTOs and most readiness policy, but Python still joins several product workflows. Each script can drift from Rust validation and retention rules.

## Decisions

### 1. Inventory behavior before replacement

For every script and inline block, record inputs, outputs, exit codes, bounds, side effects, failure classes, Nix callers, and public command names. Preserve representative positive and negative fixtures.

### 2. Domain owners receive the logic

Dogfood, receipts, summaries, and KVM smoke belong in evidence or replay owners. Audit report policy belongs in a focused repository tool. Scaffold transformation belongs in the workload harness owner.

### 3. Pure cores own decisions

JSON parsing into typed DTOs, validation, classification, summary models, manifest construction, and audit policy are deterministic functions. Shells own process execution, file reads, writes, directories, and terminal output.

### 4. Nix wraps compiled tools

Nix apps invoke Rust binaries directly. Tiny shell glue can set paths and call one binary, but it cannot parse JSON or decide evidence status.

### 5. Cutover uses parity gates

Run old and new implementations over frozen positive and negative fixtures. Compare canonical outputs, exit classes, artifact plans, and diagnostics. Live command names change owners only after parity passes.

### 6. Removal is last

Delete Python scripts, inline blocks, and Python runtime inputs only after all callers use Rust. A source guard prevents product policy from returning to inline Python.

### 7. Claims do not increase

Rust migration improves ownership and testability. It does not prove orchestration correctness, KVM behavior, audit completeness, or release eligibility.

## Risks

Exact diagnostic text can differ across languages. Freeze machine-readable schemas and error classes. Permit reviewed prose changes only when tests and docs update together.
