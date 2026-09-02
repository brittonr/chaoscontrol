## Context

ChaosControl executes deterministic exploration and replay. Property testing today is ad hoc: properties, generators, and oracles are not typed, seeds are not bound, and counterexamples are not minimized into evidence. MoonBit's QuickCheck rail shows the discipline that is missing: properties state invariants, generators construct valid domains, seeds make runs reproducible, and shrinking preserves the failure mode.

## Decisions

### Decision: Campaigns are typed and seed-bound

**Choice:** every campaign declares properties, generators, oracle kind, seed, bounds, and verdict classes; execution is reproducible from the recorded seed.

**Rationale:** determinism is a ChaosControl invariant. Typed campaigns keep verdicts and evidence unambiguous.

### Decision: Minimization reuses the reducer core

**Choice:** failure-mode-preserving, bound-checked shrinking lives in the existing reducer core; the campaign rail calls it.

**Rationale:** this avoids a second reducer and keeps one owner of reduction semantics.

### Decision: Agent-synthesized inputs are admitted by contract

**Choice:** properties and generators may come from an agent, but only through the campaign contract with provenance and distribution profiles.

**Rationale:** synthesis is useful exactly when it still cannot bypass the machine check and the bounds.

## Risks / Trade-offs

- Poor generator distributions waste runs; distribution profiles make the bias visible and reviewable.
- Naive reference models can share bugs with the implementation; round-trip and invariant oracles provide orthogonal checks.
- Large counterexamples obscure root cause; minimization with mode preservation bounds the noise.
