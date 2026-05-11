## Context

ChaosControl should not claim arbitrary guest/device determinism, but it can improve confidence for the specific single-machine multi-hypervisor profiles we care about. Likewise, local campaigns should show which deterministic faults were exercised.

## Goals / Non-Goals

**Goals:**
- Expand bounded matrix rows for current local product profiles.
- Summarize deterministic fault coverage by class and workload.
- Keep unsupported/failing rows visible.

**Non-Goals:**
- Universal hypervisor/device/timing determinism theorem.
- Cross-machine fleet claims.
- Exhaustive fault taxonomy for all systems.

## Decisions

### 1. Matrix rows are product-profile rows

**Choice:** Add rows for named local multi-hypervisor profiles, not arbitrary guests.
**Rationale:** This matches current product scope and keeps evidence reviewable.

### 2. Fault coverage is receipt metadata

**Choice:** Campaign receipts summarize fault classes exercised, injections attempted, injections observed, and unsupported classes.
**Rationale:** Operators need to know what was actually tested without parsing logs.

## Risks / Trade-offs

**Overclaiming matrix expansion** → Keep row IDs explicit and add anti-claim fixtures for universal determinism wording.
