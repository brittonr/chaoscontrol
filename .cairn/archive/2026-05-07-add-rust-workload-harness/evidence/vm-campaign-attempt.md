# Rust Workload VM Campaign Attempt

Status: captured

## Commands / Oracles

- command: `rm -rf /tmp/cc-rust-workload-vm-drain; nix run .#explore-rust-workload -- /tmp/cc-rust-workload-vm-drain`
- oracle: Complete the one-command bounded VM campaign and write `/tmp/cc-rust-workload-vm-drain/evidence-classification.json` plus campaign output.

## Outcomes

- result: fail command was still running with no stdout after more than 30 minutes and was terminated by Hermes process control.
- result: fail `ps` showed the `nix run` client blocked in `unix_stream_read_generic`; `/tmp/cc-rust-workload-vm-drain` had not been materialized for review.
- result: pass source-level rail evidence remains in prior checks: `.#guest-rust-workload`, `.#initrd-rust-workload`, and `.#rust-workload-local-report` build/run, but this is not VM campaign proof.

## Classification

This is blocked runtime-validation evidence, not replay proof and not a completed VM campaign. The parent implementation/spec package can be closed only by deferring the bounded VM campaign proof to a scoped follow-up OpenSpec.

Captured: 2026-05-07T15:37:24Z
