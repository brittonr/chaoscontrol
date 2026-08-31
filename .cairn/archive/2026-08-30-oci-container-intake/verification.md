# Verification

Date: 2026-08-30

## Baseline

Before implementation, the focused protocol, SDK, and evidence tests passed.

## Accepted evidence

- `nix develop -c cargo test -p chaoscontrol-protocol -p chaoscontrol-evidence oci_intake -- --nocapture` passed.
- The positive shell fixture materialized an ordered OCI layer and a bounded directory source into one two-service bundle.
- Negative tests reject unsupported formats, duplicate roles, unsafe entrypoints, wrong layer identities, existing outputs, and receipt provenance drift.
- `nix develop -c cargo test --workspace` passed.
- `nix develop -c cargo clippy --workspace --all-targets -- -D warnings` passed.
- `nix develop -c cargo fmt --all -- --check` passed.
- The Nickel topology contract typechecked. The conformant multi-service fixture exported, while duplicate-role and unsafe-entrypoint fixtures failed as required.
- `nix build .#oci-intake --no-link -L --builders ''` passed. Its focused Crane dependency derivation excludes unrelated workspace packages and the published VM Cohort source defect.
- Directory admission and copy use Bounded Tree at `b0fd0103bc9eed2c1b6d852045959462d105d8f1`.

## Broad Nix result

`nix flake check -L --builders ''` evaluated the new package and app. Broad completion was blocked when crates.io returned HTTP 403 for existing SpaceWasm dependencies such as `wasm-smith 0.220.1`. Earlier repository-wide runs also retain the independent `cargo deny` metadata panic. The focused OCI package is complete and passed.

## Product-scope result

`AGENTS.md`, `README.md`, and the OCI guide now replace the old image-intake non-goal with an experimental packaging boundary. Docker, Compose, Kubernetes, registries, namespaces, and cross-machine scheduling remain unsupported. The central generated product-scope command remains blocked by the unrelated active `add-protocol-observation-cohorts` package, which has no product-scope intent. Generated product-scope files were not bypassed or edited manually.

## Claim boundary

The evidence establishes bounded topology lowering, declared layer order, regular-file and directory extraction, OCI whiteouts, Bounded Tree directory revalidation, atomic bundle publication, and identity-bound receipts for the tested inputs. It does not establish image trust, namespace isolation, registry behavior, vulnerability scanning, filesystem correctness, service correctness, or a successful VM campaign.
