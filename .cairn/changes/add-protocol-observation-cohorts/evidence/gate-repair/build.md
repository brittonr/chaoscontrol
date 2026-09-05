# Broad build repairs

## Exact crate mirrors

The broad check initially failed with HTTP 403 for four crate downloads through `crates.io/api/v1`.
The official `static.crates.io` mirror supplied the exact expected fixed-output object for each crate.
All four objects have retained GC roots. No package version, checksum, source pin, or lockfile changed for this repair.

| Crate version | Evidence |
| --- | --- |
| `wasm-smith` 0.220.1 | `wasm-smith-mirror.log` |
| `wasmparser` 0.220.1 | `wasmparser-mirror.log` |
| `wasmprinter` 0.220.1 | `wasmprinter-mirror.log` |
| `wasm-encoder` 0.220.1 | `wasm-encoder-mirror.log` |

Nix requires SHA-256 for these existing fixed-output identities. BLAKE3 remains the default for new repair manifests.

## Isolated adapter lockfile

The next broad check reached `vm-cohort-adapter-octet-deny-all`.
Octet found no source findings, but its lockfile guard rejected a changed `Cargo.lock`.
`flake-after-four-mirrors.log` retains that failure. The clean source report did not override the guard.

Cargo regenerated the isolated lockfile against the materialized workspace and its pinned vendored sources.
The only changes add the existing `serde_json` dependency to `chaoscontrol-protocol` and `chaoscontrol-sim-core`.
No package version, checksum, or external source changes.
The workspace `Cargo.lock` and `flake.lock` remain unchanged.

The unchanged strict adapter check now passes with zero findings and no lockfile mutation.
Evidence: `adapter-lock-generation.exit`, `adapter-lock-octet.log`, and `adapter-lock-octet.exit`.

## Artifact-install metadata

The next broad check compiled the Raft guest, then Cargo panicked during Crane artifact selection.
`flake-after-adapter-lock.log` retains that failure.
The build command used Cargo 1.98.0. The install hook made another unpatched `cargo metadata` call.

Both the host and musl Crane compositions now use the compatibility package for that metadata subprocess only.
The compiler, normal Cargo build command, artifact filters, workspace-member check, and dependency policy remain unchanged.
The hook wrapper retains the original body and requires an exact command substitution.

The first hook attempt covered only the host composition.
The second attempt used `postInstall`, which this setup-hook derivation does not run.
The drift control exposed that no-op. The final wrapper extends the existing `buildCommand` instead.
The retained failed attempts are not acceptance evidence.

The final Raft build uses Cargo 1.98.0 and successfully selects and installs its binary.
The missing-command fixture fails with the expected `substituteStream()` pattern error.
Evidence: `cargo-install-hook-verified.log`, `.exit`, and `cargo-install-hook-drift-final.log`, `.exit`.

These repairs do not establish a strict clean result for the broader source scope.
That source report still has 1,814 warnings. Lifecycle completion remains withheld.
