## Why

Local sequential scheduler execution proves multi-run orchestration on one machine, but the missing product surface is durable hosted/fleet scheduler evidence: a queue, workers, leases, linked run receipts, and operator decisions. This change adds a bounded receipt-backed fleet scheduler proof without claiming an always-on production service or Antithesis parity.

## What Changes

- Add a bounded hosted/fleet scheduler receipt model covering durable queue entries, worker leases, run receipts, and linked decision receipts.
- Extend the scheduler CLI with sample/check modes for the fleet receipt.
- Package the fleet scheduler receipt in the replay-readiness Nix check.
- Promote replay scheduler orchestration status from local execution to bounded fleet scheduler receipt while preserving live-service/product-parity non-claims.

## Impact

- Files: `chaoscontrol-evidence`, `flake.nix`, generated readiness docs.
- APIs: new fleet scheduler receipt sample/write/validate helpers and CLI modes.
- Testing: positive/negative model tests, CLI smoke, readiness report check, OpenSpec validation, Nix replay-readiness check.
