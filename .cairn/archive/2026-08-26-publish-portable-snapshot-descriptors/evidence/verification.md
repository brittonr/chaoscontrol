# Portable snapshot descriptor verification

Verified on 2026-08-26 in the dedicated ChaosControl worktree.

## Focused behavior

`cargo test -p chaoscontrol-snapshot-descriptor` passed:

- six positive identity, closure, destination, restore-plan, successful-restore, and consumer-reference tests;
- eight negative completeness, cohort, schema, profile, digest, topology, device, closure, mutation, poison, and authority-overclaim tests.

`cargo clippy -p chaoscontrol-snapshot-descriptor --all-targets --all-features -- -D warnings` passed.

The previously recorded focused descriptor, closure, evidence, exact-KVM, workspace-test, and contract-freshness rails remain passing.

## Strict Octet

The descriptor source was split at coherent framing and closure-validation boundaries. Narrow source allowances retain specific reasons for the closed DTO import surface.

Both the direct package command and the Nix check completed with:

- Status: `clean`
- Findings: `0`
- Warnings: `0`
- Errors: `0`
- Config: `b3:610cd3db70af4bf0dcd66b70d1f79e5bde5d2685432df2f518dd6c9ac5d446b7`
- Profile: `b3:5240bc54577077ed262ec7adc0ebbcc7fd8881f18637c8f2be62141d4875fb0f`

Commands:

- `cargo octet check -p chaoscontrol-snapshot-descriptor -- --all-targets --all-features`
- `nix build path:$PWD#checks.x86_64-linux.snapshot-descriptor-octet-deny-all -L --builders ''`

This focused result does not convert unrelated repository-wide findings into a pass.

## Contracts, Nix, and lifecycle

- Contract freshness was regenerated through the Rust owner.
- `snapshot-descriptor-contracts` passed with local builders.
- The fixture reported monolithic identity `Blake3:58bbf38e90abca0add48909c5968d4aaa9b8009843e84e8b830841a223792d41`.
- The fixture reported chunked identity `Blake3:60c0ec0797341679a1fac049f007d397b264992a07c9cdaaad98998bd228aa7b`.
- Strict Cairn validation passed with the shared policy.
- `git diff --check` passed.

The broad product-scope checker still stops on the unrelated active `add-protocol-observation-cohorts` change. Broad Nix evaluation still encounters the unrelated Mantle SpaceWasm store-path blocker. These blockers are not counted as descriptor passes or failures.

## Claim boundary

The verified descriptor proves exact identity, declared closure, exact supplied cohort compatibility, bounded restore observations, and refs-only consumer projection.

It does not prove guest correctness, universal portability, current authority, successful future restoration, semantic equivalence with logical state, or release eligibility.
