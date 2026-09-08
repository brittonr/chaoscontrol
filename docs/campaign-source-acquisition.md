# Campaign source acquisition

The development shell uses a scoped Cargo 1.98.0 transport repair for Campaign acquisition. The same Cargo also supports explicit package names for pathless source URLs. Host and musl Crane builds use this Cargo. The Rust compilers and Nix vendored-input path stay unchanged.

## Transport contract

`.cargo/config.toml` lists the exact Campaign HTTPS URL and private VM Cohort Radicle URL under `net.git-fetch-exact-revisions`. This list selects transport behavior. It is not repository authorization.

The VM Cohort packaging repair uses a published signed branch without moving the default branch before consumer verification. Its root URL supports exact-object fetches. The declared transport capability avoids a cache preload or namespace-HEAD mutation.

For a listed URL and a complete object ID, Cargo requests that object directly. It does not require an advertised default branch. A missing object or a server denial remains an error. There is no success fallback.

Unlisted URLs keep their previous behavior, including the GitHub fast path. Symbolic or abbreviated requests without a complete locked object ID keep stock resolution. An unlocked default-branch request still requires HEAD.

The consumer still pins `e23e3edf1dc6a8c612a4ea33a3b805bda1173e3b`. It uses the ordinary Campaign HTTPS endpoint. No source reference, Radicle signature, node identity, or namespace HEAD changes.

## Why the stock fetch fails

Stock Cargo fetches ordinary branches, tags, and HEAD for this non-GitHub revision. The ordinary forge view stores the checkpoint under a publisher namespace. The namespace view exposes the branch but lacks HEAD.

An isolated Git fixture reproduces that missing HEAD. A synthetic namespace HEAD makes Git advertise HEAD, but Radicle requires qualified, signed namespace references. The repair therefore belongs in client transport selection, not live Radicle storage.

## Pathless package IDs

VM Cohort uses a pinned `rad://` source whose URL has no path component. Stock Cargo 1.98.0 panics during package-ID formatting for metadata and `pkgid`.

`nix/cargo-pathless-package-ids.patch` emits an explicit package name when no final path segment matches that name. Parsing accepts a pathless URL only with an explicit valid name. A missing name or a version-only fragment remains an error. Existing path-based package IDs retain their format.

This repair changes metadata handling, not source authority or acquisition. The VM Cohort URL, revision, and private visibility remain unchanged.

## Repeatable checks

```sh
nix build .#checks.x86_64-linux.cargo-exact-revisions
nix develop -c cargo test -p chaoscontrol-explore --lib --locked
nix develop -c cargo clippy -p chaoscontrol-explore --lib --tests --locked -- -D warnings
```

The transport fixture uses an isolated loopback Git daemon and a new Cargo home for every acquisition. It checks the exact compiled payload and repeats the locked fetch from another empty cache. Named branches still work. Unlocked requests with missing default HEAD, unknown revisions, and malformed capability configuration fail. Unlisted URLs keep the stock behavior.

The package also runs 18 upstream Git-related library tests. These focused checks do not prove full Cargo compatibility or ChaosControl adoption. Fresh acquisition from the live public endpoint remains a separate acceptance step.

## Source and maintenance

Source repository: `rust-lang/cargo`, as shipped in the official Rust 1.98.0 source archive.

- Archive: `https://static.rust-lang.org/dist/rustc-1.98.0-src.tar.gz`
- Immutable Nix fetch identity: `sha256-siau83X/vp++K4X96Za1BxbVnVUmjiQNBSOWU0t16Sk=`
- Transport patch: `nix/cargo-exact-revisions.patch`
- Metadata patch: `nix/cargo-pathless-package-ids.patch`
- Package: `nix/cargo-exact-revisions.nix`
- Behavioral fixture: `tools/check-cargo-exact-revisions.rs`

Nix requires SHA-256 SRI for this fetch. Git object IDs retain their protocol algorithm. These identities do not replace stack-owned BLAKE3 content receipts.

The upstream source retains its MIT OR Apache-2.0 notices. ChaosControl maintainers own this consumer-specific patch and its exact-URL configuration. A compiler version change fails the package assertion and requires a source review. Remove the patch after the pinned upstream Cargo satisfies the same positive and negative controls.
