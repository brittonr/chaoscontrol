## Why

The replay-readiness check now emits JSON, a one-line summary, and a static dashboard artifact, but the repository front page still requires operators to run or open artifacts to see the current bounded Antithesis-alternative claim. A generated README snippet makes the latest committed checks-only posture visible at rest.

## What Changes

- Add a deterministic README status snippet updater backed by the replay-readiness receipt summary.
- Add a committed README snippet between explicit markers.
- Package the updater as a Nix app and document how to refresh it from a receipt.

## Capabilities

### Modified Capabilities
- `replay-readiness-operator`: Adds an at-rest README status surface derived from the same receipt contract as CI artifacts.

## Impact

- **Files**: `README.md`, `scripts/update-replay-readiness-readme-status.py`, `flake.nix`, OpenSpec.
- **APIs**: New `replay-readiness-readme-status <receipt.json> --readme README.md` app.
- **Testing**: Script self-test, Nix app self-test, replay-readiness check, OpenSpec validation, whitespace check.
