# Campaign source acquisition

The development shell uses a scoped Cargo 1.98.0 repair for Campaign acquisition. The Rust compiler stays unchanged. Nix package builds retain their existing vendored-input path.

## Transport contract

`.cargo/config.toml` lists the exact Campaign HTTPS URL under `net.git-fetch-exact-revisions`. This list selects transport behavior. It is not repository authorization.

For a listed URL and a complete object ID, Cargo requests that object directly. It does not require an advertised default branch. A missing object or a server denial remains an error. There is no success fallback.

Unlisted URLs keep their previous behavior, including the GitHub fast path. Symbolic or abbreviated requests without a complete locked object ID keep stock resolution. An unlocked default-branch request still requires HEAD.

The consumer still pins `e23e3edf1dc6a8c612a4ea33a3b805bda1173e3b`. It uses the ordinary Campaign HTTPS endpoint. No source reference, Radicle signature, node identity, or namespace HEAD changes.

## Why the stock fetch fails

Stock Cargo fetches ordinary branches, tags, and HEAD for this non-GitHub revision. The ordinary forge view stores the checkpoint under a publisher namespace. The namespace view exposes the branch but lacks HEAD.

An isolated Git fixture reproduces that missing HEAD. A synthetic namespace HEAD makes Git advertise HEAD, but Radicle requires qualified, signed namespace references. The repair therefore belongs in client transport selection, not live Radicle storage.

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
- Patch: `nix/cargo-exact-revisions.patch`
- Package: `nix/cargo-exact-revisions.nix`
- Behavioral fixture: `tools/check-cargo-exact-revisions.rs`

Nix requires SHA-256 SRI for this fetch. Git object IDs retain their protocol algorithm. These identities do not replace stack-owned BLAKE3 content receipts.

The upstream source retains its MIT OR Apache-2.0 notices. ChaosControl maintainers own this consumer-specific patch and its exact-URL configuration. A compiler version change fails the package assertion and requires a source review. Remove the patch after the pinned upstream Cargo satisfies the same positive and negative controls.
