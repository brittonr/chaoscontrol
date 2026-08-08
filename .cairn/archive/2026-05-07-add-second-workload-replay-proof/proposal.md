## Why

ChaosControl now has accepted snapshot-backed replay evidence for the Raft probe rail, but a single workload does not show the replay path generalizes beyond one guest shape. The next Antithesis-alternative slice should add a second independent workload proof using the existing machine-readable verdict/export evidence path.

## What Changes

- Add a second-workload replay proof requirement for snapshot-backed accepted verdict evidence from a non-Raft guest.
- Reuse the existing filtered `export-bugs` and `--verdict-output` rail.
- Allow the accepted dogfood wrapper to parameterize workload assertion ID, cmdline, and optional disk image so it can exercise redb as a second workload without duplicating logic.
- Keep raw logs/checkpoints/attempts local or ignored; commit only concise receipts, hashes, selected bug/verdict, and any required snapshot artifact.

## Impact

- **Specs**: `replay-parent-snapshots` gains a second-workload evidence scenario.
- **Code**: redb guest may gain a bounded snapshot replay probe; dogfood wrapper gains workload parameters.
- **Testing**: targeted Rust checks, wrapper syntax/help, evidence contracts, and one accepted non-Raft dogfood receipt.

## Out of Scope

- Claiming global hypervisor determinism.
- Replacing the Raft smoke gate.
- Committing raw runtime logs, checkpoints, disk images, or full attempt directories.
