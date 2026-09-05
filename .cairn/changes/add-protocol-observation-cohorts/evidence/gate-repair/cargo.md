# Cargo/Radicle repair evidence

The dependency-policy blocker is resolved for the selected Nix check.
The VM Cohort URL and revision remain unchanged. No lockfile, compiler, lint catalog, or host installation changes.
The ordinary unpatched Cargo command remains outside this compatibility result.

## Checked results

| Check | Result | Evidence |
| --- | --- | --- |
| New schema controls before repair | Two expected failures, three passes | `cargo-vendored-before.log` |
| Full schema unit suite after repair | 14 passes | `cargo-vendored-after.log` |
| Nix provider build and schema tests | Passed, including 14 schema tests | `cargo-package-manifest.log` |
| Actual locked offline metadata | Passed | `cargo-fixed-metadata.exit` |
| Actual VM Cohort package ID | Passed | `cargo-fixed-pkgid.log` |
| Dependency policy after formatter repair | Rejected missing policy entries | `dependency-policy.log` |
| Dependency policy after existing-fact corrections | Bans, licenses, and sources pass | `dependency-policy-after.log` |
| Near-match allowed URL | Rejected with `source-not-allowed`, exit 8 | `deny-source-negative.log` |
| Near-match license exception name | Rejected with `rejected`, exit 4 | `deny-license-negative.log` |

The provider source and patch scope are documented in `docs/cargo-radicle-compatibility.md`.
The new policy rows correspond to the accepted VM Cohort contract, the archived differential-core adoption, and the existing probe license.
These rows do not admit arbitrary Git sources or arbitrary AGPL dependencies.

## Rejected approaches and setup failures

The namespaced Radicle URL advertises refs, but Cargo fetch requires a `HEAD` ref that this namespace does not advertise.
The public Garden seed returned HTTP 500. The Onix seed returned not found.
These URL-only approaches did not resolve the exact dependency.

Two Nix attempts stopped before their checks started. They provide no acceptance evidence.
Explicit `builtins.path` values materialize the patch files for the successful derivation.
One provider build compiled Cargo but selected the wrong workspace for its test command.
The final command names the Cargo workspace manifest explicitly.
The configured Nix build directory remains unchanged.

The source repair changes both the formatter and the parser. Missing package names still fail.
This result does not establish a full-flake pass, a strict Octet pass, or lifecycle completion.
