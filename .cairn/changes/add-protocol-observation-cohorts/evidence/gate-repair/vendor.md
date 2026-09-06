# Vendor resource repair

## Contract and limits

Continue from `e7575478b4d9e75bf38d41256b59b3b01e06166f`.
Restore the pinned VM Cohort profile in Cargo builds without changing source bytes, dependencies, or gate enforcement.
The prior `flake-final.log` is the unchanged-source baseline: the conformance crate cannot resolve its workspace-relative profile.

This pass uses serial, correlated review. It does not use subagents.
The initial budget covers three layout mechanisms, four focused build rounds, and one broad retry after the focused checks pass.
A new source pass follows the packaging repair. Each source batch needs a baseline, regression checks, and a pinned report.
Validation, a bounded blocker, budget exhaustion, or a genuine user decision can end a pass.
No result permits archive or main integration while required gates remain open.

## Source facts

The published VM Cohort input remains at `ab123e3673b6dd616b3df5d044026b5e85755149`.
Its `config/generated/profile.json` exists in the pinned source.
Only `vm-cohort-conformance/src/standard.rs` includes a workspace-relative resource among the three selected crates.
The profile supplies a standard fixture, not new ChaosControl policy or runtime authority.

Pinned Crane supplies `overrideVendorGitCheckout` and links selected crate directories from the resulting checkout.
Crane first separates packages from the source workspace and resolves Cargo manifest inheritance.
The package source then loses the parent directories that contain the profile.
A local Lattice example restores resources after a full vendor copy. It is a reference, not a product dependency.

## Approach registry

| Family | Mechanism | Claim | Evidence or next check | State |
| --- | --- | --- | --- | --- |
| Shared vendor root | Copy the profile above flattened packages | The relative include resolves | Reject the shared config namespace and whole-vendor copy unless narrower routes fail | blocked |
| Package-local patch | Copy the profile into the crate and patch the include | The include resolves with the same bytes | Requires a source patch and provenance record | independent |
| Private workspace layout | Reparent the package inside its own vendor output and retain a package symlink | Original Rust bytes and relative paths remain intact | Compiler parity, rejection controls, real adapter, and dependency checks pass | validated |

## Required controls

- The original flattened layout must fail at the missing include.
- The projected layout must compile and read the exact pinned profile bytes.
- Projection must reject a missing profile, changed source, and an existing reserved workspace path.
- Source and profile bytes must match before and after projection.
- The real conformance and adapter tests must pass through Cargo vendoring.
- Dependency policy and the exact source guard must remain enabled.
- A successful focused check is not a full-flake or strict Octet result.

## Results

`vendor-baseline.log` reproduces the missing include without a source edit.
`vendor-controls.log` passes the projected compiler fixture and the rejecting controls.
`vendor-adapter.log` passes nine real adapter cases. Three host-only KVM cases remain ignored in the sandbox.
The exact dependency guard and dependency policy pass. The policy reports `bans ok, licenses ok, sources ok`.

`vendor-final.log` repeats the layout, adapter, and dependency checks after the source-filter correction.
The source filter excludes `.cairn/` and `.pi/`. Its positive and negative controls pass without excluding similar directory names.
Both host and musl compositions consume the explicit vendor directory.
No Rust source bytes, profile bytes, Cargo dependency identity, or lockfile changed.

The full-flake retry advances beyond the vendor error, then rejects the SpaceWasm bundle manifest.
The expected digest is `13058ea2d9913348a203cceff7b58d98b6446610ac80518dc3359b8d7ee57472`.
The measured digest is `5dd34420136f7d3f98e62df57ea9bc61adf269e74b07ad8521b00335e77eb4d7`.
`flake-vendor.log` retains that failure. No expected digest or provider pin changes to bypass it.
The source-quality and publication tasks remain open.
