# Verification

Date: 2026-08-30

## Implementation evidence

- `chaoscontrol-fault` now plans clock freeze, clock jitter, CPU stall, and finite memory pressure.
- `chaoscontrol-vmm` applies each plan and restores bounded effects at exact virtual deadlines.
- `chaoscontrol-sdk::resources::memory_ceiling_bytes` exposes the admitted ceiling to a guest.
- Active memory-pressure snapshots bind the limit, baseline, deadline, and observed applied plan.
- The stage ledger records `Selected`, `Applicable`, `Applied`, and `Observed` for accepted effects.
- Zero windows, invalid memory ceilings, target drift, deadline drift, and unsupported profile facts fail closed.

## Commands

The pre-change baseline passed:

```text
nix develop -c cargo test -p chaoscontrol-fault -p chaoscontrol-vmm
```

The post-change validation passed:

```text
nix develop -c cargo test --workspace
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo fmt --all
```

The workspace test run included positive and negative planner, VMM, SDK, scheduler, snapshot, and replay tests.

`nix flake check -L --option builders ''` evaluated all outputs and started 92 checks. It stopped in the existing dependency-policy rail. The crates.io API returned HTTP 403 for `wasmparser`, `wasmprinter`, and `wasm-encoder`. The offline `cargo deny` process then panicked while reading Cargo metadata. This failure did not occur in the changed Rust surfaces.

Cairn proposal, design, and task gates passed. Strict repository validation also passed with the canonical workspace policy.

## Claim boundary

This evidence proves deterministic behavior for the selected virtual effects and limits. It does not prove guest-kernel OOM behavior, host timing, workload impact, or production readiness.
