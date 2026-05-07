## Why

The Rust workload rail now has a successful bounded VM campaign, but its evidence classification explicitly stops at `bounded-vm-campaign`. To move closer to an Antithesis alternative for Rust projects, the same downstream-shaped harness rail needs accepted snapshot-backed replay proof rather than only campaign output.

## What Changes

- Add a guarded Rust workload snapshot replay probe that can emit a parent-context bug only when explicitly enabled by kernel cmdline.
- Add or document a one-command dogfood rail that runs the Rust workload through filtered export and `reproduce --verdict-output` until an accepted `snapshot_backed_reproduced` verdict exists.
- Curate concise evidence and update replay proof coverage/readiness manifests without committing raw logs or huge checkpoints.

## Non-Goals

- No multi-language SDK or container/Docker onboarding.
- No hosted UI/product expansion.
- No claim that ordinary bounded campaign output is replay proof.

## Verification

Accepted when a Rust workload dogfood run produces an exported bug with `replay_parent_depth > 0`, a valid snapshot ref/artifact, and a replay verdict whose `replay_class` is `snapshot_backed_reproduced`; coverage/readiness checks must include the Rust workload proof.
