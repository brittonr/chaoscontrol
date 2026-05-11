## Context

Assertions are useful local invariants, but Jepsen-style competitors model histories of client operations and then check consistency properties such as linearizability or register/queue semantics. ChaosControl needs a bounded version that integrates with existing replay artifacts but does not claim replay proof from checker output alone.

## Goals / Non-Goals

**Goals:**
- Record deterministic operation histories from workloads or adapters.
- Provide checker interfaces and at least one concrete model checker slice.
- Emit machine-readable reports with counterexample traces when histories fail.
- Keep checker evidence distinct from replay and assertion-readiness support labels.

**Non-Goals:**
- Reimplementing all Jepsen workloads/checkers at once.
- Requiring Clojure or Jepsen runtime dependencies.
- Treating semantic checker pass as deterministic replay proof.

## Decisions

### 1. Rust-owned typed histories

**Choice:** Define typed Rust DTOs for operations, process/client IDs, invocation/completion times, outcomes, and workload metadata.
**Rationale:** This matches ChaosControl evidence patterns and avoids raw-log scraping.
**Alternative:** Parse ad-hoc logs after the fact; rejected because it is fragile and violates existing receipt practices.

### 2. Checker trait plus first model

**Choice:** Add a checker trait and one small model family first, with fixtures for invalid histories.
**Rationale:** This creates the ecosystem seam without overbuilding.
**Alternative:** Build a broad checker catalog immediately; rejected as too wide for one drain.

## Risks / Trade-offs

**Checker complexity** → Start with finite bounded histories and explicit model limitations.
**Semantic overclaims** → Reports and readiness surfaces must state that checker evidence is semantic validation, not replay proof.
