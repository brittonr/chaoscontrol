## Context

The committed evidence rail already exposes separate Python checks and Nix dogfood wrappers. The missing surface is operator composition: a single command that first proves committed readiness state, then optionally runs one selected slow KVM proof app.

## Goals / Non-Goals

**Goals:**
- Provide one checks-only default command for fast readiness review.
- Support an explicit `--dogfood <workload>` option for one selected slow VM proof rail.
- Keep existing evidence contract logic as the source of truth.

**Non-Goals:**
- Do not run all KVM proof rails by default.
- Do not curate or commit new dogfood evidence automatically.
- Do not claim full Antithesis product parity or universal determinism.

## Decisions

### 1. Thin Nix shell application

**Choice:** Implement the operator rail as a `pkgs.writeShellApplication` in `flake.nix`.

**Rationale:** The command mostly composes existing repository checks and already-packaged dogfood apps. A shell wrapper avoids duplicating evidence validation logic in another Python entrypoint.

**Alternative:** Add a Rust CLI subcommand. Rejected for this slice because readiness validation is currently repo/evidence-contract orchestration, not runtime replay semantics.

### 2. Checks before slow dogfood

**Choice:** Always run static readiness checks before optional KVM dogfood.

**Rationale:** If committed evidence or docs are stale, launching an expensive proof rail is wasted operator time. The command fails early with the existing checker diagnostics.

### 3. Explicit selected workload only

**Choice:** `--dogfood` accepts one workload and forwards remaining arguments after `--` to that workload wrapper.

**Rationale:** This keeps the command bounded and predictable while still making the slow proof rail discoverable.

## Risks / Trade-offs

**Kernel build surprise** → Mitigated by defaulting to checks-only and documenting that selected VM dogfood may build kernels when artifacts are not cached.

**Wrapper drift** → Mitigated by directly invoking the same packaged dogfood wrappers used elsewhere.
