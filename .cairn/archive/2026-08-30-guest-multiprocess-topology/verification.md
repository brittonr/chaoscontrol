# Verification

Date: 2026-08-30

## Baseline

Before implementation, the focused protocol, SDK, fault, VMM, explorer, replay, and evidence tests passed. The VMM suite reported 494 passed tests, zero failures, and nine ignored tests.

## Accepted evidence

- `nix develop -c cargo test -p chaoscontrol-protocol -p chaoscontrol-fault -p chaoscontrol-sdk -p chaoscontrol-vmm -p chaoscontrol-explore` passed.
- `nix develop -c cargo test --workspace` passed.
- `nix develop -c cargo clippy --workspace --all-targets -- -D warnings` passed.
- `nix develop -c cargo fmt --all -- --check` passed.
- The Nickel process-manifest and multiprocess-receipt contracts typechecked.
- The conformant two-process Nickel fixture exported successfully.
- The duplicate-role, duplicate-transport, and unknown-directory Nickel fixtures failed as required.
- `cargo test -p chaoscontrol-sdk --test multiprocess_shell` started two real child processes, observed shared state, restarted only the writer, preserved the shared file, and stopped both children.
- `nix build .#guest-supervisor .#initrd-multiprocess --no-link -L --builders ''` passed. The package uses an explicit Crane install phase because Cargo 1.98 metadata parsing panics in the generic binary-discovery hook.
- Positive and negative Rust tests cover admission, stable BLAKE3 identities, restart policy, shared-state identity, targeted kill/pause/restart, exact pause release, unknown targets, queue replay, duplicate requests, transport-slot conflicts, process-scoped assertions, receipt drift, and claim overreach.

## Broad Nix result

`nix flake check -L --builders ''` evaluated the new packages and 83 checks. The existing `dependency-policy` derivation blocked completion. Its pinned `cargo deny` process panicked in Cargo metadata parsing at `package_id_spec.rs:248`. The focused supervisor and initrd derivations pass with the narrow install workaround.

## Product-scope result

The README roadmap and the dedicated boundary guide now classify this feature as experimental and bounded. The generated product-scope command could not refresh the central generated table because the unrelated active `add-protocol-observation-cohorts` change lacks its required product-scope intent. Canonical generated product-scope files were left unchanged rather than bypassing that guard.

## Claim boundary

The evidence establishes the typed manifest, pure transition plans, real local child supervision, shared-directory persistence across one process restart, bounded host command transport, package construction, and process-scoped record validation for the tested cohort. It does not establish container isolation, arbitrary child-catalog composition, filesystem correctness, cross-VM process scheduling, or a live multiprocess KVM campaign.
