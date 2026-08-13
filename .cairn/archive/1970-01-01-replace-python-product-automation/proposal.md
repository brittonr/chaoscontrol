## Why

Five Python scripts and several inline Python blocks still own dogfood, receipt, summary, audit, scaffold, and KVM orchestration. This duplicates Rust evidence models and creates separate error, bound, and test behavior.

## What Changes

- Inventory every Python-owned product decision and output schema.
- Move structured parsing, validation, classification, and rendering into owned Rust cores.
- Keep process and filesystem work in thin Rust shells.
- Preserve command names, output schemas, exit classes, and Nix app behavior during cutover.
- Remove Python runtime inputs only after positive and negative parity validation.

## Impact

- **Code**: evidence CLIs, KVM smoke runner, audit validator, workload scaffold tool, and Nix wrappers.
- **Removal candidates**: all five scripts under `scripts/*.py` and inline Python in `flake.nix`.
- **Testing**: golden output parity plus malformed, missing, stale, timeout, partial, and write-failure cases.

## Non-Goals

- No ban on Python for external research tools.
- No output schema change during migration.
- No large catch-all automation binary with unclear ownership.
