## Context

Some KVM tests run only on suitable hosts. Broad CI can pass without proving that the selected release revision completed the required behavior matrix.

## Decisions

### 1. Nickel owns the release matrix

The matrix binds required rows, host capability predicates, exact commands, finite limits, artifact retention, and claim boundaries. Runtime observations remain Rust-owned.

### 2. Required rows are explicit

The initial required set covers exact deterministic SMP behavior, serialized snapshot replay, production virtio malformed-input survival, one admitted drift profile, and one fresh workload replay row. PMU rows are required only for a profile that claims PMU support.

### 3. Unsupported is not success

A worker that lacks a required capability emits `unsupported` with the missing fact. A required unsupported, skipped, timed-out, or absent row blocks the release verdict.

### 4. Receipts bind the complete cohort

The receipt includes source revision, dirty state, runner revision, kernel and KVM capabilities, host architecture, built binaries, guest artifacts, matrix profile, command identities, limits, row outcomes, and retained artifact identities.

### 5. Classification is pure

The core validates matrix shape, capability matching, row freshness, artifact linkage, and terminal release class. The shell queries the host, runs rows, and publishes bounded artifacts.

### 6. CI has two lanes

Portable CI runs formatting, unit, property, schema, lifecycle, and Nix checks. The KVM lane runs only on admitted workers and returns a separate receipt.

## Risks

Trusted workers require maintenance and isolation. The receipt reports observed host facts, but it does not prove worker integrity or platform equivalence.
