# ChaosControl Local Operator Triage Runbook

Generated from a replay-readiness receipt and `dogfood-results/accepted-workload-proofs.json`. Do not scrape `run.log`, `reproduce.log`, or temporary VM logs for the triage decision. Use only the bounded artifacts and status below.

## Receipt entry point

- Summary: `replay-readiness status=passed exit=0 static_gates=2/2 failed_gates=none dogfood=skipped failed_phase=none scope=bounded`
- Selected workload: `all committed proofs`
- Scope: bounded committed replay/evidence readiness; not hosted product parity and not universal determinism.

## Triage steps

1. Open the readiness receipt and dashboard/summary artifacts for status only.
2. Treat every listed bug, verdict, and snapshot as historical diagnostic data.
3. Do not run reproduction or minimization for these ID-only carriers.
4. Record `blocked-assertion-identity` until fresh admitted v2 KVM evidence exists.

## Workloads

### `raft` — blocked assertion identity

- Historical bug: `dogfood-results/raft-fresh-v2-proof-20260809/bug_4.json`
- Historical replay verdict: `dogfood-results/raft-fresh-v2-proof-20260809/replay-verdict-bug4.json`
- Historical snapshot: `dogfood-results/raft-fresh-v2-proof-20260809/snapshots/e91a40b4d06037dc463756248745dc2973560284c31a4e0d7c9561d44a8ea5ae.snapshot.bin`
- Status: `blocked-assertion-identity`
- Blocker: snapshot-ref.codec: unsupported

Do not reproduce, minimize, or promote this ID-only carrier. Generate fresh admitted v2 KVM evidence first.

### `redb` — blocked assertion identity

- Historical bug: `dogfood-results/redb-fresh-v2-proof-20260809/bug_0.json`
- Historical replay verdict: `dogfood-results/redb-fresh-v2-proof-20260809/replay-verdict-bug0.json`
- Historical snapshot: `dogfood-results/redb-fresh-v2-proof-20260809/snapshots/a2eba3c13b009865adf837500752d3de92c5e003e0fd3dbc13dedfcdc77104c4.snapshot.bin`
- Status: `blocked-assertion-identity`
- Blocker: snapshot-ref.codec: unsupported

Do not reproduce, minimize, or promote this ID-only carrier. Generate fresh admitted v2 KVM evidence first.

### `net` — blocked assertion identity

- Historical bug: `dogfood-results/net-fresh-v2-proof-20260809/bug_0.json`
- Historical replay verdict: `dogfood-results/net-fresh-v2-proof-20260809/replay-verdict-bug0.json`
- Historical snapshot: `dogfood-results/net-fresh-v2-proof-20260809/snapshots/17f24c2485277dde53e8d1787e5f5d0cc7f84e1b23631f22c13933821110a662.snapshot.bin`
- Status: `blocked-assertion-identity`
- Blocker: snapshot-ref.codec: unsupported

Do not reproduce, minimize, or promote this ID-only carrier. Generate fresh admitted v2 KVM evidence first.

### `rust-workload` — blocked assertion identity

- Historical bug: `dogfood-results/rust-workload-fresh-v2-proof-20260809/bug_0.json`
- Historical replay verdict: `dogfood-results/rust-workload-fresh-v2-proof-20260809/replay-verdict-bug0.json`
- Historical snapshot: `dogfood-results/rust-workload-fresh-v2-proof-20260809/snapshots/36400b0af56e8792c2c6729eaabbcd870650dcf5ad0a05aaff6cee5e36e5a9f9.snapshot.bin`
- Status: `blocked-assertion-identity`
- Blocker: snapshot-ref.codec: unsupported

Do not reproduce, minimize, or promote this ID-only carrier. Generate fresh admitted v2 KVM evidence first.
