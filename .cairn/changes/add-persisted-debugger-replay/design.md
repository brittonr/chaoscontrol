# Design: Persisted Snapshot-Backed Debugger Replay

## Context and Ownership

The current recording serde boundary omits the entire `CheckpointStore`, not merely RAM bytes (`recording.rs`, `checkpoint.rs`). `serialize::load_recording` only reads and validates metadata. The real runner also reconstructs omitted VM configuration from defaults (`replay.rs:91-123`). Consequently, persisting one snapshot ref is insufficient for exact non-default SMP replay. `Debugger::goto` must also stop using a weaker validation loop than `ReplayEngine::replay_from`.

The current consumer is the existing Rust-workload reproduction CLI. `chaoscontrol-replay` owns the persisted-session shell and debugger; `chaoscontrol-explore` owns capturing a fresh reproduction; neutral snapshot reference and assertion DTOs remain in `chaoscontrol-replay-evidence-core`. No new crate, second VMM, or second snapshot format is introduced.

## Decisions

### 1. Reuse one snapshot persistence implementation

Move the existing `SnapshotStore`, `FileSnapshotStore`, artifact envelope, codec, bounds, and their behavioral fixtures from explorer into the replay library. Add the replay dependency to explorer and migrate all store imports and callers; remove the obsolete explorer module instead of retaining a re-export. The observed dependency graph permits `explore -> replay -> vmm/fault/protocol` without a cycle. Keep snapshot codec/schema and SHA-256 byte identity unchanged, as existing public evidence already binds them. New session and configuration identities use domain-separated canonical BLAKE3 framing. Do not move filesystem work into `replay-evidence-core` or `sim-core`.

### 2. Persist a versioned session, not a lossy reconstruction

A Rust-owned exact-session envelope explicitly contains:

- session schema/profile identity and a bounded recording interval with first and last simulation tick;
- the complete deterministic execution configuration, VM/vCPU and device topology, CPU/clock/progress policy, instruction quantum and correction settings, seed, canonical fault schedule, fault run identity/sequence, and artifact identities needed to create the same controller;
- kernel, initrd, and optional base-disk identity, byte length, role, and confined locator; a host path alone never identifies their content;
- an ordered checkpoint table containing checkpoint ID, tick, replay-parent depth, existing snapshot artifact reference, and bindings to the session's topology, configuration, and fault-ledger prefix;
- canonical fault-stage records and per-tick/per-VM SMP schedule traces for the recorded continuation, including evidence needed to identify an intentionally empty trace rather than silently accept an omitted one;
- optional exact bug, v2 assertion descriptor/catalog identity, original replay-parent provenance, and verdict reference when produced from a finding.

Checkpoint bytes remain separate artifacts. A session can reuse existing local artifacts by a validated reference or copy their exact bytes beneath its artifact root without changing their identity. It cannot synthesize missing history from serial logs. Duplicate checkpoints, reversed ticks, unknown schema/codec, absent deterministic inputs, trace holes, or inconsistent identities are admission failures, not reasons to fill defaults.

### 3. Capture from the existing reproduction path

`chaoscontrol-explore reproduce --debug-recording-output <directory>` records a bounded fresh continuation while preserving the existing reproduction/verdict exit contract. It first validates and restores the selected original parent and exact assertion identity as today. After explicit counterfactual-run/reset operations, it captures a new session-start checkpoint before guest progress and records that new run's identity and ledger. The original parent and bug remain provenance; pre-parent history is not fabricated. Recording drains each completed round's canonical traces through the existing recorder path and takes checkpoints under the declared profile.

Publish the completed manifest only after all selected snapshots, traces, configuration, and artifact bindings have validated and been retained. A write failure leaves an explicitly incomplete output, not a success-shaped session. Preserve any previously completed session. Publication does not claim power-loss durability merely because a rename succeeded. The manifest records the checked bounded outcome; producing it does not upgrade the original bug's replay class.

### 4. Admit before machine effects

A pure session validator receives supplied metadata, decoded snapshot facts, artifact digest observations, topology/capability facts, requested target, and named limits. It returns a complete selected restore/continuation plan or typed blockers. The shell performs bounded, no-follow, capability-rooted file reads, decoding, hashing, host admission, KVM calls, and publication. Lexical path checks alone are not no-follow I/O.

Validate the manifest, every artifact needed for the selected checkpoint/continuation, the complete execution descriptor, assertion binding when present, exact target range, and snapshot topology before VM creation or mutation. Do not follow absolute/traversing/symlink locators, fetch remote artifacts, accept mismatched bytes, or silently substitute a boot replay for an explicitly requested checkpoint. Existing full-snapshot preflight remains authoritative. The shell may remap validated artifact locations explicitly; it may not change content identity.

