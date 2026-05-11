# ChaosControl Local Operator Triage Runbook

Generated from a replay-readiness receipt and `dogfood-results/accepted-workload-proofs.json`. Do not scrape `run.log`, `reproduce.log`, or temporary VM logs for the triage decision; use the receipt, bug JSON, replay verdict, snapshot artifact/chunk manifest, and the commands below.

## Receipt entry point

- Summary: `replay-readiness status=passed exit=0 static_gates=2/2 failed_gates=none dogfood=skipped failed_phase=none scope=bounded`
- Selected workload: `all committed proofs`
- Scope: bounded committed replay/evidence readiness; not hosted product parity and not universal determinism.

## Triage steps

1. Open the readiness receipt and dashboard/summary artifacts for status only.
2. Open each listed `bug_*.json` and `replay-verdict*.json` below. Confirm `replay_class = snapshot_backed_reproduced`, `reproduced = true`, `replay_parent_depth > 0`, and `snapshot.status = valid`.
3. Re-run reproduce with the recorded command, writing any fresh verdict under `target/operator-triage/`.
4. Run minimize using the same kernel/initrd/VM options from the recorded reproduce command and the listed bug path; write minimized output under `target/operator-triage/`.
5. Record the operator decision with the JSON template for each workload. Keep raw logs local unless a concise hash-bound receipt explicitly promotes them.

## Workloads

### `raft`

- Assertion: `1806003755`
- Evidence directory: `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/`
- Bug: `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/bug_0.json`
- Replay verdict: `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/replay-verdict-bug0.json`
- Snapshot artifact or chunk manifest: `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/snapshots/cc0161208b3e591ef79625c902e7418aa70a2f33e1445095790e5202265511d2.snapshot.bin`
- Accepted summary: `dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/accepted-snapshot-verdict-summary.json`
- Replay class/depth: `snapshot_backed_reproduced` / `2`

Reproduce from committed artifacts:

```bash
mkdir -p target/operator-triage
/nix/store/afr8jv1dal1zih8x28g4xv9zrc071zps-chaoscontrol-0.1.0/bin/chaoscontrol-explore reproduce --kernel /nix/store/8ki40sdb3cb8zyg7zsvjlmybgir46z6b-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/inn4r7ksfamxds56yd2kgqk2jzs6ir6i-chaoscontrol-initrd-raft --bug dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/bug_0.json --vms 3 --bootstrap-budget 10000 --memory-mb 256 --extra-cmdline raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=25 --verdict-output target/operator-triage/raft-replay-verdict.json
```

Minimize using the same kernel/initrd/VM options as the reproduce command above:

```bash
cargo run --release --bin chaoscontrol-explore -- minimize --bug dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/bug_0.json --output target/operator-triage/raft-minimized-bug.json
```

Record the operator decision:

