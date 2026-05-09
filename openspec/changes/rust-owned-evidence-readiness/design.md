## Context

The repository currently has a green full flake check and refreshed accepted proofs for `net`, `redb`, `raft`, and `rust-workload`. The remaining evidence/control-plane risk is implementation-language fragmentation: most committed evidence checks and report generators live in Python, while snapshot replay smoke and documentation generation use Bash wrappers. Recent work also showed generated public docs can drift when only a subset of reports is derived from the accepted proof manifest.

## Goals / Non-Goals

**Goals:**
- Move evidence/readiness parsing, validation, rendering, and chunk materialization into Rust-owned library code with thin CLI shells.
- Preserve current fail-closed semantics, output files, and bounded anti-claim language during migration.
- Make `docs/replay-proof-coverage.md` generated/checkable from `dogfood-results/accepted-workload-proofs.json`.
- Wire migrated Rust commands into Nix checks before deleting their Python/Bash predecessors.

**Non-Goals:**
- Rewriting all VM/dogfood process orchestration in the first slice.
- Changing accepted proof semantics or promoting new workload claims.
- Removing legacy snapshot codec compatibility.
- Replacing Nickel-authored config/receipt contracts; Rust remains source of truth for runtime-derived records and validators.

## Decisions

### 1. Rust library first, CLI second

**Choice:** Create a Rust evidence/readiness core that owns manifest models, artifact validation, report rendering, and chunk materialization, then expose narrow CLI subcommands.

**Rationale:** A pure core keeps parsing/rendering testable without Nix or KVM and avoids another collection of one-off binaries.

**Alternative:** Port each Python script to a separate Rust binary. Rejected because it would preserve duplication and make drift prevention harder.

### 2. Migration by compatibility slices

**Choice:** Replace one gate/report at a time, starting with replay proof coverage and `docs/replay-proof-coverage.md` generation, while keeping old commands available until Nix/docs are migrated.

**Rationale:** The current evidence surface is green; small slices lower risk and make before/after comparisons straightforward.

**Alternative:** Delete all Python/Bash scripts in one rewrite. Rejected because it would combine many public operator surfaces and make regressions harder to isolate.

### 3. Fail-closed parity before deletion

**Choice:** A migrated Rust command must demonstrate positive parity on committed evidence and negative coverage for malformed/stale/tampered inputs before its Python/Bash predecessor is removed from gates.

**Rationale:** These tools guard proof claims. Losing a negative check is worse than retaining temporary duplication.

### 4. Shell remains only at orchestration boundaries

**Choice:** Process launch wrappers may remain temporarily for VM/dogfood orchestration, but proof validation, report generation, summary rendering, and materialization decisions must become Rust-owned.

**Rationale:** Rust is the right source of truth for structured evidence. Some Nix/app wrappers may still need shell glue, but that glue must not contain proof policy.

## Risks / Trade-offs

**Output drift during migration** → Preserve golden fixtures or before/after transcripts for every migrated command and wire the Rust command into the same Nix check before removing the old one.

**Over-broad rewrite stalls progress** → Start with the stale `docs/replay-proof-coverage.md` gap and migrate only enough surface to prevent that drift.

**New Rust dependencies expand policy surface** → Run cargo-audit/cargo-deny and prefer serde/standard workspace dependencies already accepted by current policy.

## Validation Plan

- `cargo fmt --check`
- Focused Rust tests for evidence/readiness core and CLI subcommands.
- Positive and negative fixture tests for each migrated gate.
- Old-script parity checks via `git show HEAD:<script>` while the predecessor still exists in the previous commit.
- `nix build .#checks.x86_64-linux.evidence-contracts --no-link -L`
- `nix build .#checks.x86_64-linux.replay-readiness --no-link -L`
- `nix flake check -L` before declaring the migration complete.
