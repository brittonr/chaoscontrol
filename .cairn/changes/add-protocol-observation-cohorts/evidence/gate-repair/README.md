# Gate repair checkpoint

## Outcome

Cargo/Radicle compatibility now passes dependency policy and artifact installation.
The source repairs remove 644 findings from the same pinned Octet scope.
Octet still reports 1,814 warnings. This is not strict acceptance.
The change remains active with 20 of 22 tasks complete.
No accepted-spec sync, archive, or main integration occurs.

The final broad Nix check fails at a separate vendor-layout boundary.
The vendored `vm-cohort-conformance` crate cannot find `../../../config/generated/profile.json` from `src/standard.rs:33`.
The next packaging correction must retain the exact pinned profile and source revision.
`flake-final.log` and `flake-final.exit` retain this failure.

## Repair commits

| Commit | Scope |
| --- | --- |
| `c636c2c` | Explicit Serde derives |
| `c822499` | Cargo parser and formatter, dependency policy |
| `d9a8dae` | Owner paths and compatible protocol names |
| `171edce` | Standard-library owners across inline modules |
| `a78b3a2` | Compiler-reported cross-crate owner paths and Rustdoc link repair |
| `a3100ee` | Generated adapter lockfile and host/musl install metadata |

## Evidence

- `cargo.md` records the Cargo reproduction, 14 schema passes, metadata checks, policy checks, and rejecting near-match controls.
- `names.md`, `modules.md`, and `reported.md` record the source rounds and their failed controls.
- `build.md` records four exact crate mirrors, the isolated adapter lockfile, the Crane hook, and the remaining vendor-layout error.
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
`build-inputs.b3` records the current root and isolated build inputs.
`helper-inputs.b3` binds the retained helper artifacts. The corresponding freshness logs pass.
The root `Cargo.lock`, `flake.lock`, and `dylint.toml` remain unchanged.
Cargo generated the only isolated lockfile changes: two existing `serde_json` dependency edges.

The helpers are not a Rust name resolver, independent review, or a substitute for compiler and behavior checks.
The evidence does not establish protocol-semantic authority, universal replay, production readiness, or release eligibility.
