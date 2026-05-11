## Why

The readiness status now names hosted/fleet triage as the next missing feature. A full hosted service is still out of scope, but operators need a bounded multi-run artifact before any hosted claim can be made.

## What Changes

- Add a static fleet triage index renderer over one or more replay-readiness receipts.
- Expose it as a packaged CLI/Nix app.
- Preserve anti-overclaim wording: this is not hosted UI, scheduler integration, shared decision storage, or product parity.

## Capabilities

### Modified Capabilities
- `replay-readiness-operator`: Adds static multi-receipt fleet triage artifact support.

## Impact

- **Files**: `chaoscontrol-evidence`, `flake.nix`, README, generated replay readiness status.
- **Testing**: surface drift selftest, generated readiness report check/write, sample CLI smoke, targeted Cargo checks.
