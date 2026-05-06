## Context

ChaosControl consumes pinned sibling `../tigerstyle` and `../verified-logic` flakes. The focused Tigerstyle check is already green for `chaoscontrol-fault` and `chaoscontrol-protocol` with a staged lint set.

## Goals / Non-Goals

**Goals:** Add `chaoscontrol-sdk` to the focused consumer gate, preserve the staged profile, and keep verification reproducible through the root flake.

**Non-Goals:** Enforce the full Tigerstyle catalog, cover every workspace crate, or change SDK runtime semantics.

## Decisions

### 1. Extend the existing focused check

**Choice:** Reuse `tigerstyle-chaoscontrol-focused` and append `chaoscontrol-sdk` to its package list and workspace metadata scope.

**Rationale:** The check name already represents a staged multi-crate gate, and adding another owned library crate keeps the rollout incremental.

**Alternative:** Create a separate SDK-only check. Rejected because it would duplicate profile and Nix wiring without improving CI signal.

### 2. Keep library-only scope

**Choice:** Keep `cargoExtraArgs = "--lib"` and `cargo_check_args = ["--lib"]`.

**Rationale:** Library targets are the stable staged boundary for the current gate. Broader targets can be added once the focused library gate is wider and green.

## Risks / Trade-offs

**SDK ambient boundary findings** → Fix narrow findings when they are behavior-preserving; otherwise document a follow-up instead of broad suppressions.

**Source-filter omissions** → Reuse `tigerstyleSrc`, which already keeps Cargo sources and `dylint.toml`.
