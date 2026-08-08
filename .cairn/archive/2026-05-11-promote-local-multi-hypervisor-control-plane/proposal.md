## Why

The full flake baseline is green and the local KVM multi-hypervisor smoke rail now proves real replay-readiness dogfood execution through two local hypervisor worker identities. The README still treats the local multi-hypervisor control plane as a missing feature, even though the durable receipt model, queue-state persistence, worker budgets, artifact roots/indexes, follow-up jobs, dashboard, and KVM smoke rail exist.

This change promotes that bounded one-machine control-plane surface from an active gap to the supported local workflow while preserving strict anti-overclaim boundaries.

## What Changes

- **Readiness status**: Reclassify `Local multi-hypervisor control plane` from `active-local-gap` to a supported local control-plane status backed by durable receipts and KVM smoke evidence.
- **Promotion gate**: Require the generated status row to cite durable queue state, worker budgets, artifact roots/indexes, follow-up jobs, KVM smoke, and non-hosted/non-cross-machine scope.
- **Roadmap**: Move local multi-hypervisor control-plane promotion out of the missing-feature list and into the completed baseline.

## Capabilities

### Modified Capabilities
- `replay-readiness-operator`: Local multi-hypervisor control-plane readiness and anti-overclaim gating.

## Impact

- **Files**: `crates/chaoscontrol-evidence/src/lib.rs`, `crates/chaoscontrol-evidence/src/readiness_promotion_gate.rs`, `docs/replay-readiness-status.md`, `README.md`, and this OpenSpec change.
- **APIs**: No public SDK or VMM API changes; this is a readiness/status promotion over existing receipt surfaces.
- **Dependencies**: None.
- **Testing**: Run the promotion gate/report check, focused evidence tests, OpenSpec strict validation, and the replay-readiness Nix rail.
