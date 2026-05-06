## Context

The existing `tigerstyle-chaoscontrol-fault` check uses `tigerstyle.lib.mkConsumerCheck`, a `tigerstyleSrc` clean-source filter, `stdenv.cc`, and `--lib`. That plumbing is already proven green.

## Goals / Non-Goals

**Goals:**
- Add `chaoscontrol-protocol` to the staged focused Tigerstyle scope.
- Keep validation cheap and deterministic by checking library targets only.
- Rename or expose the check so its name describes the widened focused scope.

**Non-Goals:**
- Lint the full workspace.
- Enable additional noisy lint families.
- Change sibling Tigerstyle or verified-logic pins.

## Decisions

### 1. Keep one focused consumer check

**Choice:** Replace the single-crate check with a focused multi-package gate over `chaoscontrol-fault` and `chaoscontrol-protocol`.

**Rationale:** The gate remains staged and reviewable while reducing duplication in flake outputs.

**Alternative:** Add a second independent `tigerstyle-chaoscontrol-protocol` check. This would work, but it encourages fragmented CI surfaces before the focused rollout needs per-crate isolation.

**Implementation:** Use `packages = [ "chaoscontrol-fault" "chaoscontrol-protocol" ];` with `cargoExtraArgs = "--lib"`.

## Risks / Trade-offs

**Unexpected protocol findings** → Fix small semantic-preserving findings when straightforward; otherwise keep the scope staged rather than broadening configuration.

**Check name churn** → Document the new focused check name in OpenSpec and keep the old single-crate scenario superseded by the widened focused check.
