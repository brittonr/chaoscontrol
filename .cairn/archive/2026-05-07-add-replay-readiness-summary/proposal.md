## Why

Replay readiness receipts are machine-readable, but CI logs and simple dashboards still need a tiny consumer that turns one receipt into a stable operator summary line without bespoke JSON parsing in every job.

## What Changes

- Add a script and Nix app that reads a replay readiness receipt JSON file.
- Print one concise summary line with pass/fail status, static gate counts, dogfood status, and failed phase when present.
- Document the consumer alongside the `replay-readiness --receipt` workflow.

## Impact

- **Files**: `scripts/`, `flake.nix`, README, replay-parent-snapshots OpenSpec.
- **APIs**: `summarize-replay-readiness-receipt <receipt.json>` and `nix run .#replay-readiness-summary -- <receipt.json>`.
- **Testing**: Run the consumer against a live generated receipt and a synthetic failed receipt; validate touched OpenSpec and build the Nix app.
