# ChaosControl Local Operator Triage Runbook

Generated from a replay-readiness receipt and `dogfood-results/accepted-workload-proofs.json`. Do not scrape `run.log`, `reproduce.log`, or temporary VM logs for the triage decision. Use only the bounded artifacts and status below.

## Receipt entry point

- Summary: `replay-readiness status=passed exit=0 static_gates=2/2 failed_gates=none dogfood=skipped failed_phase=none scope=bounded`
- Selected workload: `all committed proofs`
- Scope: bounded committed replay/evidence readiness; not hosted product parity and not universal determinism.

## Triage steps

1. Open the readiness receipt and dashboard/summary artifacts for status only.
2. Confirm that each selected bug and verdict has exact admitted v2 assertion identity.
3. Re-run reproduce and minimize only with the catalog-bound commands shown below.
4. Record the operator decision. Keep raw logs local unless a concise hash-bound receipt explicitly promotes them.

## Workloads

### `raft`

- Assertion: `3463273124`
- Evidence directory: `dogfood-results/raft-fresh-v2-proof-20260809/`
- Bug: `dogfood-results/raft-fresh-v2-proof-20260809/bug_4.json`
- Replay verdict: `dogfood-results/raft-fresh-v2-proof-20260809/replay-verdict-bug4.json`
- Snapshot artifact or chunk manifest: `dogfood-results/raft-fresh-v2-proof-20260809/snapshots/e91a40b4d06037dc463756248745dc2973560284c31a4e0d7c9561d44a8ea5ae.snapshot.bin`
- Accepted summary: `dogfood-results/raft-fresh-v2-proof-20260809/accepted-snapshot-verdict-summary.json`
- Replay class/depth: `snapshot_backed_reproduced` / `1`

Reproduce from committed artifacts:

```bash
mkdir -p target/operator-triage
/nix/store/i0z222lpwn6a48wwnbwdvia1chw98yg1-chaoscontrol-0.1.0/bin/chaoscontrol-explore reproduce --kernel /nix/store/8ki40sdb3cb8zyg7zsvjlmybgir46z6b-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/qwd26d15sd4p0ibjhvkflkz8c7fy0xap-chaoscontrol-initrd-raft --bug dogfood-results/raft-fresh-v2-proof-20260809/bug_4.json --vms 3 --bootstrap-budget 10000 --memory-mb 256 --extra-cmdline raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=1 --verdict-output target/operator-triage/raft-replay-verdict.json
```

Minimize using the same kernel/initrd/VM options as the reproduce command above:

```bash
cargo run --release --bin chaoscontrol-explore -- minimize --bug dogfood-results/raft-fresh-v2-proof-20260809/bug_4.json --output target/operator-triage/raft-minimized-bug.json
```

Record the operator decision:

```bash
cat > target/operator-triage/raft-decision.json <<'JSON'
{
  "assertion_id": 3463273124,
  "bug": "dogfood-results/raft-fresh-v2-proof-20260809/bug_4.json",
  "decision": "accepted|needs-refresh|blocked",
  "minimized_bug": "target/operator-triage/raft-minimized-bug.json",
  "raw_log_scraping": false,
  "reason": "operator note",
  "replay_verdict": "target/operator-triage/raft-replay-verdict.json",
  "schema_version": 1,
  "workload": "raft"
}
JSON
```

### `redb`

- Assertion: `4149728441`
- Evidence directory: `dogfood-results/redb-fresh-v2-proof-20260809/`
- Bug: `dogfood-results/redb-fresh-v2-proof-20260809/bug_0.json`
- Replay verdict: `dogfood-results/redb-fresh-v2-proof-20260809/replay-verdict-bug0.json`
- Snapshot artifact or chunk manifest: `dogfood-results/redb-fresh-v2-proof-20260809/snapshots/a2eba3c13b009865adf837500752d3de92c5e003e0fd3dbc13dedfcdc77104c4.snapshot.bin`
- Accepted summary: `dogfood-results/redb-fresh-v2-proof-20260809/accepted-snapshot-verdict-summary.json`
- Replay class/depth: `snapshot_backed_reproduced` / `1`

Reproduce from committed artifacts:

```bash
mkdir -p target/operator-triage
/nix/store/jc5m6ckqlxhc263xfsfcx2acdx7zkd4q-chaoscontrol-0.1.0/bin/chaoscontrol-explore reproduce --kernel /nix/store/96hxkvhlf1ifzvkrl5xpkigf3g2jv1m6-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/bx40f0qr3a7fd0zrfl4nflbvp788lyl0-chaoscontrol-initrd-redb --bug dogfood-results/redb-fresh-v2-proof-20260809/bug_0.json --vms 1 --seed 42 --bootstrap-budget 10000 --memory-mb 256 --extra-cmdline redb_bug=snapshot_replay_probe redb_snapshot_probe_fail_after=1 --verdict-output target/operator-triage/redb-replay-verdict.json
```

Minimize using the same kernel/initrd/VM options as the reproduce command above:

```bash
cargo run --release --bin chaoscontrol-explore -- minimize --bug dogfood-results/redb-fresh-v2-proof-20260809/bug_0.json --output target/operator-triage/redb-minimized-bug.json
```

Record the operator decision:

