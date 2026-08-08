## Context

`docs/assertion-readiness-status.md` is generated from accepted workload proofs and each committed `assertions.json`. It currently states that assertion density is not replay proof by itself and lists promotion gaps such as unhit, uncategorized, and non-passing assertions. The replay-readiness gate already protects accepted snapshot-backed replay claims, but assertion-readiness promotion remains guidance rather than a fail-closed guard.

## Goals / Non-Goals

**Goals:**
- Prevent assertion-density or exercised-count tables from being used as stronger support claims while known gaps are hidden or undocumented.
- Keep assertion-readiness promotion cheap and static: manifest/report/fixture checks, not KVM dogfood.
- Preserve a clear operator distinction between bounded replay proof and instrumentation readiness.

**Non-Goals:**
- Require every accepted workload to have perfect assertion coverage before bounded replay support remains valid.
- Run or curate a new dogfood proof.
- Change assertion macro semantics, assertion IDs, or replay verdict classes.

## Decisions

### 1. Static assertion-readiness checker

**Choice:** Add a deterministic checker around the accepted proof manifest, generated assertion-readiness report, and committed assertion artifacts.
**Rationale:** The failure mode is overclaiming/report drift. It should be caught without VM cost and without changing runtime behavior.
**Alternative:** Fold this into replay proof coverage; rejected because replay proof validity and assertion-instrumentation promotion are distinct operator claims.

### 2. Gap-preserving promotion semantics

**Choice:** Treat unhit, uncategorized, and non-passing assertion counts as required promotion evidence unless a workload-specific rationale explicitly documents why the remaining gaps are acceptable.
**Rationale:** Existing reports already compute these gaps. The gate should prevent accidental removal or silent weakening, not force immediate broad instrumentation work.

### 3. Negative fixture/self-test coverage

**Choice:** Include synthetic report/manifest cases that remove anti-claims, hide nonzero gap counts, or promote a workload beyond bounded proof without explicit rationale.
**Rationale:** These are the cheapest durable regressions for the overclaiming boundary.

## Risks / Trade-offs

**Gate brittleness** → Keep parsing constrained to generated surfaces and prefer structured data from accepted proof/assertion artifacts when possible.

**False promotion blockers** → Allow explicit workload-specific rationale for known gaps, but require it to be generated or checked rather than informal prose.
