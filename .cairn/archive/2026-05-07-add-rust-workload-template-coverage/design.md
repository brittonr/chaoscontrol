## Context

The current harness proves that a Rust workload can be packaged and replayed, and the local report already summarizes setup, assertion counts, and high-level gaps. The adoption gap is the first-hour path for a downstream Rust project: a copyable starting point and a report that shows exactly which assertion IDs/messages were registered but not observed.

## Goals / Non-Goals

**Goals:**
- Keep local dry-runs cheap and non-VM.
- Preserve the evidence boundary: local instrumentation reports are not replay proof.
- Make assertion coverage actionable at the per-site level.
- Provide a template that uses only public `chaoscontrol-sdk` APIs.

**Non-Goals:**
- No new language SDKs.
- No external Docker/OCI packaging path.
- No fresh KCOV/kernel build as part of the fast acceptance gate.

## Decisions

### 1. Template lives in docs/templates

**Choice:** Add a source template under `docs/templates/rust-workload/` rather than a new workspace member.

**Rationale:** The template is copyable downstream guidance and should not add workspace build churn or imply an in-tree product crate.

**Alternative:** Add another workspace crate. Rejected because the existing packaged sample crate already exercises the compiled in-tree path.

### 2. Local report keeps summary fields and adds detailed coverage

**Choice:** Keep `cataloged_assertions`, `exercised_assertions`, and existing gap arrays, then add stable `assertion_coverage` entries and derived registered/observed/unobserved arrays/counts.

**Rationale:** Existing evidence scripts remain compatible while downstream users get enough detail to fix blind spots.

**Alternative:** Replace the report schema. Rejected because committed dogfood/evidence tooling already consumes the v1 summary fields.

### 3. Local report remains instrumentation-only

**Choice:** The report continues to set `replay_evidence = false` and explicitly says VM/replay artifacts are separate.

**Rationale:** Avoid overclaiming local dry-run evidence as Antithesis-class replay proof.

## Risks / Trade-offs

**Report verbosity** → Mitigated by keeping summary fields and sorting detailed entries deterministically.

**Template drift** → Mitigated by matching the public example/harness style and documenting the existing Nix rails rather than inventing a second packaging path.
