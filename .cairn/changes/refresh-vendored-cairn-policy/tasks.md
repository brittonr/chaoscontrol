## Tasks

- [x] [serial] Reproduce the `outcome-machine` parse failure and confirm that a probe export from current Cairn policy sources lets `cairn validate` run. r[chaoscontrol.cairn_policy_sync.validation]
- [ ] [depends:policy-sync-baseline] Pin one Cairn revision and import `contracts.ncl` and `default.ncl` from it. Update `cairn-policy/UPSTREAM.md`. r[chaoscontrol.cairn_policy_sync.pinned_import]
- [ ] [depends:policy-sync-import] Check the import closure against the pinned revision. r[chaoscontrol.cairn_policy_sync.pinned_import]
- [ ] [depends:policy-sync-import] Regenerate `cairn-policy/generated/cairn-policy.json` with the pinned Cairn build and confirm that a fresh export matches. r[chaoscontrol.cairn_policy_sync.projection]
- [ ] [depends:policy-sync-projection] Run `cairn validate --root .` and fix or record every finding on active changes. r[chaoscontrol.cairn_policy_sync.validation]
