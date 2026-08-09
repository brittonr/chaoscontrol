## Context

The repository presents current, historical, experimental, and non-goal material in several documents. Some old facts now conflict with the workspace and accepted evidence.

## Decisions

### 1. Nickel owns the scope registry

A typed registry lists capability ID, owner, state, evidence prerequisite, support boundary, and documentation targets. Rust or a repository tool exports deterministic inputs for consumers that require JSON.

### 2. Support state follows evidence

A feature remains experimental, deferred, or blocked until its named evidence gate passes. A completed implementation task cannot promote support by itself.

### 3. Active changes declare scope intent

Each product or architecture change names the target scope state and prerequisite. The identity-aware campaign remains blocked on its producer contract. Wasm component work remains experimental host-side evidence.

### 4. Facts come from authoritative inputs

Workspace crates come from Cargo metadata. Test inventory comes from the selected Cargo command. Proof status comes from accepted manifests. Architecture facts come from the scope registry, not old estimates.

### 5. Generated and narrative text stay separate

Tools own marked factual sections. Maintainers own rationale, tutorials, and non-claim prose. Freshness validation compares generated sections without rewriting narrative text.

### 6. The validator is pure at its decision boundary

The core compares loaded registry, repository facts, evidence facts, and document projections. Shells read files, run Cargo, and update marked sections.

## Risks

A fact can be technically current but misleading. Each generated fact therefore includes its source and bounded meaning. Scope state still requires human review and evidence.
