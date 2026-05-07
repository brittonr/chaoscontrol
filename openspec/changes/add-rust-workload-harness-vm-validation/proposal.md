## Why

The parent `add-rust-workload-harness` change implemented and verified the Rust harness, local dry-run report, and Nix packaging rail. The remaining proof is a bounded VM campaign through `.#explore-rust-workload`, but the first drain run exceeded the local interactive budget while waiting inside `nix run` and produced no campaign output.

This change isolates the long-running VM validation so the implemented harness package can be archived without promoting an unproven VM campaign.

## What Changes

- Capture a successful bounded VM campaign for the Rust workload harness rail.
- Preserve the evidence classification boundary between local instrumentation, bounded VM campaign output, and snapshot-backed replay proof.
- Record command/output paths and any blocker transcript if the VM campaign still cannot complete.

## Non-Goals

- No SDK API redesign.
- No hosted-product or multi-language expansion.
- No claim that bounded VM campaign output is accepted replay proof without replay/minimization artifacts.

## Verification

Accepted when `nix run .#explore-rust-workload -- <out>` completes or a stronger equivalent VM campaign command completes, and the output directory contains an inspectable classification receipt plus campaign artifacts/logs.
