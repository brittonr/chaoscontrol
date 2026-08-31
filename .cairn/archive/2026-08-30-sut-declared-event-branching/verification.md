# Verification

Date: 2026-08-30

## Accepted evidence

- `nix develop -c cargo test -p chaoscontrol-protocol -p chaoscontrol-fault -p chaoscontrol-sdk -p chaoscontrol-explore -p chaoscontrol-replay -p chaoscontrol-evidence` passed.
- The focused suites include positive marker admission, stable identity, collapse, rare-marker scoring, frontier retention, and replay binding.
- The focused suites include negative malformed-marker, marker-limit, never-reached, and identity-drift cases.
- `nix develop -c cargo clippy -p chaoscontrol-protocol -p chaoscontrol-fault -p chaoscontrol-sdk -p chaoscontrol-explore -p chaoscontrol-replay -p chaoscontrol-evidence --all-targets -- -D warnings` passed.
- `nix develop -c cargo fmt --all -- --check` passed.
- `nix develop -c cargo test --workspace` passed.
- `nix develop -c cargo clippy --workspace --all-targets -- -D warnings` passed.

## Nix check

`nix flake check -L --builders ''` evaluated the full flake and started 85 checks. The existing `dependency-policy` derivation blocked completion. Its pinned `cargo deny` process panicked inside Cargo metadata parsing at `package_id_spec.rs:248` after an `Option::unwrap()` call. This failure is outside the branch-marker change. The same repository-wide blocker existed before this change.

The first remote-builder attempt also received crates.io HTTP 403 responses. The local-builder retry removed that network failure and reproduced only the existing `dependency-policy` panic.

## Claim boundary

The evidence proves bounded marker validation, deterministic identity projection, frontier metadata, replay-binding validation, and local coverage-gap projection for the tested inputs. It does not prove that guest marker placement is useful, that all reachable states were explored, or that every repository-wide Nix check passed.
