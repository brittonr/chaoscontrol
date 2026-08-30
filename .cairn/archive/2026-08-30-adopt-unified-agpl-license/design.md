## Context

The current license map follows runtime authority. Protocol, SDK, guest support, fixtures, and templates use Apache-2.0. Host and controller crates use AGPL-3.0-or-later.

The planned shared assertion and deterministic-testing crates cross that split. A single AGPL boundary permits direct reuse without duplicate implementations.

## Decisions

### Decision: Use AGPL-3.0-or-later for repository-owned source

**Choice:** Future revisions of all repository-owned crates, tools, templates, lifecycle material, and documentation use AGPL-3.0-or-later.

**Rationale:** One project license removes internal dependency restrictions and matches the host-side research stack.

### Decision: Check authority before changing each path

**Choice:** Build an explicit path inventory. Change a path only when repository ownership or a compatible grant permits the change. Keep unknown or third-party terms unchanged and visible.

**Rationale:** A repository-level decision cannot replace terms that the project does not control.

### Decision: Preserve prior grants

**Choice:** State that earlier Apache-2.0 releases and grants remain valid. Apply the new policy to future repository-owned revisions.

**Rationale:** A new license choice does not revoke rights already granted.

### Decision: Make package archives self-contained

**Choice:** Each published crate includes the AGPL-3.0-or-later text and matching package metadata. Mixed source archives include a path map for all exceptions.

**Rationale:** Offline reviewers need complete and accurate terms.

### Decision: Treat copied templates as source distribution

**Choice:** Repository-owned template source and generated copies include AGPL notices. Documentation explains the effect before users copy or generate them.

**Rationale:** Templates contain distributable source and need an explicit license.

### Decision: Keep runtime outputs outside the license map

**Choice:** VM output, reports, traces, receipts, and unrelated workload output are not automatically relicensed. A versioned format can include license metadata without changing this rule.

**Rationale:** Processing data does not transfer source-code ownership.

## Risks / Trade-offs

- Existing Apache-only consumers need to pin an earlier version or accept the new terms.
- Template adoption becomes less permissive.
- Mixed third-party paths still need separate review.
- Package policy changes can expose previously hidden metadata errors.
