# Design: Adopt Nickel 1.17 for simulation configuration

## Context

ChaosControl uses Nickel for reviewable profiles and evidence configuration. The current Nix package set provides Nickel `1.15.1`.

## Decisions

### Decision: Use an exact narrow Nickel pin

**Choice:** Select Nickel `1.17.0` from commit `1320a983e6c3d1e2fb53dd2464b084b4903b1426`.

Do not update the full Nix package set only to obtain Nickel.

**Rationale:** The migration must not include unrelated VMM or toolchain changes.

### Decision: Keep simulation meaning outside the evaluator

**Choice:** Nickel validates profile shape. ChaosControl retains schedule, fault, guest, replay, and evidence meaning.

**Rationale:** Configuration acceptance cannot create simulation authority or correctness.

### Decision: Test representative profile outcomes

**Choice:** Run valid deterministic profiles and rejected malformed, missing-import, contract, bound, and unknown-field fixtures.

Regenerate `flake.lock` only with Nix lock commands.

**Rationale:** Both accepted and rejected profiles define the compatibility boundary.

### Decision: Record scoped evidence

**Choice:** Relevant receipts record the evaluator version and exact source identity.

The receipt stays scoped to the declared profile and harness.

## Risks and trade-offs

- The evaluator can reject previously accepted malformed profiles.
- Diagnostic wording can change while stable dispositions remain correct.
- Configuration compatibility does not prove VMM or workload correctness.
