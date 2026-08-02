## Why

ChaosControl implements bounded regular-file reads, JSON structure preflight, bounded JSON serialization, and bounded snapshot decompression in several crates. Cairn, OnixOS, and Mantle contain related implementations. The copies differ in path authority, error behavior, and limit coverage.

A shared mechanism can remove duplicate security-sensitive code. Its primary file API must use already-open or capability-relative handles instead of treating an ambient path as authority.

## What Changes

- Establish a product-neutral `bounded-input` repository under AGPL-3.0-or-later.
- Provide a pure policy core for byte, structure, allocation, and decompression decisions.
- Provide capability-handle shells for regular-file reads and relative file admission.
- Provide iterative bounded JSON preflight and bounded serialization utilities.
- Provide compressed-input and expanded-output limits for streaming decompression.
- Migrate the duplicated ChaosControl readers and JSON helpers with positive and negative parity checks.
- Keep path authorization, recursive tree observation, artifact trust, schema meaning, and evidence promotion with their existing owners.

## Impact

- **Source candidates**: `chaoscontrol-evidence/src/bounded_file.rs`, `json_preflight.rs`, SDK `local_json_security.rs` and `bounded_json.rs`, explore `bounded_json.rs`, and snapshot-store decompression.
- **New repository**: `bounded-input` with pure core, JSON, and standard-library adapter crates.
- **Consumers**: ChaosControl first. Cairn, OnixOS, and Mantle remain independent adoption targets.
- **Compatibility**: existing limits and accepted inputs must remain stable unless a stricter difference is explicit and tested.
- **Claims**: the shared mechanism bounds supplied reads and parsing work. It does not grant path authority or prove input semantics.
