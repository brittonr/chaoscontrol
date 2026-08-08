## Phase 1: Networking probe

- [x] [serial] Add a cmdline-gated networking `snapshot_replay_probe` with stable assertion ID and focused unit/parser coverage.
- [x] [depends:probe] Build/check the networking guest path.

## Phase 2: Accepted evidence

- [x] [depends:probe] Run accepted snapshot verdict dogfood for the net workload and curate concise committed evidence plus the selected snapshot artifact.
- [x] [depends:evidence] Update the accepted workload proof manifest and generated coverage/readiness docs.
- [x] [depends:docs] Run replay proof coverage, readiness, evidence, formatting, and OpenSpec validation gates.

## Phase 3: Closeout

- [x] [depends:validation] Archive the OpenSpec change, commit, push, and verify a clean synced worktree.
