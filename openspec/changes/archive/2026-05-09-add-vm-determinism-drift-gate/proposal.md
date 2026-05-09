## Why

ChaosControl now has accepted bounded replay evidence for named workloads, but the operator-facing readiness status still correctly labels arbitrary guest/device determinism as unproven. The existing `determinism_stress` binary exercises useful VM configurations, yet its proof surface is console-only and easy to drift from the replay/operator evidence model.

## What Changes

- Add a VM determinism drift gate contract around repeated same-seed VM/controller runs.
- Emit a machine-readable receipt that captures inputs, artifact digests, per-case fingerprints, mismatch classes, and optional dlog structural comparison status.
- Keep the gate bounded: it detects drift for selected configurations and does not promote universal determinism or full Antithesis replacement claims.

## Capabilities

### Modified Capabilities
- `determinism-log`: dlog can be used as optional structural evidence for repeated-run drift detection.
- `replay-readiness-operator`: operator readiness can cite a bounded VM drift gate without broadening unsupported surfaces.

## Impact

- **Files**: `crates/chaoscontrol-vmm/src/bin/determinism_stress.rs`, `crates/chaoscontrol-vmm/src/determinism_gate.rs`, `crates/chaoscontrol-vmm/Cargo.toml`, `crates/chaoscontrol-vmm/src/lib.rs`, OpenSpec deltas.
- **APIs**: Adds a Rust-owned `determinism_gate` helper module and extends the existing stress binary with `--receipt` and `--dlog-dir` options.
- **Dependencies**: Adds `serde_json` to `chaoscontrol-vmm` for receipt emission.
- **Testing**: Unit-test the pure comparison/receipt layer, run the focused VMM package tests/checks, validate OpenSpec, and keep costly KVM drift runs operator-invoked.