```bash
cat > target/operator-triage/redb-decision.json <<'JSON'
{
  "assertion_id": 4149728441,
  "bug": "dogfood-results/redb-fresh-v2-proof-20260809/bug_0.json",
  "decision": "accepted|needs-refresh|blocked",
  "minimized_bug": "target/operator-triage/redb-minimized-bug.json",
  "raw_log_scraping": false,
  "reason": "operator note",
  "replay_verdict": "target/operator-triage/redb-replay-verdict.json",
  "schema_version": 1,
  "workload": "redb"
}
JSON
```

### `net`

- Assertion: `2074476939`
- Evidence directory: `dogfood-results/net-fresh-v2-proof-20260809/`
- Bug: `dogfood-results/net-fresh-v2-proof-20260809/bug_0.json`
- Replay verdict: `dogfood-results/net-fresh-v2-proof-20260809/replay-verdict-bug0.json`
- Snapshot artifact or chunk manifest: `dogfood-results/net-fresh-v2-proof-20260809/snapshots/17f24c2485277dde53e8d1787e5f5d0cc7f84e1b23631f22c13933821110a662.snapshot.bin`
- Accepted summary: `dogfood-results/net-fresh-v2-proof-20260809/accepted-snapshot-verdict-summary.json`
- Replay class/depth: `snapshot_backed_reproduced` / `1`

Reproduce from committed artifacts:

```bash
mkdir -p target/operator-triage
/nix/store/jc5m6ckqlxhc263xfsfcx2acdx7zkd4q-chaoscontrol-0.1.0/bin/chaoscontrol-explore reproduce --kernel /nix/store/8ki40sdb3cb8zyg7zsvjlmybgir46z6b-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/c386gpcvs7z3fisk1lh9pr2vvr69h2m0-chaoscontrol-initrd-net --bug dogfood-results/net-fresh-v2-proof-20260809/bug_0.json --vms 3 --seed 42 --bootstrap-budget 10000 --memory-mb 256 --extra-cmdline net_bug=snapshot_replay_probe net_snapshot_probe_fail_after=1 --verdict-output target/operator-triage/net-replay-verdict.json
```

Minimize using the same kernel/initrd/VM options as the reproduce command above:

```bash
cargo run --release --bin chaoscontrol-explore -- minimize --bug dogfood-results/net-fresh-v2-proof-20260809/bug_0.json --output target/operator-triage/net-minimized-bug.json
```

Record the operator decision:

```bash
cat > target/operator-triage/net-decision.json <<'JSON'
{
  "assertion_id": 2074476939,
  "bug": "dogfood-results/net-fresh-v2-proof-20260809/bug_0.json",
  "decision": "accepted|needs-refresh|blocked",
  "minimized_bug": "target/operator-triage/net-minimized-bug.json",
  "raw_log_scraping": false,
  "reason": "operator note",
  "replay_verdict": "target/operator-triage/net-replay-verdict.json",
  "schema_version": 1,
  "workload": "net"
}
JSON
```

### `rust-workload`

- Assertion: `3143219316`
- Evidence directory: `dogfood-results/rust-workload-fresh-v2-proof-20260809/`
- Bug: `dogfood-results/rust-workload-fresh-v2-proof-20260809/bug_0.json`
- Replay verdict: `dogfood-results/rust-workload-fresh-v2-proof-20260809/replay-verdict-bug0.json`
- Snapshot artifact or chunk manifest: `dogfood-results/rust-workload-fresh-v2-proof-20260809/snapshots/36400b0af56e8792c2c6729eaabbcd870650dcf5ad0a05aaff6cee5e36e5a9f9.snapshot.bin`
- Accepted summary: `dogfood-results/rust-workload-fresh-v2-proof-20260809/accepted-snapshot-verdict-summary.json`
- Replay class/depth: `snapshot_backed_reproduced` / `1`

Reproduce from committed artifacts:

```bash
mkdir -p target/operator-triage
/nix/store/jc5m6ckqlxhc263xfsfcx2acdx7zkd4q-chaoscontrol-0.1.0/bin/chaoscontrol-explore reproduce --kernel /nix/store/x9qp3ls75w73mxf1mvypsj6p8zmyk9x4-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/zw2n4j57dvs9f78jv14pvc45mz5rh1cq-chaoscontrol-initrd-rust-workload --bug dogfood-results/rust-workload-fresh-v2-proof-20260809/bug_0.json --vms 1 --seed 42 --bootstrap-budget 10000 --memory-mb 128 --extra-cmdline rust_workload_bug=snapshot_replay_probe rust_workload_snapshot_probe_fail_after=1 --verdict-output target/operator-triage/rust-workload-replay-verdict.json
```

Minimize using the same kernel/initrd/VM options as the reproduce command above:

```bash
cargo run --release --bin chaoscontrol-explore -- minimize --bug dogfood-results/rust-workload-fresh-v2-proof-20260809/bug_0.json --output target/operator-triage/rust-workload-minimized-bug.json
```

Record the operator decision:

```bash
cat > target/operator-triage/rust-workload-decision.json <<'JSON'
{
  "assertion_id": 3143219316,
  "bug": "dogfood-results/rust-workload-fresh-v2-proof-20260809/bug_0.json",
  "decision": "accepted|needs-refresh|blocked",
  "minimized_bug": "target/operator-triage/rust-workload-minimized-bug.json",
  "raw_log_scraping": false,
  "reason": "operator note",
  "replay_verdict": "target/operator-triage/rust-workload-replay-verdict.json",
  "schema_version": 1,
  "workload": "rust-workload"
}
JSON
```
