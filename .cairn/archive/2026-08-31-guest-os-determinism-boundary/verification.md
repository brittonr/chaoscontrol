# Verification

Date: 2026-08-31

## Baseline

Before implementation, `nix develop -c cargo test -p chaoscontrol-vmm --bin determinism_stress` and `nix develop -c cargo test -p chaoscontrol-protocol` passed. The protocol suite retained two pre-existing dead-code warnings.

## Adversarial feasibility result

The first gate compared two independent fresh boots. It correctly rejected the claim with `boot-entropy` and `monotonic-clock` drift. The repository's hide-TSC profile removed clock drift, but fresh-boot CRNG output still drifted.

The accepted design now starts the byte-exact claim at one quiescent snapshot. The specification, design, receipt non-claims, and operator guide record that limit. No fresh-boot CRNG equality claim remains.

## Accepted evidence

- The pure core derives the Linux boot seed, fixed-layout binding, clock policy, profile identity, setup-data bytes, and ordered drift verdicts.
- Unit tests accept identical probes and reject malformed entropy, malformed signals, missing clock controls, and individual entropy, clock, layout, and signal-order drift.
- The VMM writes a Linux x86 `SETUP_RNG_SEED` node and validates the effective command line before boot.
- The dedicated static guest fixture emits one stable marker before it reads `getrandom`, monotonic time, process addresses, and queued signal order.
- The shell captures one complete VM snapshot at that marker and compares two restored continuations.
- `nix develop -c cargo test --workspace --all-targets` passed.
- `nix develop -c cargo clippy --workspace --all-targets -- -D warnings` passed.
- `nix develop -c cargo fmt --all -- --check` passed.
- The valid Nickel receipt fixture exported. The incoherent accepted-drift fixture failed as required.
- `nix build .#guest-determinism-gate .#guest-determinism-probe .#initrd-determinism-probe .#determinism-probe-vmlinux --no-link -L --builders ''` passed.
- The Nix app ran on KVM and wrote `evidence/guest-determinism-receipt.json` with `accepted=true`, no drifted surfaces, and equal probe BLAKE3 identities.

## Broad Nix result

The broad default package still fails in the published VM Cohort dependency because `vm-cohort-conformance` omits `config/generated/profile.json`. This is independent of the focused guest determinism package. The focused host gate, guest, initrd, kernel, and KVM path passed without bypassing that defect.

## Product-scope result

This change narrows the existing deterministic-simulation surface. It does not add hosted, remote, cross-machine, container-runtime, or non-Rust SDK scope. The central product-scope command remains blocked by the unrelated active `add-protocol-observation-cohorts` package, which lacks a product-scope intent. Generated product-scope files were not bypassed or edited manually.

## Claim boundary

The accepted receipt establishes bit-exact output for two continuations from one admitted quiescent snapshot. It binds the run-derived boot seed, deterministic-jiffies clock profile, fixed-layout policy, and four observed surfaces. It does not establish fresh-boot CRNG equality, universal Linux determinism, arbitrary closed-binary determinism, syscall interception, host signal timing, or cross-machine replay.
