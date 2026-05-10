## Why

Accepted workload assertion summaries still report every assertion as `uncategorized` even when the current guest source already names meaningful assertion categories. That makes the assertion-readiness surface less useful for deciding which redb gaps to remediate first, and it blocks cheap operator-trust progress without requiring a fresh VM campaign.

## What Changes

- Add deterministic category inference for committed accepted-proof assertion artifacts when an artifact lacks category metadata.
- Keep inferred categories distinct from runtime trace mutation: the generated report should show the effective category and its source.
- Preserve fail-closed promotion behavior for unhit and non-passing gaps.

## Capabilities

### Modified Capabilities
- `assertion-catalog`: accepted-proof readiness reports can categorize legacy/metadata-poor assertion summaries without editing runtime artifacts.

## Impact

- **Files**: `crates/chaoscontrol-evidence`, `docs/assertion-readiness-status.md`, OpenSpec assertion-catalog spec.
- **APIs**: no public runtime API changes.
- **Testing**: focused evidence tests, report check, promotion gate selftest, OpenSpec validation, and cheap Nix evidence contract if feasible.
