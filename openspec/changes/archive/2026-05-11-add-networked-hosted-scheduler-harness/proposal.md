## Why

Replay readiness now has bounded local scheduler execution, restart-persistent fleet state, local multi-hypervisor KVM smoke evidence, and a loopback hosted/shared-state harness. The remaining competitor-style gap is proving the same queue/lease/decision-store semantics across independently started machine identities rather than a single-process loopback executor.

## What Changes

- Add a bounded networked hosted scheduler harness that starts separate worker identities against a shared queue/decision-store adapter.
- Bind each worker's leases, run receipts, decision revisions, state snapshots, and health events into one receipt without raw-log scraping.
- Keep the claim narrow: loopback/networked integration evidence only, not SaaS hosting, auth, billing, public UI, universal fleet scale, or Antithesis parity.

## Capabilities

### Modified Capabilities
- `replay-readiness-operator`: Adds a networked hosted scheduler integration seam above the existing loopback shared-state harness.

## Impact

- **Files**: `crates/chaoscontrol-evidence`, scheduler receipt CLI/Nix packaging, generated replay readiness docs.
- **APIs**: New bounded receipt/plan modes for networked hosted scheduler execution.
- **Dependencies**: No external service dependency; first implementation should use loopback TCP or file-backed shared state with independently spawned worker processes.
- **Testing**: Focused receipt/model tests, negative validator fixtures, CLI smoke, generated report check, OpenSpec strict validation, and replay-readiness Nix check.
