## Context

The previous local proof/style input change pinned `../tigerstyle` and exposed Tigerstyle packages plus the policy-registry check. That proves the sibling toolchain is available, but not that ChaosControl source can be checked as a downstream consumer.

## Goals / Non-Goals

**Goals:**
- Wire a deterministic Tigerstyle consumer check into the root flake.
- Scope the first gate to one owned production crate with a small, reviewable finding surface.
- Keep noisy legacy lint families disabled until future focused drains.

**Non-Goals:**
- No workspace-wide hard gate.
- No broad source suppression or lint-debt cleanup.
- No changes to Tigerstyle itself.

## Decisions

### 1. Scope the first consumer gate to `chaoscontrol-fault`

**Choice:** Run `cargo-tigerstyle check -p chaoscontrol-fault -- --lib` through `tigerstyle.lib.mkConsumerCheck`.

**Rationale:** `chaoscontrol-fault` is an owned production crate with deterministic fault-scenario logic and no KVM/runtime dependency. It is small enough for a first gate but central enough to catch useful style drift.

The check includes `stdenv.cc` because Rust proc-macro/build-script compilation needs a `cc` linker inside the Nix sandbox.

**Alternative:** Gate the whole workspace immediately. Rejected because the first rollout should prove plumbing without forcing unrelated legacy debt into this slice.

### 2. Use a staged profile in `dylint.toml`

**Choice:** Enable a narrow deny set (`ambient_clock`, `clone_then_borrow`, `compound_assertion`, `contradictory_time`, `explicit_defaults`, `negated_predicate`, `unbounded_channel`) and disable noisier families for later drains.

**Rationale:** These lints are useful for deterministic fault code and are expected to be low-noise for the first crate.

**Alternative:** Deny the full catalog. Rejected because it would mix tool plumbing with a broad style-remediation change.

## Risks / Trade-offs

**Staged coverage is incomplete** → The new OpenSpec requirement records this as a focused first gate, not a full Tigerstyle adoption claim.

**Source filtering can omit config** → The flake uses a dedicated Tigerstyle source filter that includes `dylint.toml` with Cargo sources.

## Validation Plan

- Build `.#checks.x86_64-linux.tigerstyle-chaoscontrol-fault`.
- Evaluate root checks with `nix flake check --no-build`.
- Validate the OpenSpec change and canonical spec after archive.
- Run `git diff --check` before commit.
