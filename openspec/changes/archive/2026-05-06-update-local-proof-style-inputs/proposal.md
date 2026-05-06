## Why

ChaosControl already carries local verified/structured code, but its flake did not name the sibling proof and style sources that guide the next hardening slices. Pinning the current `../tigerstyle` and `../verified-logic` checkouts makes the toolchain provenance explicit and gives the workspace a repeatable verified-logic proof rail.

## What Changes

- Add flake inputs for the latest local sibling `../tigerstyle` and `../verified-logic` HEADs.
- Expose the pinned `verified-logic` package in ChaosControl packages and dev shells.
- Add a `verified-logic-verus-proofs` flake check so ChaosControl can evaluate and build the sibling proof rail from the same lock.
- Expose pinned Tigerstyle cargo and standards tools plus its policy-registry check using Tigerstyle's own flake toolchain pins.

## Capabilities

### New Capabilities
- `local-proof-style-inputs`: The flake records sibling Tigerstyle and verified-logic revisions used by ChaosControl hardening work.
- `verified-logic-proof-rail`: The flake can evaluate and build the pinned verified-logic Verus proof check.

## Impact

- **Files**: `flake.nix`, `flake.lock`, OpenSpec records.
- **Build**: Adds upstream verified-logic proof check to `nix flake check`.
- **Developer environment**: Adds the pinned verified-logic package to the default dev shell.
- **Non-goals**: This change does not vend or rewrite ChaosControl runtime code, and does not hard-gate Tigerstyle lints yet.
