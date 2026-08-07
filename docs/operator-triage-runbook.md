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

- Historical bug: `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/bug_0.json`
- Historical replay verdict: `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/replay-verdict-bug0.json`
- Historical snapshot: `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/snapshots/cc0161208b3e591ef79625c902e7418aa70a2f33e1445095790e5202265511d2.snapshot.bin`
- Status: `blocked-assertion-identity`
- Blocker: bug-report: legacy assertion ID-only evidence cannot promote

Do not reproduce, minimize, or promote this ID-only carrier. Generate fresh admitted v2 KVM evidence first.

### `redb` — blocked assertion identity

- Historical bug: `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/bug_0.json`
- Historical replay verdict: `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/replay-verdict-bug0.json`
- Historical snapshot: `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/snapshots/bacc336ca613083d1276472e79fe6845220205c30582dbac93cd9537629134ac.snapshot.bin`
- Status: `blocked-assertion-identity`
- Blocker: bug-report: legacy assertion ID-only evidence cannot promote

Do not reproduce, minimize, or promote this ID-only carrier. Generate fresh admitted v2 KVM evidence first.

### `net` — blocked assertion identity

- Historical bug: `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/bug_0.json`
- Historical replay verdict: `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/replay-verdict-bug0.json`
- Historical snapshot: `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/snapshots/0a1d6142c36dd4fc0f875deffdef316e7965c5b310b0b96ec0434789fe047dd5.snapshot.bin`
- Status: `blocked-assertion-identity`
- Blocker: bug-report: legacy assertion ID-only evidence cannot promote

Do not reproduce, minimize, or promote this ID-only carrier. Generate fresh admitted v2 KVM evidence first.

### `rust-workload` — blocked assertion identity

- Historical bug: `dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/bug_0.json`
- Historical replay verdict: `dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/replay-verdict-bug0.json`
- Historical snapshot: `dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/snapshots/e8e870d6577678e4de12d874716b8c7f9a87b8a9dbdb6ae1dbcac935e03718b7.snapshot.bin`
- Status: `blocked-assertion-identity`
- Blocker: bug-report: legacy assertion ID-only evidence cannot promote

Do not reproduce, minimize, or promote this ID-only carrier. Generate fresh admitted v2 KVM evidence first.
