## Why

ChaosControl now has a bounded local fleet scheduler runtime with restart-persistent queue state, but it still only proves command-level worker orchestration. The next narrower product-relevant step is to prove that one campaign can drive multiple local ChaosControl hypervisor instances through the same queue/lease/receipt model, without prematurely building a hosted service or distributed scheduler.

## What Changes

- Add a bounded local multi-hypervisor campaign runner contract for spawning multiple ChaosControl hypervisor workers from one campaign plan.
- Bind every hypervisor worker to a queue lease, run receipt, persisted queue-state update, and replay-readiness summary.
- Require fail-closed evidence for duplicate leases, missing worker/run links, crash/retry ambiguity, and hosted-service overclaims.
- Package campaign plan, fleet receipt, per-hypervisor run receipts, and queue-state proof in the replay-readiness output.
- Promote readiness wording only to local multi-hypervisor evidence; keep shared multi-machine hosted scheduling and product parity unpromoted.

## Impact

- Files: likely `chaoscontrol-evidence`, scheduler CLI/runtime code, `flake.nix`, generated readiness docs, and model tests.
- APIs: new campaign-plan/run/check modes or extensions to the existing scheduler receipt CLI.
- Testing: pure plan/receipt validation tests, negative duplicate/missing-link tests, focused local multi-hypervisor smoke if available, replay-readiness Nix packaging, readiness report check, and OpenSpec validation.
