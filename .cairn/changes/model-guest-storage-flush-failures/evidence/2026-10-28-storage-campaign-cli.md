# Storage-recovery campaign CLI — 2026-10-28

## Added

- `crates/chaoscontrol-evidence/src/rust_automation/storage_campaign.rs` parses a
  bounded `chaoscontrol.storage-recovery-campaign-request.v1`, builds a
  `PersistentNodeCohort`, seeds declared initial states, runs the pure campaign,
  and projects a `chaoscontrol.storage-recovery-campaign-receipt.v1` binding the
  request BLAKE3, cohort receipt ref, verdict, schedule observations, safety,
  progress, and fixed non-claims.
- `crates/chaoscontrol-evidence/src/bin/storage-recovery-campaign.rs` is the
  file-facing shell: no-follow bounded input, existing-path refusal for output,
  `0` pass, `1` non-pass with a written receipt, `2` usage or schema denial.

## Executed

```sh
cargo run -q -p chaoscontrol-evidence --bin storage-recovery-campaign \
  -- --input /dev/shm/cc-campaign-pass.json --out /dev/shm/cc-campaign-pass-receipt.json
```

- Pass request: exit 0; receipt
  `blake3:e09d21dabad3e7822a9f3467b6a7e005514821f109d0864e80c1d92d8fdea620`,
  input `blake3:368d6b48…`, verdict `pass`, observation
  `restarted(node=0)`, progress `local-repair`, four non-claims.
- Unnecessary-remote-repair request: exit 1 with a receipt (non-pass is visible,
  not suppressed).
- Stale request schema: exit 2, `schema is not current`, no output file
  created.

Unit cases: request/receipt round trip, non-pass reporting, stale schema,
seed-count mismatch, and over-bound input (`cargo test -p
chaoscontrol-evidence --lib storage_campaign` → 3 passed; strict Clippy clean).

## Scope

The CLI covers protocol recovery campaigns: restarts, lagging replicas,
unavailable peers, corruption, local repair, and the declared safety/progress
facts. Flush-error, write-error, and cache-retention behaviour remains covered
by the block-model tests in `chaoscontrol-vmm`, not as campaign steps; adding
those as schedule payloads is follow-up work. No result here is a candidate
KVM campaign claim.

The pinned inputs and receipts are retained beside this note so another
repository can bind exact campaign identities.
