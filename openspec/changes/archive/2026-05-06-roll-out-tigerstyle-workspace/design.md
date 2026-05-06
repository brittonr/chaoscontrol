## Context

The repository currently pins local sibling `../tigerstyle` and `../verified-logic` flakes and exposes a staged Tigerstyle consumer gate named `tigerstyle-chaoscontrol-focused`. That gate currently checks `chaoscontrol-fault`, `chaoscontrol-protocol`, and `chaoscontrol-sdk` library targets only.

## Goals / Non-Goals

**Goals:**
- Apply the existing staged Tigerstyle lint profile to every Cargo workspace package.
- Keep the rollout reproducible through the already pinned local Tigerstyle flake.
- Preserve `--lib` target scope for this pass so binary/test cleanup remains future work.

**Non-Goals:**
- Do not enable the full Tigerstyle lint catalog in this change.
- Do not change sibling flake pins unless required by evaluation.
- Do not introduce broad suppressions for new findings; prefer small behavior-preserving source fixes.

## Decisions

### 1. Use an explicit full-package list instead of `--workspace`

**Choice:** Keep `mkConsumerCheck` package scoping as an explicit positive list containing every workspace package.

**Rationale:** The previous gate used package lists successfully. An explicit list documents the rollout surface and avoids accidentally widening to future experimental packages without an OpenSpec-backed decision.

**Alternative:** Use a raw `--workspace` gate. Rejected for this change because the existing Nix helper path already models packages explicitly.

**Implementation:** Update `[workspace.metadata.tigerstyle].default_scope` and `flake.nix` package list to match the root workspace members.

### 2. Keep staged lint profile and `--lib`

**Choice:** Retain the current `dylint.toml` deny set and `cargo_check_args = ["--lib"]` / `cargoExtraArgs = "--lib"`.

**Rationale:** The requested rollout is project-wide coverage for the established staged profile, not a full-catalog hardening wave.

**Alternative:** Enable all lints and all targets. Rejected as too broad for one rollout and likely to mix production fixes with test/binary debt.

## Risks / Trade-offs

**Workspace build noise** → Mitigated by preserving the staged lint profile and fixing only findings surfaced by currently denied lints.

**Scope drift** → Mitigated by recording the complete package list in both Cargo metadata and the canonical OpenSpec after archive.
