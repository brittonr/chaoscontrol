## Why

ChaosControl now publishes bounded replay-readiness surfaces for accepted workloads, but fresh workload authoring is still explicitly experimental. A new workload should not be promoted accidentally by editing the accepted manifest or generated docs without preserving anti-claims, snapshot-backed replay requirements, and the experimental/unproven surface classifications.

## What Changes

- Add a deterministic promotion gate that cross-checks the accepted workload manifest against the generated readiness report.
- Require stable anti-claim text and unique workload/assertion identities for accepted proofs.
- Add negative self-tests so depth-zero/schedule-only style evidence and missing experimental classifications cannot silently become supported claims.

## Capabilities

### Modified Capabilities
- `replay-readiness-operator`: adds a fresh-workload promotion gate to the static readiness checks.

## Impact

- **Files**: `scripts/check-readiness-promotion-gate.py`, `flake.nix`, OpenSpec replay-readiness operator spec.
- **APIs**: no runtime API changes.
- **Testing**: Python self-test, strict OpenSpec validation, replay-readiness/evidence-contract Nix checks.
