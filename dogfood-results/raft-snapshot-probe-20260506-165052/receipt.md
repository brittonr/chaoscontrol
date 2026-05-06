# Snapshot replay probe dogfood receipt

- Run id: `raft-snapshot-probe-20260506-165052`
- Selected bug: `bug_2.json`
- Assertion: `1806003755` — snapshot replay probe trips only after restored parent context
- Replay parent depth: `2`
- Snapshot ref: `snapshots/f59cb5761405698981eec1e68f4053a9767b21caa45d9bf49ca36879971001a0.snapshot.bin` (`simulation-snapshot-bincode-zstd-v1`)
- Reproduce: BUG REPRODUCED (exit 0)
- Minimize: 3 faults -> 1 fault(s), snapshot ref preserved
- Raw logs: kept under `/tmp`, excluded from git