```bash
cat > target/operator-triage/raft-decision.json <<'JSON'
{
  "assertion_id": 1806003755,
  "bug": "dogfood-results/raft-accepted-verdict-dogfood-20260509T030143Z/bug_0.json",
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

- Assertion: `2718281828`
- Evidence directory: `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/`
- Bug: `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/bug_0.json`
- Replay verdict: `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/replay-verdict-bug0.json`
- Snapshot artifact or chunk manifest: `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/snapshots/bacc336ca613083d1276472e79fe6845220205c30582dbac93cd9537629134ac.snapshot.bin`
- Accepted summary: `dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/accepted-snapshot-verdict-summary.json`
- Replay class/depth: `snapshot_backed_reproduced` / `1`

Reproduce from committed artifacts:

```bash
mkdir -p target/operator-triage
/nix/store/15ja72bg2flpzxj3hj736lzbrph8qnsg-chaoscontrol-0.1.0/bin/chaoscontrol-explore reproduce --kernel /nix/store/96hxkvhlf1ifzvkrl5xpkigf3g2jv1m6-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/yrdzgqbyp7gg88s28mkm8ybs5m2gij9g-chaoscontrol-initrd-redb --bug dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/bug_0.json --vms 1 --bootstrap-budget 10000 --memory-mb 256 --extra-cmdline redb_bug=snapshot_replay_probe redb_snapshot_probe_fail_after=25 --verdict-output target/operator-triage/redb-replay-verdict.json
```

Minimize using the same kernel/initrd/VM options as the reproduce command above:

```bash
cargo run --release --bin chaoscontrol-explore -- minimize --bug dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/bug_0.json --output target/operator-triage/redb-minimized-bug.json
```

Record the operator decision:

```bash
cat > target/operator-triage/redb-decision.json <<'JSON'
{
  "assertion_id": 2718281828,
  "bug": "dogfood-results/redb-accepted-verdict-dogfood-20260510T191449Z/bug_0.json",
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

- Assertion: `3141592653`
- Evidence directory: `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/`
- Bug: `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/bug_0.json`
- Replay verdict: `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/replay-verdict-bug0.json`
- Snapshot artifact or chunk manifest: `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/snapshots/0a1d6142c36dd4fc0f875deffdef316e7965c5b310b0b96ec0434789fe047dd5.snapshot.bin`
- Accepted summary: `dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/accepted-snapshot-verdict-summary.json`
- Replay class/depth: `snapshot_backed_reproduced` / `1`

Reproduce from committed artifacts:

```bash
mkdir -p target/operator-triage
/nix/store/0cwd8y7nfnw24pjnk806i8x7ycmc4k05-chaoscontrol-0.1.0/bin/chaoscontrol-explore reproduce --kernel /nix/store/8ki40sdb3cb8zyg7zsvjlmybgir46z6b-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/45l3cyam35gby5wm07kdx788mpzhbaf6-chaoscontrol-initrd-net --bug /home/brittonr/git/chaoscontrol/dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/bug_0.json --vms 3 --bootstrap-budget 10000 --memory-mb 256 --extra-cmdline net_bug=snapshot_replay_probe net_snapshot_probe_fail_after=3 --verdict-output target/operator-triage/net-replay-verdict.json
```

Minimize using the same kernel/initrd/VM options as the reproduce command above:

```bash
cargo run --release --bin chaoscontrol-explore -- minimize --bug dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/bug_0.json --output target/operator-triage/net-minimized-bug.json
```

Record the operator decision:

```bash
cat > target/operator-triage/net-decision.json <<'JSON'
{
  "assertion_id": 3141592653,
  "bug": "dogfood-results/net-accepted-verdict-dogfood-20260509T015147Z/bug_0.json",
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

- Assertion: `1414213562`
- Evidence directory: `dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/`
- Bug: `dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/bug_0.json`
- Replay verdict: `dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/replay-verdict-bug0.json`
- Snapshot artifact or chunk manifest: `dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/snapshots/e8e870d6577678e4de12d874716b8c7f9a87b8a9dbdb6ae1dbcac935e03718b7.snapshot.bin`
- Accepted summary: `dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/accepted-snapshot-verdict-summary.json`
- Replay class/depth: `snapshot_backed_reproduced` / `2`

Reproduce from committed artifacts:

```bash
mkdir -p target/operator-triage
/nix/store/adk06696jdqmvpy9q834f2zsasq9bxk3-chaoscontrol-0.1.0/bin/chaoscontrol-explore reproduce --kernel /nix/store/x9qp3ls75w73mxf1mvypsj6p8zmyk9x4-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/vd7lbmdcglbr11izp2wk19bhr1h6gnkx-chaoscontrol-initrd-rust-workload --bug dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/bug_0.json --vms 1 --bootstrap-budget 10000 --memory-mb 128 --extra-cmdline rust_workload_bug=snapshot_replay_probe rust_workload_snapshot_probe_fail_after=25 --verdict-output target/operator-triage/rust-workload-replay-verdict.json
```

Minimize using the same kernel/initrd/VM options as the reproduce command above:

```bash
cargo run --release --bin chaoscontrol-explore -- minimize --bug dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/bug_0.json --output target/operator-triage/rust-workload-minimized-bug.json
```

Record the operator decision:

```bash
cat > target/operator-triage/rust-workload-decision.json <<'JSON'
{
  "assertion_id": 1414213562,
  "bug": "dogfood-results/rust-workload-accepted-verdict-dogfood-20260511T163054Z/bug_0.json",
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
