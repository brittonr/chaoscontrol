## Tasks

- [x] [serial] Confirm the shared exit status, the hidden `ExploreError::Vm` source, and the worker-pool placeholder conversion. r[chaoscontrol.explorer_outcomes.exit_status]
- [ ] [depends:outcomes-baseline] Add the pure outcome-to-status mapping and replace literal exit calls in exploration commands. r[chaoscontrol.explorer_outcomes.exit_status]
- [ ] [depends:outcomes-baseline] Print error source chains and include the VM error in `ExploreError::Vm` display. r[chaoscontrol.explorer_outcomes.error_chain]
- [ ] [depends:outcomes-baseline] Add failed branch outcomes to the worker pool, the round ledger, round history, and the report. r[chaoscontrol.explorer_outcomes.failed_branches]
- [ ] [depends:outcomes-status] Update `scripts/bug-hunt.sh`, CLI help, and the README status table, and search repository consumers of explorer exit statuses. r[chaoscontrol.explorer_outcomes.exit_status]
- [ ] [parallel] Add positive and negative tests for each status, error-chain output, and failed-branch accounting. r[chaoscontrol.explorer_outcomes.exit_status] r[chaoscontrol.explorer_outcomes.error_chain] r[chaoscontrol.explorer_outcomes.failed_branches]
- [ ] [depends:outcomes-tests] Run explorer tests, one failing and one bug-finding KVM run, and Cairn validation. r[chaoscontrol.explorer_outcomes.exit_status]
