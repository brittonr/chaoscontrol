## Context

ChaosControl already has bounded replay proof, local multi-hypervisor KVM smoke, and networked harness receipts. The product direction is not SaaS, multi-machine fleets, or a multi-language SDK. Leaving those as the headline gaps makes the roadmap optimize for the wrong target.

## Goals / Non-Goals

**Goals:**
- Treat Rust-only SDK support as deliberate current scope.
- Treat single-machine multi-hypervisor campaigns as the operator/product target.
- Keep anti-overclaim language for hosted/multi-machine claims without ranking them as current product gaps.

**Non-Goals:**
- Removing historical archived evidence.
- Claiming full Antithesis replacement.
- Building hosted service, cross-machine queueing, or non-Rust SDKs.

## Decisions

### 1. Product scope is explicit readiness metadata

**Choice:** Add generated/readiness scope language and fail-closed checks that say current product gaps are local/Rust gaps.
**Rationale:** This prevents future work from drifting back toward SaaS/fleet/multi-language comparisons.
**Alternative:** Rely on memory or ad hoc reports. Rejected because generated docs are what operators and future agents read.
**Implementation:** Update readiness surface constants/docs and tests so hosted/multi-machine/non-Rust items are non-goals, while still prohibited as overclaims.

## Risks / Trade-offs

**Understating future optional work** → Keep wording as current-scope non-goals, not permanent impossibilities.
