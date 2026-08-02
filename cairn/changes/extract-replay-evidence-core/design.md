# Design: Extract the replay evidence core

## Context

`replay_verdict.rs` currently contains pure verdict DTOs and classifiers beside `write_verdict`, filesystem cleanup, and `new_run_id`, which reads `SystemTime`. Evidence-readiness code consumes overlapping replay concepts. The current package boundary cannot guarantee that both sides use one DTO and classification authority.

## Goals

- Give explorer emission and evidence validation one Rust-owned replay model.
- Make replay classification testable from in-memory facts.
- Keep runtime and host effects in shells.
- Preserve public artifacts during migration.

## Decisions

### 1. Add one shared replay evidence crate

Create `chaoscontrol-replay-evidence-core`. It owns:

- `ReplayVerdict` and replay class values;
- artifact hash DTOs and validation;
- snapshot and replay-parent references;
- snapshot-validation status values;
- reproduced, schedule-only, gap, invalid, and no-bug classification;
- bounded diagnostics and compatibility validation.

The crate accepts owned or borrowed in-memory facts and returns data. It does not read an artifact path to obtain those facts.

### 2. Keep all effects in shell crates

`chaoscontrol-explore` keeps:

- run-ID allocation from the selected shell policy;
- bug, snapshot, and checkpoint reads;
- VM and replay execution;
- verdict serialization and create-new file publication;
- cleanup, logging, and command rendering.

`chaoscontrol-evidence` keeps evidence-tree discovery, file reads, Nickel review-boundary checks, and report output.

### 3. Preserve the accepted wire format

Freeze representative verdict bytes before migration. Compatibility adapters retain current field names, enum spellings, optional-field behavior, and diagnostic classes.

The public artifact hash remains SHA-256 where the accepted replay format requires it. This is an interoperability constraint, not a new ChaosControl hashing default. New internal content identities use BLAKE3 unless another accepted format requires a different algorithm.

### 4. Use explicit run and observation facts

The core receives run identity, command summary, exit observation, bug facts, snapshot-validation facts, and artifact hashes. It never calls a clock or derives filesystem state.

`new_run_id` remains a shell concern. A later change can replace its current time-derived policy without changing the core API.

### 5. Enforce one-way dependencies

Explorer and evidence shells depend on the replay core. The replay core cannot depend on either shell. Source and dependency checks reject filesystem, environment, process, network, KVM, clock, async-runtime, logging, CLI, and Nickel runtime imports.

## Alternatives

### Keep duplicate DTOs with conversion functions

Rejected. Conversions do not prevent semantic drift between emitter and validator.

### Move all replay orchestration into the core

Rejected. Artifact discovery, VM execution, snapshot access, and verdict publication are shell effects.

### Expand this change to the full explorer campaign loop

Deferred. The replay evidence seam is smaller and already has shared consumers. Campaign coordination needs a separate ownership and observation inventory.

## Risks and Controls

- **Wire drift**: freeze canonical positive and negative fixtures before moving types.
- **Evidence policy moves into a generic DTO crate**: limit the core to replay artifact admission and classification named by this package.
- **Algorithm confusion**: retain explicit algorithm tags and document SHA-256 interoperability fields.
- **Shell logic duplicates classification**: add cross-crate parity tests and source checks.
- **Passing tests are overclaimed**: receipts state the exact source and fixture scope.

## Verification

Run current focused replay verdict and evidence tests before changes. After extraction, run core unit tests, explorer emission tests, evidence-readiness tests, JSON fixture parity, compile and source-policy negatives, Cargo formatting and Clippy, Cairn validation and gates, and the relevant Nix checks.
