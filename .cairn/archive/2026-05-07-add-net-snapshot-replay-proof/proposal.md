## Why

ChaosControl's accepted replay proof coverage currently spans Raft and redb. A third independent workload should exercise the virtio-net guest path so breadth claims are less tied to consensus or disk/database behavior.

## What Changes

- Add a bounded `snapshot_replay_probe` mode to the networking guest.
- Run the accepted snapshot verdict dogfood wrapper against `initrd-net` and retain concise proof evidence plus the referenced snapshot artifact.
- Update the accepted workload proof manifest and generated replay coverage/readiness docs.

## Impact

- **Workload:** `chaoscontrol-net-guest` gains a cmdline-gated probe only active when requested.
- **Evidence:** `dogfood-results/accepted-workload-proofs.json` gains a third workload entry.
- **Docs:** Replay proof coverage/readiness reports list the network workload as supported-bounded once evidence validates.
- **Testing:** Rust build/checks, proof coverage checks, readiness generation checks, evidence contract checks, and KVM dogfood reproduce verdict.
