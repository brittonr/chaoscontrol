## Tasks

- [x] [serial] Add detached fuzz workspaces with deterministic input parsing and bounded shapes. r[chaoscontrol.fuzzing.guest_boundary_targets]
- [x] [serial] Implement payload, catalog-frame, descriptor-chain, and request targets with round-trip, canonicality, and plan-soundness checks. r[chaoscontrol.fuzzing.guest_boundary_targets]
- [x] [serial] Generate committed seed corpora from existing protocol and virtio fixtures. r[chaoscontrol.fuzzing.guest_boundary_targets]
- [x] [serial] Add the bounded smoke script and non-default `nix run .#fuzz-smoke` app. r[chaoscontrol.fuzzing.guest_boundary_targets]
- [x] [serial] Run every target under the smoke and fix harness defects; record results. r[chaoscontrol.fuzzing.guest_boundary_targets]
- [x] [serial] Keep a scheduled or CI job that runs the smoke and files crashers as regressions. r[chaoscontrol.fuzzing.guest_boundary_targets]

## Evidence

Four targets across two detached workspaces. 30-second cargo-fuzz runs: payload
2,659,570, catalog_frames 3,685,474, virtio_chain 927,234, virtio_request 1,099,604,
no crashes. The committed `scripts/fuzz-smoke.sh` then ran all four for five seconds
each and reported completion, including through `nix run .#fuzz-smoke` with the
Nix-provided nightly toolchain. Two harness defects were fixed during verification
(undersized event re-encode buffer, zero-length containment reference); no
production crate change was required. The smoke app requires a nightly toolchain
and network for crate fetching and is not part of `nix flake check`. A clean run is
bounded evidence only; it does not prove decoder or planner correctness.