### 5. One exact continuation path

Factor the existing exact replay loop so replay and debugger seek call the same continuation path and validators. Validate actual fault outcomes and SMP schedule traces at each round, the selected fault interval, and arrival at the exact target. Only commit the debugger's displayed position and inspected state after those checks pass. Rewind selects the latest admitted checkpoint at or before the target in deterministic tick/ID order; targets before session start or after its declared end fail before mutation.

A divergence, poisoned controller, missing evidence after progress, or premature stop invalidates the attempted position. Dispose of the affected runner; a later operation must start from an independently admitted checkpoint on a fresh runner. Keep last verified metadata separate from any failed live state. Host watchdogs are operational failures, never proof of a guest deadlock or successful seek.

### 6. Separate exact inspection from counterfactual edits

Read-only navigation and memory/register inspection describe the verified original session. Existing explicit memory/register changes and modified schedules create counterfactual state and cannot be rendered as a verified position of the original trace. This change does not redefine whether live edits persist across `goto` or add source-level stepping. A newly captured counterfactual session can later be checked against its own retained continuation, never against the original divergent suffix.

Historical JSON without checkpoint inventory, exact execution descriptor, or bound traces remains available to `info`/`events` diagnostics. Exact-session replay/inspect rejects it with the missing prerequisites; there is no compatibility alias or lossy migration that manufactures proof.

### 7. Named finite admission and execution limits

A Nickel-backed review profile names maximum manifest bytes, checkpoint count, artifact count, compressed and decoded snapshot bytes, per-VM trace records, retained output bytes, replay ticks, and host execution deadline. Rust revalidates the exported profile and owns runtime records. Reuse current snapshot limits where applicable rather than inventing new numeric defaults. Reject arithmetic overflow and over-budget inputs before allocating or executing beyond those limits. A deadline is operational evidence and is excluded from deterministic guest state.

## Verification and Acceptance

- Before implementation, run the focused replay/explorer baseline and capture one real fresh-session gap; exported symbol moves require reference discovery before caller migration.
- Positive portable fixtures cover a complete persisted checkpoint table, exact non-default topology/configuration, allowed artifact relocation with unchanged bytes, equal replay/debugger validation, exact start/end targets, and explicitly empty evidence slices.
- Negative fixtures cover unknown/malformed schemas, duplicate or missing checkpoints, unsafe paths and symlinks, missing/truncated/wrong-size/corrupt snapshots, wrong guest/config/catalog/run identities, forged/reordered/omitted SMP and fault records, target overflow, exhausted bounds, preflight incompatibility, premature stop, publication failure, and poisoned-runner reuse.
- The actual bounded KVM workflow must reproduce an admitted Rust finding with recording enabled, terminate that producer process, then reload the retained session in separate `chaoscontrol-replay replay` and `inspect` invocations. Seek backward and forward across a persisted checkpoint, inspect a declared memory/register value, and compare canonical continuation observations with the control. Include a non-default SMP profile so omitted configuration cannot pass by coincidence.
- Tamper one retained schedule artifact and remove one snapshot in isolated copies; both CLI paths must reject the operation without exposing a success position. An affected deep overlay is blocked under the lineage owner's contract, not repaired here.
- Run focused replay/explorer/evidence tests, Clippy for touched crates, applicable evidence and Nix KVM rails, and Cairn validation/gates after implementation. Update replay usage and artifact-retention documentation then, not by asserting this spec-only package has executed the new workflow.

## Alternatives Rejected

- Inline RAM in recording JSON: duplicates large state and evades the existing bounded snapshot boundary.
- Add another file-store implementation in replay or make replay depend on the explorer product shell: duplicates validation or reverses ownership. Relocating the current store makes replay the shared owner.
- Recreate omitted configuration or checkpoints from defaults/boot: changes the requested execution and hides missing evidence.
- Adopt Rewind's kernel, GUI, or page store first: not necessary to connect existing ChaosControl primitives and changes the scope or licensing boundary.

## Risks and Claim Boundaries

Moving a public store module affects all explorer callers; migrate them atomically and keep serialized bytes unchanged. Reproduction recording adds bounded capture overhead; do not turn host timing into guest scheduling authority. Source-inspected debugger gaps are not claims that every historical recording is corrupt. Known deep incremental-lineage gaps remain owned by `repair-incremental-snapshot-lineage`. Only named admitted fixtures and hosts can gain execution evidence. A verified seek, gate pass, or saved session does not prove whole-system correctness, global causality, snapshot crash durability, cross-host equivalence, or a new accepted replay verdict.

The immutable RewindVM comparison and its MIT-engine/GPL-kernel/proprietary-app reuse boundary are recorded in `proposal.md`; no upstream code or runtime dependency is imported.
