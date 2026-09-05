# Cargo and Radicle package identities

The dependency-policy Nix check uses a repository-owned Cargo compatibility package.
Crane also uses this package for the metadata subprocess that selects installable artifacts.
The compiler, normal Cargo build command, VM Cohort URL, and VM Cohort revision remain unchanged.

```console
nix run .#cargo-radicle -- metadata --offline --locked --format-version 1
nix run .#cargo-radicle -- pkgid --offline --locked -p vm-cohort-core
nix build .#checks.x86_64-linux.dependency-policy -L
```

The ordinary unpatched Cargo command can still reject the pathless Radicle source.
Use the explicit compatibility package for metadata diagnostics.
This package does not replace the host Cargo installation or change transport authority.

## Provider and correction

`nix/cargo-radicle.nix` derives the provider from Nixpkgs `a82ccc39b39b621151d6732718e3e250109076fa`, which supplies Cargo 1.92.0.
The existing `flake.lock` pins that source. The patches change `cargo-util-schemas` 0.10.2.
An exact version assertion requires review before a future provider upgrade.
The package builds the upstream source with two repository-owned patches.

`PackageIdSpec::fmt` previously unwrapped a missing URL path segment.
The parser also rejected a pathless source even when its fragment supplied a complete package name.
The correction requires a path only when Cargo must infer the package name from it.
The formatter always retains an explicit name when the URL supplies no matching path name.
Missing names, malformed versions, unsupported source protocols, and empty fragments remain errors.

The patch tests reproduce both failures before the correction.
All 14 schema tests pass after the correction, including the existing URL and version cases.
The packaged executable also passes locked offline metadata and package-ID checks for the real VM Cohort dependency.
The Nix package runs schema tests against the explicit Cargo workspace manifest.

Crane compiles guests with the normal build toolchain.
Its install hook then uses the compatibility package for `cargo metadata`.
The hook retains the original workspace-membership and non-test artifact filters.
A fail-closed substitution changes only that metadata command in the pinned Crane hook.
The normal compiler and Cargo build command do not change.

Upstream Cargo retains its MIT OR Apache-2.0 notices and source terms.
Repository-owned patch additions follow the ChaosControl AGPL-3.0-or-later policy.
No upstream public license or third-party notice changes.

## Existing policy entries

The repaired metadata path exposed policy entries missing from `deny.toml`:

- The exact VM Cohort URL already appears in its accepted adoption contract and dependency check.
- The exact differential-core URL and revision appear in the archived `2026-08-22-adopt-differential-execution-core` change.
- The existing guest-determinism probe already uses the workspace AGPL license.

The policy now records these existing facts. Unknown Git sources and unknown registries remain denied.
The source URLs, dependency revisions, workspace `Cargo.lock`, and `flake.lock` remain unchanged.
Negative controls change only the allowed URL or exception name to a near match. Both controls retain the expected rejection.

The evidence is under `.cairn/changes/add-protocol-observation-cohorts/evidence/gate-repair/`.
A passing dependency check does not establish a clean Octet gate, protocol correctness, or release readiness.
