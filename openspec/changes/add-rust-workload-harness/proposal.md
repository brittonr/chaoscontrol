## Why

ChaosControl's Rust SDK already exposes the right Antithesis-style primitives for the user's own Rust projects: assertions, lifecycle events, guided randomness, local output, and VM transport. The remaining friction is repeating project glue every time a new Rust service is brought under deterministic chaos: init placement, setup-complete conventions, assertion density review, guest binary packaging, flake wiring, local dry-runs, and replay evidence collection.

This change defines a Rust-only workload harness layer so a downstream Rust project can be instrumented once and then run through ChaosControl without rediscovering repo-local VMM details.

## What Changes

- Add a Rust workload harness/template surface around `chaoscontrol-sdk` for downstream Rust projects.
- Define a local dry-run mode that produces assertion/lifecycle/randomness diagnostics before VM execution.
- Define Nix/CLI wiring that packages a downstream Rust guest and runs a bounded ChaosControl campaign from one command.
- Define a report surface that connects SDK instrumentation to assertion density, reached/unreached assertions, sometimes progress, replay verdicts, and evidence paths.

## Non-Goals

- No multi-language SDK parity.
- No hosted service, UI, or broad Antithesis product replacement claim.
- No requirement that downstream users understand internal VMM snapshot/replay mechanics.
- No automatic inference of application correctness properties; projects still author their own assertions and scenarios.

## Verification

Implementation will be accepted only when a repo-local sample Rust workload can be generated or wired from the harness, run locally in dry-run mode, packaged as a guest, executed through a bounded campaign, and produce a report that links SDK instrumentation to replay/evidence artifacts.