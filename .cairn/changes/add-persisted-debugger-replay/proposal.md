# Persisted Snapshot-Backed Debugger Replay

## Why

ChaosControl already has tick-based rewind, memory/register inspection, exact recording replay, and persisted explorer replay-parent snapshots. They are not one durable operator workflow. `Recording.checkpoints` is skipped by serialization in `crates/chaoscontrol-replay/src/recording.rs`; `Checkpoint.snapshot` is also skipped. `load_recording` validates JSON but does not reload checkpoint state. `validated_checkpoint_snapshot` rejects a checkpoint without its snapshot. In addition, `Debugger::goto` checks fault outcomes but does not perform the schedule-round comparison used by `ReplayEngine::replay_from`. These are source-inspection findings, not a newly executed ChaosControl failure campaign.

The current consumer is an operator inspecting a freshly reproduced Rust workload finding after the producer process exits. The immediate outcome is a saved session that a fresh `chaoscontrol-replay` process can seek without reconstructing snapshots or configuration from defaults. The durable capability is one validated persisted-session intake shared by exact replay and debugger navigation. The maintenance owner is `chaoscontrol-replay`, with `chaoscontrol-explore` owning the reproduction producer and `chaoscontrol-replay-evidence-core` retaining neutral evidence identities.

## What Changes

- Persist a versioned session envelope with explicit checkpoint inventory, existing snapshot references, bounded continuation traces, exact execution configuration, guest artifact identities, and optional exact v2 bug/assertion binding. r[chaoscontrol.replay_debugger.session]
- Move the existing explorer snapshot store and codec into the replay library and migrate every explorer caller. Keep the current snapshot codec, digest semantics, and neutral reference DTO; do not create a second reader, compatibility re-export, or storage abstraction. r[chaoscontrol.replay_debugger.shared_store]
- Validate the whole selected continuation and its inputs before creating or mutating a VM. Resolve paths only under the declared artifact root; do not fetch missing artifacts or silently boot instead of restoring a requested checkpoint. r[chaoscontrol.replay_debugger.admission]
- Make exact replay and debugger navigation use the same per-round fault and SMP schedule validation and exact-target checks. Preserve the distinction between original-run inspection and deliberate counterfactual mutation. r[chaoscontrol.replay_debugger.navigation]
- Extend the existing explorer reproduction path with `--debug-recording-output` to capture a bounded fresh session from an admitted replay parent, and wire existing replay/inspect commands to consume it in a new process. r[chaoscontrol.replay_debugger.producer]
- Fail explicitly on missing, corrupt, incompatible, incomplete, or divergent state without publishing a successful target position or reusing a poisoned runner. r[chaoscontrol.replay_debugger.failure_isolation]
- Keep historical metadata-only recordings diagnostic-only when they lack exact-session inputs. A debugger seek is not a new `snapshot_backed_reproduced` verdict. r[chaoscontrol.replay_debugger.claim_boundary]
- Prove fresh-process reload, bounded continuation, non-default topology/configuration, and adversarial intake failures through the actual CLI, with paired positive and negative fixtures. r[chaoscontrol.replay_debugger.validation]

## Impact

- **Code:** replay recording/checkpoint serialization, session admission, runner construction, debugger navigation, CLI intake, and explorer reproduction/recording output. Existing store callers and imports migrate in one cutover.
- **Configuration:** a Nickel-backed session profile supplies named limits for manifest bytes, checkpoint count, trace records, artifact bytes, replay ticks, and shell execution. Runtime session records and snapshots remain Rust-owned.
- **Compatibility:** new exact sessions are versioned explicitly. Existing SHA-256 snapshot refs remain validated under their current codec; new session/config identities use domain-separated BLAKE3. Old JSON stays available for metadata inspection, not implicit exact restore.
- **Verification:** focused replay/explorer/evidence tests; real reproduction, process exit, session reload, seek, and inspection; strict corruption, identity, prefix, target-bound, and publication-failure cases. No new runtime behavior has been implemented or exercised by authoring this package.

## Dependencies and Related Changes

- The accepted deterministic snapshot, fault-outcome, assertion-identity, and SMP trace contracts remain authoritative.
- `repair-incremental-snapshot-lineage` owns deep-overlay correctness. An affected or unvalidated lineage blocks the selected session; this change does not repair or promote old deep findings. A valid full snapshot can exercise this workflow independently.
- `evaluate-page-addressed-snapshots` may evaluate a different physical representation later; it is not a prerequisite for existing snapshot references.
- `add-counterfactual-replay-witnesses` can consume retained sessions, but witnesses and general causality attribution are not prerequisites for exact debugger intake.
- The installed Cairn/policy mismatch remains owned by `refresh-vendored-cairn-policy`; lifecycle commands for this package use the revision recorded in `cairn-policy/UPSTREAM.md` without changing vendored policy.

## Reference and Reuse Boundary

RewindVM at immutable revision [`6ec1f68de82ae3e657cdfcbe0d83f349bb367082`](https://github.com/fzakaria/rewindvm/tree/6ec1f68de82ae3e657cdfcbe0d83f349bb367082) supplies the design comparison: retained keyframes, seek-by-restore, and inspecting a fork of a run. Relevant sources are [`rewind-core/src/keyframes.rs`](https://github.com/fzakaria/rewindvm/blob/6ec1f68de82ae3e657cdfcbe0d83f349bb367082/crates/rewind-core/src/keyframes.rs) and [`run.rs`](https://github.com/fzakaria/rewindvm/blob/6ec1f68de82ae3e657cdfcbe0d83f349bb367082/crates/rewind-core/src/run.rs). This is not a new runtime dependency or a parity claim. Its engine is MIT, its Linux patch is GPL-2.0, and its desktop app is proprietary; this plan imports none of that code.

## Non-Goals

- No new VMM, patched guest kernel, cooperative scheduling mode, GUI, GDB protocol, historical filesystem shell, or source-level debugger.
- No change to existing inspection mutation semantics or a promise that live pokes survive a rewind; mutations remain explicitly counterfactual and cannot certify the original trace.
- No container intake, non-Rust SDK, hosted service, cross-machine scheduler, automatic artifact download, or new external storage adapter.
- No silent default reconstruction of omitted deterministic inputs, no boot fallback for an explicitly selected persisted checkpoint, and no acceptance of numeric-only historical assertion identities as proof.
- No sync, archive, implementation completion, replay promotion, or universal determinism claim from these planning documents.
