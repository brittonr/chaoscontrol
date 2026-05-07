## Context

The accepted replay proof rail now uses targeted checkpoint bug export and Rust-owned replay verdict JSON. Raft and redb already have committed `snapshot_backed_reproduced` verdicts; networking is the next highest-ROI independent workload because it exercises virtio-net and multi-VM TCP communication.

## Goals / Non-Goals

**Goals:**
- Add a cmdline-gated networking snapshot replay probe with a stable assertion ID.
- Reuse the existing accepted snapshot verdict dogfood wrapper.
- Commit only concise evidence and the selected snapshot artifact, not raw logs or checkpoints.

**Non-Goals:**
- Do not claim mathematical or universal hypervisor determinism.
- Do not make every networking assertion a replay proof.
- Do not commit raw run/reproduce logs.

## Decisions

### 1. Probe on server-side progress

**Choice:** The networking probe asserts that the server's `pong_count` stays below a cmdline threshold only when `net_bug=snapshot_replay_probe` is set.

**Rationale:** The server observes successful inter-VM TCP round trips, making the proof independent from Raft and redb while remaining deterministic and bounded.

**Alternative:** Client-side pongs were rejected because multiple clients could emit competing assertion streams with the same probe ID.

### 2. Reuse accepted verdict wrapper

**Choice:** The existing `scripts/accepted-snapshot-verdict-dogfood.py` remains the evidence runner, with workload/cmdline/assertion parameters for net.

**Rationale:** Reusing the wrapper keeps export filters, snapshot digest validation, verdict acceptance, and raw-log policy identical across workloads.
