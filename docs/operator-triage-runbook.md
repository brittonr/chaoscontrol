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
- Evidence directory: `dogfood-results/raft-fresh-v4-proof-20261108/`
- Bug: `dogfood-results/raft-fresh-v4-proof-20261108/bug_0.json`
- Replay verdict: `dogfood-results/raft-fresh-v4-proof-20261108/replay-verdict-bug0.json`
- Snapshot artifact or chunk manifest: `dogfood-results/raft-fresh-v4-proof-20261108/snapshots/7abbb53fbb34203efac8e95fe20cd54dad0c38e02e6d0d212212a26c77912b8a.snapshot.bin`
- Accepted summary: `dogfood-results/raft-fresh-v4-proof-20261108/accepted-snapshot-verdict-summary.json`
- Replay class/depth: `snapshot_backed_reproduced` / `1`

Reproduce from committed artifacts:

```bash
mkdir -p target/operator-triage
/home/brittonr/.cargo-target/release/chaoscontrol-explore reproduce --kernel /nix/store/d1gy1wcxcdh198bnnx2g6bvnwygggibs-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/0nnczx0h202livhw6ngml1pf51phsk8l-chaoscontrol-initrd-raft --bug dogfood-results/raft-fresh-v4-proof-20261108/bug_0.json --vms 3 --seed 42 --bootstrap-budget 10000 --memory-mb 256 --extra-cmdline raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=1 clocksource=jiffies notsc --verdict-output target/operator-triage/raft-replay-verdict.json
```

Minimize using the same kernel/initrd/VM options as the reproduce command above:

```bash
cargo run --release --bin chaoscontrol-explore -- minimize --bug dogfood-results/raft-fresh-v4-proof-20261108/bug_0.json --output target/operator-triage/raft-minimized-bug.json
```

Record the operator decision:

```bash
cat > target/operator-triage/raft-decision.json <<'JSON'
{
  "assertion_id": 3463273124,
  "bug": "dogfood-results/raft-fresh-v4-proof-20261108/bug_0.json",
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
- Evidence directory: `dogfood-results/redb-fresh-v4-proof-20261108/`
- Bug: `dogfood-results/redb-fresh-v4-proof-20261108/bug_0.json`
- Replay verdict: `dogfood-results/redb-fresh-v4-proof-20261108/replay-verdict-bug0.json`
- Snapshot artifact or chunk manifest: `dogfood-results/redb-fresh-v4-proof-20261108/snapshots/4c6b542e913f157d19d15c1011efb6c3209c9193c98c335c3cca0fef45f87986.snapshot.bin`
- Accepted summary: `dogfood-results/redb-fresh-v4-proof-20261108/accepted-snapshot-verdict-summary.json`
- Replay class/depth: `snapshot_backed_reproduced` / `1`

Reproduce from committed artifacts:

```bash
mkdir -p target/operator-triage
/home/brittonr/.cargo-target/release/chaoscontrol-explore reproduce --kernel /nix/store/alklhcbzi7rppnnscr7xcsb1dgf9zr3r-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/l4czccnh3jz3n2n2mfb1ak2hycv1qz9l-chaoscontrol-initrd-redb --bug dogfood-results/redb-fresh-v4-proof-20261108/bug_0.json --vms 1 --seed 42 --bootstrap-budget 10000 --memory-mb 256 --extra-cmdline redb_bug=snapshot_replay_probe redb_snapshot_probe_fail_after=1 --verdict-output target/operator-triage/redb-replay-verdict.json
```

Minimize using the same kernel/initrd/VM options as the reproduce command above:

```bash
cargo run --release --bin chaoscontrol-explore -- minimize --bug dogfood-results/redb-fresh-v4-proof-20261108/bug_0.json --output target/operator-triage/redb-minimized-bug.json
```

Record the operator decision:

```bash
cat > target/operator-triage/redb-decision.json <<'JSON'
{
  "assertion_id": 4149728441,
  "bug": "dogfood-results/redb-fresh-v4-proof-20261108/bug_0.json",
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
- Evidence directory: `dogfood-results/net-fresh-v4-proof-20261108/`
- Bug: `dogfood-results/net-fresh-v4-proof-20261108/bug_0.json`
- Replay verdict: `dogfood-results/net-fresh-v4-proof-20261108/replay-verdict-bug0.json`
- Snapshot artifact or chunk manifest: `dogfood-results/net-fresh-v4-proof-20261108/snapshots/699d353ad24b8063d6b55c49ddefc222eb6c46ffcea68b5d69c78a796bb21dc7.snapshot.bin`
- Accepted summary: `dogfood-results/net-fresh-v4-proof-20261108/accepted-snapshot-verdict-summary.json`
- Replay class/depth: `snapshot_backed_reproduced` / `1`

Reproduce from committed artifacts:

```bash
mkdir -p target/operator-triage
/home/brittonr/.cargo-target/release/chaoscontrol-explore reproduce --kernel /nix/store/d1gy1wcxcdh198bnnx2g6bvnwygggibs-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/ln9l7xbfy9niifnwvfv8whsx70xfylgy-chaoscontrol-initrd-net --bug dogfood-results/net-fresh-v4-proof-20261108/bug_0.json --vms 3 --seed 42 --bootstrap-budget 10000 --memory-mb 256 --extra-cmdline net_bug=snapshot_replay_probe net_snapshot_probe_fail_after=1 --verdict-output target/operator-triage/net-replay-verdict.json
```

Minimize using the same kernel/initrd/VM options as the reproduce command above:

```bash
cargo run --release --bin chaoscontrol-explore -- minimize --bug dogfood-results/net-fresh-v4-proof-20261108/bug_0.json --output target/operator-triage/net-minimized-bug.json
```

Record the operator decision:

```bash
cat > target/operator-triage/net-decision.json <<'JSON'
{
  "assertion_id": 2074476939,
  "bug": "dogfood-results/net-fresh-v4-proof-20261108/bug_0.json",
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
- Evidence directory: `dogfood-results/rust-workload-fresh-v4-proof-20261108/`
- Bug: `dogfood-results/rust-workload-fresh-v4-proof-20261108/bug_0.json`
- Replay verdict: `dogfood-results/rust-workload-fresh-v4-proof-20261108/replay-verdict-bug0.json`
- Snapshot artifact or chunk manifest: `dogfood-results/rust-workload-fresh-v4-proof-20261108/snapshots/cf14a43ec9b533c3c752245579dc96cc28f07fc08afb58ba4acd03966e1c1cf3.snapshot.bin`
- Accepted summary: `dogfood-results/rust-workload-fresh-v4-proof-20261108/accepted-snapshot-verdict-summary.json`
- Replay class/depth: `snapshot_backed_reproduced` / `1`

Reproduce from committed artifacts:

```bash
mkdir -p target/operator-triage
/home/brittonr/.cargo-target/release/chaoscontrol-explore reproduce --kernel /nix/store/5aya7zvynanjw5mh3xvz40sga3zfsmn8-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/wdx6f941qb2cdsf1c1p50sygqy32pa3w-chaoscontrol-initrd-rust-workload --bug dogfood-results/rust-workload-fresh-v4-proof-20261108/bug_0.json --vms 1 --seed 42 --bootstrap-budget 10000 --memory-mb 128 --extra-cmdline rust_workload_bug=snapshot_replay_probe rust_workload_snapshot_probe_fail_after=1 clocksource=jiffies notsc --verdict-output target/operator-triage/rust-workload-replay-verdict.json
```

Minimize using the same kernel/initrd/VM options as the reproduce command above:

```bash
cargo run --release --bin chaoscontrol-explore -- minimize --bug dogfood-results/rust-workload-fresh-v4-proof-20261108/bug_0.json --output target/operator-triage/rust-workload-minimized-bug.json
```

Record the operator decision:

```bash
cat > target/operator-triage/rust-workload-decision.json <<'JSON'
{
  "assertion_id": 3143219316,
  "bug": "dogfood-results/rust-workload-fresh-v4-proof-20261108/bug_0.json",
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
