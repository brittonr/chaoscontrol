# Targeted Raft replay attempt — 2026-05-06

## Scope

Targeted `raft_bug=fig8_commit` campaign to force bug artifacts and test standalone replay after persisted replay-parent snapshot support landed.

## Result

- Checkpoint reached round 14 with 124 branches and 24 recorded bugs.
- Exported 4 checkpoint bug records to `bug_*.json` for replay probing.
- All exported bugs had `replay_parent_depth = 0` and `replay_parent_snapshot_ref = null`; snapshot-backed replay was not exercised.
- Standalone reproduce failed for all 4 exported bugs; this is schedule-only replay-insufficient evidence, not a passing replay receipt.

## Reproduce results

- `bug_0`: exit 1; ○ Bug NOT reproduced — assertion 3039225728 did not fail
- `bug_1`: exit 1; ○ Bug NOT reproduced — assertion 1813441339 did not fail
- `bug_2`: exit 1; ○ Bug NOT reproduced — assertion 2375026300 did not fail
- `bug_3`: exit 1; ○ Bug NOT reproduced — assertion 3039225728 did not fail

## Minimize result

- `bug_0`: exit 0; Minimization Result

## Known gap

The useful next implementation slice is to add a bounded first-class checkpoint bug export/finalization path, or run a longer uninterrupted targeted campaign until normal final bug saving emits refs.
