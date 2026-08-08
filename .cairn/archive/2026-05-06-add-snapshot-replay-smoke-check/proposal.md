# Change: Add snapshot replay smoke check

## Why

Snapshot-backed replay now has committed dogfood proof, but the proof path is not a repeatable gate. A small KVM smoke check should exercise the same real run/export/reproduce rail so regressions in replay parent snapshot persistence, artifact validation, or CLI propagation fail before another dogfood stint.

## What Changes

- Add an explicit snapshot replay smoke check target.
- Run the bounded Raft `snapshot_replay_probe` workload with small memory and deterministic parameters.
- Finalize checkpoint-held bugs with `chaoscontrol-explore export-bugs`.
- Assert at least one exported bug has `replay_parent_depth > 0` and a non-null `replay_parent_snapshot_ref`.
- Verify the referenced snapshot artifact is present, content-addressed, digest-matching, and reproducible via standalone `reproduce`.

## Impact

- Adds an opt-in KVM-required Nix check for local/CI hosts with `/dev/kvm`.
- Keeps raw run/reproduce logs ephemeral in the build directory.
- Does not add committed dogfood evidence unless a human intentionally curates it.
