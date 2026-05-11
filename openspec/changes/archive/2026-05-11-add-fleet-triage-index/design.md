## Context

Single-receipt dashboard and local triage runbook are already generated. The missing surface is hosted/fleet triage; implementing a hosted service would be broader than the current ROI slice.

## Goals / Non-Goals

**Goals:** deterministic static HTML over multiple replay-readiness receipts; one CLI; explicit anti-overclaim language.

**Non-Goals:** hosted service, fleet scheduler, shared decision database, cross-machine workflow automation, or a full Antithesis replacement claim.

## Decisions

### Static receipt index first

**Choice:** Render a static HTML index from receipt paths.
**Rationale:** It reuses validated receipt summaries and is cheap to package and test.
**Alternative:** Start a web service/database; rejected as too broad for this slice.

## Risks / Trade-offs

**Overclaiming** → The renderer and readiness status state that this remains static artifact review, not hosted fleet UI or shared decision storage.
