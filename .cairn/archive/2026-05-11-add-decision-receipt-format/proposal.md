## Why

The static fleet triage index made multi-receipt review visible, but the next missing workflow surface was a durable operator decision artifact. Without a receipt format, review outcomes still live in ad hoc notes and can be mistaken for a hosted/shared decision store once copied between CI artifacts.

## What Changes

- Add a bounded local replay-readiness decision receipt format.
- Add a CLI to write a sample receipt and validate committed/local receipts.
- Package the receipt next to replay-readiness CI artifacts.
- Update readiness status language without promoting hosted UI, scheduler, shared store, or product parity.

## Capabilities

### Modified Capabilities
- `replay-readiness-operator`: Adds local decision receipt format support for fleet-style triage review.

## Impact

- **Files**: `chaoscontrol-evidence` model/CLI/tests, `flake.nix`, README, replay-readiness status docs.
- **APIs**: Public evidence crate helpers for sample/write/validate decision receipts.
- **Testing**: Focused evidence tests, drift/selftest checks, CLI smoke, OpenSpec strict validation, Nix package/check build.
