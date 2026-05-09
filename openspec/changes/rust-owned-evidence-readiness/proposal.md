## Why

ChaosControl's accepted workload evidence is now current and the full flake check is green, but the evidence/readiness control plane is still split across Python scripts and Bash wrappers. That split makes generated-doc drift easier to miss, duplicates artifact parsing logic, and leaves operator-facing proof checks outside the Rust-owned evidence model used by the rest of the project.

## What Changes

- Introduce a Rust-owned evidence/readiness tooling boundary for committed proof manifests, replay coverage checks, readiness reports, assertion readiness reports, snapshot chunk materialization, and artifact-size validation.
- Migrate public/local commands and Nix checks to Rust CLIs while preserving the current output contracts and fail-closed behavior.
- Generate or check `docs/replay-proof-coverage.md` from the accepted workload manifest so it cannot drift behind `docs/replay-readiness-status.md`.
- Keep process orchestration for slow dogfood/VM runs explicit and bounded; this change does not require rewriting every operational wrapper in one step.

## Capabilities

### New Capabilities
- `rust-owned-evidence-readiness`: Rust library and CLI surface for evidence/readiness validation and report generation.

### Modified Capabilities
- `replay-readiness-operator`: Nix/local readiness gates call Rust-owned validators rather than Python/Bash implementations as each slice migrates.
- `replay-parent-snapshots`: snapshot chunk verification/materialization logic has a Rust source of truth.

## Impact

- **Files:** `crates/**`, `Cargo.toml`, `flake.nix`, `README.md`, `docs/*.md`, and replacement/removal of migrated `scripts/*.py`/`*.sh` entrypoints.
- **APIs:** Adds internal Rust CLI subcommands; existing operator-facing behavior remains compatible unless explicitly renamed with transitional aliases.
- **Dependencies:** Prefer existing Rust dependencies already in the workspace; new dependencies require normal cargo-audit/cargo-deny coverage.
- **Testing:** Unit tests for pure validation/report rendering logic, negative fixtures for malformed evidence, focused Nix checks for migrated gates, and final `nix flake check -L`.
