# Refresh Vendored Cairn Policy

## Why

ChaosControl vendors Cairn policy sources from Cairn revision `b6759012db4dab0158936e995b7bad39ac10465b` (`cairn-policy/UPSTREAM.md`). Current Cairn binaries reject the committed projection. On 2026-09-25, both the installed `cairn` and a build of Cairn `fde71b298b61856936c35b701876d4933393f0df` failed every command that loads policy with:

```text
failed to parse policy cairn-policy/generated/cairn-policy.json: cairn.workflow.profile.registry at workflow_profile_policy.schemas: initial workflow profile is missing: outcome-machine
```

`cairn validate` could not run for any active change. A probe outside the repository copied Cairn's current `cairn-policy/contracts.ncl` and `cairn-policy/default.ncl` (last changed at `6f18c986ba8a0bc2dcd9cd20f7e4c7154129e6ef`), kept the ChaosControl overlay, and regenerated the projection. With that projection, `cairn validate --root .` ran and reported findings for the active changes.

## What Changes

- Import `contracts.ncl` and `default.ncl` from one pinned Cairn revision and record that revision in `cairn-policy/UPSTREAM.md`. r[chaoscontrol.cairn_policy_sync.pinned_import]
- Check that the imported import closure (`contracts.ncl`, `lifecycle-store-contracts.ncl`, `nominal-evidence-identities.ncl`) matches the pinned revision. r[chaoscontrol.cairn_policy_sync.pinned_import]
- Regenerate the committed projection from the ChaosControl overlay. r[chaoscontrol.cairn_policy_sync.projection]
- Run `cairn validate` with the pinned Cairn build and resolve or record every finding on active changes. r[chaoscontrol.cairn_policy_sync.validation]

## Impact

- **Files**: `cairn-policy/contracts.ncl`, `cairn-policy/upstream-default.ncl`, `cairn-policy/UPSTREAM.md`, and `cairn-policy/generated/cairn-policy.json`.
- **Lifecycle**: active changes gain policy checks they currently skip. Findings may require edits to change documents.

## Non-Goals

- No change to the ChaosControl overlay's traceability profile or policy name.
- No claim of policy correctness beyond source identity and successful validation.
