## Why

ChaosControl now has static fleet triage and local decision receipts, but it still lacks a bounded artifact for planning multiple replay-readiness runs without implying a hosted scheduler.

## What Changes

- Add a Rust-owned local scheduler receipt model and validator.
- Add a CLI that writes a sample scheduler receipt and validates existing receipts.
- Package the scheduler receipt artifact in the replay-readiness check output.
- Update the generated readiness status to keep scheduler orchestration local and non-hosted.

## Impact

- Files: `chaoscontrol-evidence`, `flake.nix`, generated replay readiness docs.
- APIs: new evidence helpers and `replay-readiness-scheduler-receipt` CLI.
- Testing: focused evidence tests, CLI check/sample smoke, OpenSpec validation, Nix replay-readiness packaging.
