# add-kvm-multi-hypervisor-smoke

## Why
The replay-readiness local multi-hypervisor campaign runner is receipt-backed, but operators still need a directly runnable KVM smoke rail that drives real accepted-verdict dogfood commands through that campaign runner instead of sample receipts.

## What Changes
- Add a bounded local KVM smoke wrapper that builds a multi-hypervisor campaign plan using real `replay-readiness --dogfood <workload>` commands.
- Package the wrapper as a Nix app and KVM-required check.
- Require the rail to emit plan, queue state, campaign receipt, per-run replay-readiness receipts, and a summary without raw-log scraping or hosted-service claims.
