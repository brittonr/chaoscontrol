## Context

ChaosControl is a Rust-first deterministic VMM/simulation workspace. The sibling repos provide standards/proof inputs but were not represented in the flake lock.

## Goals / Non-Goals

**Goals:**
- Pin the local sibling HEAD revisions for Tigerstyle and verified-logic.
- Expose verified-logic as a Nix package/dev-shell tool.
- Add a verified-logic Verus proof rail check that can be evaluated and built from ChaosControl.

**Non-Goals:**
- No source vendoring.
- No ChaosControl runtime refactor.
- No ChaosControl source lint cleanup or full Tigerstyle consumer hard gate in this change.

## Decisions

### 1. Pin sibling Git revisions directly

**Choice:** Use `git+file:../...` inputs with explicit `ref` and `rev` for the current sibling HEADs.

**Rationale:** The user asked for the latest local sibling repos. Explicit revisions avoid dirty-worktree NAR-only locks while still naming the sibling source.

**Alternative:** Use `path:../...`; rejected because pure lock updates failed for parent-directory paths and produced less reproducible locks.

### 2. Let sibling flakes keep their own toolchain pins

**Choice:** Do not force sibling `nixpkgs`, `crane`, or `rust-overlay` to follow ChaosControl.

**Rationale:** verified-logic currently requires a Rust overlay with Rust 1.94.0; forcing ChaosControl's overlay made the upstream proof check fail to evaluate. Sibling proof/style flakes own their toolchains.

### 3. Wire both sibling tool surfaces now

**Choice:** Expose Tigerstyle `cargo-tigerstyle` and `tigerstyle-standards` packages, the Tigerstyle policy-registry check, the verified-logic package, and the verified-logic Verus proof check.

**Rationale:** Letting sibling flakes keep their own toolchain pins makes both tool surfaces evaluate cleanly from ChaosControl while avoiding premature ChaosControl source lint cleanup.

## Validation Plan

- `nix flake check --no-build` must evaluate all ChaosControl outputs, including the new verified-logic package/check.
- `nix build .#checks.x86_64-linux.tigerstyle-policy-registry --no-link -L` must complete.
- `nix build .#checks.x86_64-linux.verified-logic-verus-proofs --no-link -L` must complete with Verus reporting zero errors.
- `./scripts/openspec validate update-local-proof-style-inputs --strict` must pass before archive.
