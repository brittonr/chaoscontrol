# Design: Refresh Vendored Cairn Policy

## Context

`cairn-policy/default.ncl` is the ChaosControl overlay. It imports `upstream-default.ncl`, which imports `contracts.ncl`, `lifecycle-store-contracts.ncl`, and `nominal-evidence-identities.ncl`. The last two already match Cairn at `fde71b298b61856936c35b701876d4933393f0df`. The projection is committed so the CLI finds it by default.

## Decisions

### 1. One pinned revision

Every imported file comes from one full Cairn commit hash. `UPSTREAM.md` records that hash and the file mapping. No file mixes revisions.

### 2. Closure check

The import is complete only when every file in the overlay's import closure matches the pinned revision byte for byte, or is listed in `UPSTREAM.md` as ChaosControl-owned.

### 3. Projection regeneration

The projection is regenerated with the refresh command already named in the overlay, using the Cairn build at the pinned revision. The committed projection must equal a fresh export.

### 4. Validation findings

Every `cairn validate` finding on an active change is fixed in that change or recorded in this change's tasks with the reason it remains. The first probe found invalid cross-capability requirement markers in `isolate-branch-watchdog-timeouts`, which have been fixed.

## Risks

- A newer policy can reject existing active or archived artifacts. Those findings belong to the owning change and are not suppressed by policy edits.
