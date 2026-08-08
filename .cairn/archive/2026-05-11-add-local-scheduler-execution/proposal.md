## Why

The scheduler receipt format records a bounded multi-run plan, but operators still need a local execution receipt that proves the plan can run and link each run to replay-readiness receipts without introducing a hosted scheduler or shared queue.

## What Changes

- Add a bounded sequential scheduler execution receipt model.
- Extend `replay-readiness-scheduler-receipt` with `--run-plan` and `--check-execution` modes.
- Package scheduler execution evidence in the replay-readiness Nix check.
- Update generated readiness status from local scheduler receipts to local scheduler execution while preserving hosted/fleet non-claims.

## Impact

- Files: `chaoscontrol-evidence`, `flake.nix`, generated replay readiness docs.
- APIs: new scheduler execution validator/exported helper and CLI modes.
- Testing: positive/negative model tests, CLI smoke, evidence tests, OpenSpec validation, Nix replay-readiness check.
