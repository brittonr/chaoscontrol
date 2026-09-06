# Gate repair checkpoint

## Outcome

Cargo/Radicle compatibility now passes dependency policy and artifact installation.
The source repairs remove 1,003 findings from the same pinned Octet scope.
The current pass removes 307 findings after `61097cd`.
Octet still reports 1,455 warnings. This is not strict acceptance.
The no-default test matrix now passes 15 protocol and 42 SDK cases, plus strict Clippy and three rejecting target controls.
The change remains active with 20 of 22 tasks complete.
No accepted-spec sync, archive, or main integration occurs.

The VM Cohort vendor repair now passes compiler parity, rejection controls, nine adapter cases, and dependency policy.
The current broad retry rejects the SpaceWasm bundle manifest before runtime comparison.
Its observed digest matches the retained rooted manifest, not the admitted digest.
`vendor.md` records the packaging repair. `spacewasm.md` records the remaining identity mismatch.
No provider pin, expected digest, or admission guard changed.

## Repair commits

| Commit | Scope |
| --- | --- |
| `c636c2c` | Explicit Serde derives |
| `c822499` | Cargo parser and formatter, dependency policy |
| `d9a8dae` | Owner paths and compatible protocol names |
| `171edce` | Standard-library owners across inline modules |
| `a78b3a2` | Compiler-reported cross-crate owner paths and Rustdoc link repair |
| `a3100ee` | Generated adapter lockfile and host/musl install metadata |
| `9890746` | Exact VM Cohort workspace resources and source-filter controls |
| `cfcf35b` | Serde derives across admitted inline scopes |
| `8dbcbb1` | Exact integer lengths and framing controls |
| `7ed930b` | Reused owner qualifier across 12 newly admitted files |
| `4959d75` | Binding-by-binding qualification across 26 files |
| `4343e37` | Feature-aware minimal tests and explicit full-only targets |

## Current source-pass evidence

- `source-rounds.md` records the three-batch budget, review boundaries, and checked partial result.
- `source-checked-commands.md` records command scope, feature modes, the replay deadline, and the explicit lifecycle policy.
- `owner-round.md`, `binding-round.md`, and `minimal.md` record each batch and its negative controls.
- `source-checked-tests.log`, `source-checked-clippy.log`, `source-checked-rustdoc.log`, and `source-checked-rustfmt.log` pass for the seven-package scope.
- `source-checked-nix.log` passes protocol tests/contracts, vendor controls/adapter, dependency policy, source filtering, and the license boundary.
- That Nix command also passes the strict isolated adapter check with zero findings. The broader focused report still contains 1,455 warnings.
- `source-checked-replay.log` records four bounded KVM passes in 68.28 seconds.
- `minimal-*-final.log` and `minimal-reject-*.log` record both feature-compatible tests and incompatible-target rejection.
- `source-checked-flake.log` retains the SpaceWasm manifest rejection. The failed full check is not release-wide acceptance evidence.
- `source-checked-cairn-*.json` retain canonical-policy validation and the three gates. Their PASS verdicts do not close the two remaining tasks.
- `source-checked-product-scope.log` and `source-checked-registry.log` retain the read-only product checks.

## Earlier evidence

These entries describe their named checkpoints, not freshness for the current source.

- `cargo.md` records the Cargo reproduction, 14 schema passes, metadata checks, policy checks, and rejecting near-match controls.
- `names.md`, `modules.md`, and `reported.md` record the source rounds and their failed controls.
- `build.md` records four exact crate mirrors, the isolated adapter lockfile, the Crane hook, and the earlier vendor-layout error.
- `vendor.md`, `serde-scopes.md`, and `framing.md` record the continued repairs, their baselines, and their negative controls.
- `framing-tests.log` and `framing-clippy.log` pass across all targets and all features for the seven selected packages.
- `framing-nix.log` passes the focused protocol, contract, vendor, dependency, and source-filter checks. Octet still reports 1,762 warnings.
- `continuation-replay.log` passes the four bounded KVM replay cases at `61097cd`.
- `continuation-no-default.log` passes the library/binary feature check. `continuation-rustdoc.log` passes with warnings denied.
- `reported-owner-tests.log` and `reported-owner-clippy.log` pass across all targets and all features for the seven selected packages.
- `rustdoc-corrected.log` passes with warnings denied.
- `replay-final.log` records four explicit, bounded KVM replay passes.
- `no-default-build.log` passes in the original library/binary scope. The additional no-default test-target probe fails and remains in `no-default-final.log`.
- `nix-final-focused.log` passes protocol tests, protocol contracts, dependency policy, and the VM Cohort dependency check.
- `nix-confirmed.log` repeats the final protocol, contract, and dependency checks and passes the 59-rule license boundary. Octet still reports 1,814 warnings.
- `adapter-lock-octet.log` passes the unchanged strict isolated adapter gate with zero findings and no lockfile mutation.
- `cargo-install-hook-verified.log` builds and installs the Raft guest with the normal Cargo 1.98.0 build command.
- `cargo-install-hook-drift-final.log` rejects a missing metadata command with the expected substitution error.
- `cairn-final-*.json` report no issues and PASS gate verdicts with the canonical workspace policy.
- `check-product-scope-final.log` and `contract-registry-corrected.log` pass their read-only checks.

## Input identity

`product-inputs.b3` selects changed product and build files relative to `c9e9376`.
`build-inputs.b3` records the earlier root and isolated build inputs.
`helper-inputs.b3` binds the retained helper artifacts. The corresponding freshness logs pass.
The root `Cargo.lock`, `flake.lock`, and `dylint.toml` remain unchanged.
Cargo generated the only isolated lockfile changes: two existing `serde_json` dependency edges.

The continuation manifests bind the later source and build inputs separately from the earlier checkpoint.
The earlier manifests and raw attempts remain historical evidence, not freshness claims for the current source.

`source-checked-product-inputs.b3` selects product changes relative to the original change-branch base.
`source-checked-build-inputs.b3` and `source-checked-helper-inputs.b3` bind the build and helper inputs for this pass.
Their freshness logs validate these measured inputs, not every external artifact or the complete build closure.

The helpers are not a Rust name resolver, independent review, or a substitute for compiler and behavior checks.
The evidence does not establish protocol-semantic authority, universal replay, production readiness, or release eligibility.
