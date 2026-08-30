## Why

ChaosControl uses an Apache-2.0 guest and SDK boundary beside AGPL-3.0-or-later host crates. Shared extraction now needs guest, protocol, catalog, and host code to depend on the same reusable AGPL components. The split blocks that dependency graph and makes each extraction carry a separate license exception.

The repository needs one explicit license boundary for future repository-owned revisions. That boundary must preserve earlier grants and third-party terms.

## What Changes

- Classify all repository-owned crates, Rust workload templates, generated scaffold source, tools, and lifecycle material as AGPL-3.0-or-later for future revisions.
- Replace the Apache workspace default and crate-local Apache metadata with AGPL-3.0-or-later where the project has authority.
- Keep prior Apache-2.0 releases and grants valid. Do not claim retroactive withdrawal.
- Keep third-party code, generated material that contains upstream code, kernels, and external workloads under their governing terms.
- Update package license files, Cargo metadata, dependency policy, README text, and the detailed license map.
- Check source archives and generated templates so their license notices match their content.

## Impact

- **Files**: workspace and crate manifests, crate-local license files, `LICENSES/`, `README.md`, `docs/licensing.md`, dependency policy, templates, and package checks.
- **Compatibility**: new versions of guest and SDK crates no longer provide an Apache-only embedding surface.
- **Distribution**: copied repository-owned template source carries AGPL-3.0-or-later unless a separate grant applies.
- **Prior grants**: previously published Apache-2.0 versions remain available under their original terms.
- **Scope boundary**: this change records the repository license policy. It does not decide the license of third-party dependencies or unrelated workload output.
