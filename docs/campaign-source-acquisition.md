# Campaign source acquisition

The development shell uses a scoped Cargo 1.98.0 transport repair for exact-revision acquisition from the private VM Cohort source. The same Cargo also supports explicit package names for pathless source URLs. Host and musl Crane builds use this Cargo. The Rust compilers and Nix vendored-input path stay unchanged. Campaign itself no longer needs the transport repair (see below).

## Transport contract

`.cargo/config.toml` lists the private VM Cohort Radicle URL under `net.git-fetch-exact-revisions`. This list selects transport behavior. It is not repository authorization.

The VM Cohort packaging repair uses a published signed branch without moving the default branch before consumer verification. Its root URL supports exact-object fetches. The declared transport capability avoids a cache preload or namespace-HEAD mutation.

For a listed URL and a complete object ID, Cargo requests that object directly. It does not require an advertised default branch. A missing object or a server denial remains an error. There is no success fallback.

Unlisted URLs keep their previous behavior, including the GitHub fast path. Symbolic or abbreviated requests without a complete locked object ID keep stock resolution. An unlocked default-branch request still requires HEAD.

Cargo and the Nix `campaign-src` input both pin Campaign revision `4a669eadaeac50b48efd77383219372be122f692` for `campaign-core`, `campaign-choregraph`, and the `campaign-conformance` dev-dependency. Both fetch from Campaign's public Radicle seed, `https://seed.radicle.garden/z2scC9MCm3pxk9mX4FEidRKabQ5LN.git`, which advertises `HEAD` and `refs/heads/main` at that revision, so stock resolution finds it and the URL is not in the exact-revision list. The former `git.onix.computer` Campaign endpoint answers "repository not found". Fresh acquisition from the seed was observed on 2026-10-21: `cargo update -p campaign-core -p campaign-choregraph -p campaign-conformance` fetched the seed's `main` and `HEAD`, and `nix flake update campaign-src` locked the same revision from the seed (narHash `sha256-L468HoGcG1ZkOvYsJcjN9LBfKxL4HEEq6I5tBstp+fs=`). Choregraph history `b3e08e19750f53bdbcae970cdf58a47a791ed20b` still comes from its `git.onix.computer` endpoint, which serves it with `HEAD`. This is a Campaign development revision, not a release revision. No source reference, Radicle signature, node identity, or namespace HEAD changes.

## Why the stock fetch fails

This section records why the repair was introduced, for the earlier Campaign forge endpoint; the VM Cohort source still relies on the repair. Stock Cargo fetches ordinary branches, tags, and HEAD for a non-GitHub revision. That forge view stored the checkpoint under a publisher namespace. The namespace view exposed the branch but lacked HEAD.

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
