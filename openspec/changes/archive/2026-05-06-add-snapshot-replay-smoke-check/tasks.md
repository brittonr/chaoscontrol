# Tasks

## 1. Spec

- [x] Add a replay-parent-snapshots requirement for a repeatable snapshot replay smoke gate.
- [x] Validate the change strictly before implementation.

## 2. Implementation

- [x] Add a bounded smoke script that runs the Raft snapshot replay probe, exports bugs, verifies snapshot refs/digests, and reproduces a selected bug.
- [x] Wire the script into a KVM-required Nix check named `snapshot-replay-smoke`.
- [x] Keep raw run/reproduce logs out of committed evidence.

## 3. Verification

- [x] Run the smoke script/check locally.
- [x] Run targeted formatting, OpenSpec, and evidence validation checks.
- [x] Archive the OpenSpec change after verification.
