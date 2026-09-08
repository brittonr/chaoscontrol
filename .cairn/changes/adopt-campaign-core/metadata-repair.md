# Pathless package-ID repair

## Goal and boundary

Remove the Cargo metadata panic without changing source URLs, revisions, or dependency-policy checks. Acceptance requires the real locked workspace metadata, exact VM Cohort package ID, upstream package-ID controls, and the existing transport fixture.

No successful Cargo process alone establishes dependency-policy acceptance or Campaign runtime adoption.

## Cause

The baseline locked, offline metadata command panicked in `PackageIdSpec::fmt`. A separate `cargo pkgid -p vm-cohort-core` reproduced the panic. Its partial output ended after the exact `git+rad://` URL and revision.

Cargo 1.98.0 formats package IDs through two unchecked unwraps on the last URL path segment. The VM Cohort URL has no path segment. Ordinary compilation did not exercise this metadata serialization path.

The source comes from the immutable Rust source archive already pinned in `nix/cargo-exact-revisions.nix`. No remote branch or private visibility changed.

## Distinct checks

- The workspace backtrace locates the panic in package-ID serialization, not transport.
- The single-package command identifies VM Cohort as a triggering package.
- The upstream unit fixture constructs the exact pathless URL and checks the absent last segment.
- The metadata repair preserves the complete URL and prints an explicit package name.
- Parsing rejects a pathless URL without an explicit valid name. It also rejects version-only fragments and malformed names or versions.

These checks come from one assistant session. They are not independent reviewer approval.

## Implementation

`nix/cargo-pathless-package-ids.patch` changes only upstream package-ID formatting, parsing, and tests. It does not add a source fallback or invent a package name.

The Nix Cargo package runs the upstream package-ID tests with the new positive and negative cases. It retains all 18 existing Git library tests. Host and musl Crane scopes replace only Cargo. Their Rust compilers remain unchanged.

The build passed four package-ID tests and 18 Git library tests. Real locked, offline workspace metadata and the VM Cohort package-ID command now pass. Dependency-policy acceptance remains a separate check.

## Evidence

The operator retains these logs under `campaign/.pi/complete-20260906/`:

- `metadata-resume.log`: baseline metadata panic and stack trace.
- `metadata-vm-pkgid.log`: isolated package trigger.
- `metadata-cargo-build.log`: Nix Cargo build and upstream tests.
- `metadata-fixed-clean.json`: actual workspace metadata without the development-shell banner.
- `metadata-fixed-pkgid.log`: complete pathless package ID.
- `metadata-fixed-nix.log`: transport and dependency-policy checks.

The system `/tmp` reached its quota during diagnosis. The commands used a separate operator-owned temporary directory. No unrelated files or processes were removed.

## Dependency policy

The repaired checker reached the actual policy and rejected missing entries. The policy now names the exact Campaign, Choregraph, differential-harness, and VM Cohort source URLs. All revisions remain in the existing lockfile. No domain-wide Git allowlist was added.

Two package-specific license entries cover the existing guest determinism probe and Choregraph history. Both retain their existing AGPL licenses. The global license list still excludes AGPL.

The positive policy run checks bans, licenses, and sources. A repository-owned Rust tool removes each new source admission in turn and requires a source denial. It also removes each new license exception and requires a license denial. The first control run caught a misplaced Cargo-deny flag. The corrected invocation puts `--config` after `check`.

The final Nix dependency-policy check passes, including all six denial controls. The existing transport check also passes. The exploration library retains 214 passing tests and its one existing ignored placeholder. Strict Clippy and Rust formatting pass. Logs: `metadata-policy-controls-final.log`, `metadata-consumer-tests.log`, and `metadata-consumer-clippy.log`.

## Frozen full Nix result

The metadata repair is committed at `71ce5e0ed61393f43397ee40cd722336f1c79471`. The frozen archive passed the metadata and policy stages. The full Nix check then failed during dependency compilation:

```text
vm-cohort-conformance-0.1.0/src/standard.rs:33:34
couldn't read src/../../../config/generated/profile.json: No such file or directory
```

The pinned VM Cohort package embeds a file outside its crate. The vendored crate does not contain that parent-workspace file. The consumer must not bypass the generated-profile contract or copy an ambient sibling file into the build. A reviewed, published package-boundary repair remains necessary. The VM Cohort pin stays unchanged.

The full log is `metadata-full-nix.log`. This result supersedes the Cargo metadata panic as the immediate full-Nix blocker.

## Remaining boundary

This repair does not implement the durable journal or Campaign selection inside Explorer. It does not change the previously approved journal-root guard. Full Nix acceptance and lifecycle closure remain open until all relevant checks pass.
