# Fresh public acquisition

## Result

The source-acquisition blocker is resolved for the Nix development profile at code commit `3cc7324c79fd66d4d6c4604b88e0c63b46cd467b`.

A metadata-free Git archive supplied the consumer source. The run used a new Cargo home with no Git database or checkout. It also used a new target directory outside the frozen source. Only the crates.io registry cache was shared.

The repaired Cargo fetched Campaign directly from its canonical HTTPS endpoint into `refs/commit/e23e3edf1dc6a8c612a4ea33a3b805bda1173e3b`. Choregraph and the other dependencies retained their existing fetch paths. The run did not change `Cargo.lock`.

The exploration library passed 211 tests. Its one existing KVM placeholder remained ignored. Focused strict Clippy passed from the same frozen source.

A Git archive from the new Cargo database matched the reference archive byte for byte. Its BLAKE3 digest is:

```text
78136b386be193f30ee75150eaf56f2576413cb70c4fc0b304983187008480f1
```

Strict Git object validation passed. A separate empty bare repository also fetched the older code commit `cb4d2823d3fb4524eb3f5b39f2cfd19aed60855f` and passed object validation.

## Supporting controls

The Cargo package runs 18 Git-related upstream library tests. The Nix fixture reproduces the stock missing-HEAD error and checks repaired exact-revision acquisition from two empty caches. It checks the compiled payload, named branches, missing default HEAD, invalid revisions, malformed configuration, and unchanged behavior for unlisted URLs.

The transport capability applies only to the exact Campaign URL in `.cargo/config.toml`. It is not repository authorization. Other remotes retain stock behavior. The compiler stays unchanged. There is no source-ref mutation, mirror, cache bootstrap, signature downgrade, or success fallback.

## Host preservation

Onix Core source `1bd4b5ac5bc0c35ce7dd4340c7657499d92414ce` restored the forge routes while preserving the newer Aspen1 state. The comparison caught and then preserved the live Kagi default and health monitor. It also preserved the `llm-client` tools. No system units or tools were removed.

The deployed system is `/nix/store/2z8a8cdrdaa9vdcha554inq517rrbkx3-nixos-system-aspen1-26.11.20260819.afe3d8a`.

Radicle node, HTTP gateway, nginx, uWSGI, and the Kagi health timer are active. The native node identity is unchanged. The repository root HEAD still points to `refs/heads/main`, whose value remains `10182365ec53e20a0fd4c02cfc188ac2fd1a5706`. No publisher namespace HEAD exists.

Root discovery returned 200. Unknown-repository, receive-pack discovery, and receive-pack POST controls returned 404.

## Limits and evidence

This result completes dependency acquisition, not Campaign-backed exploration. The product adapter, publication fence, conformance, full workspace gates, KVM proof, and lifecycle closure remain open.

Retained operator evidence:

```text
cargo-scoped-controls-result.log
public-cargo-final-tests.log
public-cargo-final-clippy.log
public-cargo-archive.tar
public-cargo-runtime-proof.log
public-cargo-http-proof.log
forge-preserved-unit-diff.log
forge-preserved-bin-diff.log
forge-restored-deploy.log
```
