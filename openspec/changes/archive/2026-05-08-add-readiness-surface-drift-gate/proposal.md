## Why

Replay-readiness now emits several operator surfaces: receipt JSON, one-line summary, dashboard HTML, README status snippet, and generated status reports. These surfaces must stay aligned as new static gates are added; otherwise CI can pass while the receipt/dashboard/README omit a gate or expose stale bounded-claim text.

## What Changes

- Add a deterministic generated-surface drift checker for replay-readiness operator artifacts.
- Cross-check static gate names wired in the `replay-readiness` shell against the receipt's static gate list.
- Exercise summary, dashboard, and README status rendering from the same sample receipt and fail closed on divergence.

## Capabilities

### Modified Capabilities
- `replay-readiness-operator`: adds generated-surface drift protection across receipt, summary, dashboard, and README status surfaces.

## Impact

- **Files**: `scripts/check-readiness-surface-drift.py`, `flake.nix`, OpenSpec replay-readiness operator spec.
- **APIs**: no runtime API changes.
- **Testing**: Python drift check/selftest, strict OpenSpec validation, replay-readiness/evidence-contract Nix checks.
