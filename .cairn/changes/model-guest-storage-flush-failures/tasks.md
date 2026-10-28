# Tasks: Model guest storage flush failures

## Virtio flush model

- [x] [serial] Record the current block, virtio, fault, guest, and evidence baselines. r[chaoscontrol.storage_faults.virtio_flush]
  - Evidence: [flush model evidence](evidence/2026-10-27-flush-model.md) and its [captured baseline](evidence/baseline.json): 87 block/virtio, 14 SMR, 109 Raft guest, and 9 Redb guest cases passed before any edit.
- [x] [serial] Add pure `VIRTIO_BLK_T_FLUSH` descriptor admission and transfer-budget rules. r[chaoscontrol.storage_faults.virtio_flush]
  - Evidence: `virtio_request::plan_block_request` admits header+status only, refuses data descriptors and unsupported devices, and transfers zero bytes. Negative cases: `flush_requires_the_device_feature_and_carries_no_data_descriptor`, `flush_status_descriptor_must_be_a_writable_byte`.
- [x] [serial] Add feature negotiation and shell execution for supported flush requests. r[chaoscontrol.storage_faults.virtio_flush]
  - Evidence: `VIRTIO_BLK_F_FLUSH` is offered only by flush-capable devices; validated negotiation already rejects unoffered driver features. `supported_flush_commits_volatile_state_and_is_advertised`, `failed_flush_reports_ioerr_and_retains_volatile_state`, and `device_without_flush_support_neither_advertises_nor_executes_flush` exercise the shell path.
- [x] [parallel] Add valid and malformed flush descriptor, feature, status, and overflow tests. r[chaoscontrol.storage_faults.validation]
  - Evidence: the two planner tests above plus the existing aggregate-limit and chain-shape suites. No flush case bypasses the shared descriptor budget.

## Fault and cache behavior

- [x] [serial] Add deterministic success, immediate error, delayed error, and lying-flush outcomes bound to one attempt. r[chaoscontrol.storage_faults.flush_faults]
  - Evidence: `BlockFault::FlushError { delay_ns }`, `DiskFlushError`/`BlockFlushError`, and `successful_flush_commits_volatile_pages_and_reports_durability`, `flush_error_delay_binds_one_attempt_and_the_next_flush_succeeds`, `lying_flush_acknowledges_without_committing_volatile_pages`.
- [x] [serial] Model application restart with cache retained, explicit cache eviction, and guest reboot as distinct profiles. r[chaoscontrol.storage_faults.cache_recovery_modes]
  - Evidence: `CacheRecoveryMode` and `DeterministicBlock::recover_after_restart`; `restart_profiles_distinguish_retained_cache_eviction_and_reboot`.
- [x] [serial] Preserve failed-clean-page behavior until a new guest write occurs. r[chaoscontrol.storage_faults.cache_recovery_modes]
  - Evidence: `failed_flush_preserves_pages_until_the_guest_rewrites_them`; `DirtyOverlay` carries the failed-clean set across restart overlays.
- [x] [parallel] Add positive rewrite and negative retry-without-rewrite, stale-cache, partial-write, corruption, and full-disk tests. r[chaoscontrol.storage_faults.validation]
  - Evidence: `failed_flush_preserves_pages_until_the_guest_rewrites_them` covers rewrite and retry-without-rewrite; `stale_cache_survives_application_restart_but_not_reboot` covers stale cache; the existing torn-write, corruption, and full-disk suites remain green.

## Persistent consensus workload

- [x] [serial] Add one persistent deterministic block device per Raft node and bind term, vote, log, snapshot, and application state. r[chaoscontrol.smr_storage_recovery.persistent_nodes]
  - Evidence: `chaoscontrol-smr` `persistence`; six cases cover round trip, crash-before-flush, write/capacity denial, corruption/truncation, inconsistent logs, and independent cohort disks. `chaoscontrol-raft-guest` `persistence_bridge` binds an actual `Node`'s term, vote, log, and applied prefix to its own disk and restores them after restart.
- [x] [serial] Add restart, lagging-replica, unavailable-peer, corruption, and local-repair schedules. r[chaoscontrol.smr_storage_recovery.persistent_nodes]
  - Evidence: `chaoscontrol-smr` `storage_schedule`; five cases cover applied effects, intact local repair, empty/out-of-range/over-bound denial, receipt cohort binding, and receipt identity/participant denials.
- [x] [depends:protocol-observation-cohorts] Add oracles for acknowledged-data loss, committed truncation, unsafe voting, conflicting history, and unknown-commitment progress. r[chaoscontrol.smr_storage_recovery.protocol_oracles]
  - Evidence: `chaoscontrol-smr` `storage_recovery::validate_storage_recovery` with positive and five negative classes.
- [x] [depends:protocol-observation-cohorts] Add participant-scoped recovery progress for local sufficiency, peer-available repair, complete global absence, incomplete observation, and finite virtual horizons. r[chaoscontrol.smr_storage_recovery.recovery_progress]
  - Evidence: `storage_recovery::evaluate_recovery_progress` returns pass/fail/not-evaluated/incomplete for local, peer, global-absence, unnecessary-repair, missed-repair, horizon, and incomplete-observation cases.
- [ ] [parallel] Add positive local-only and peer repair plus negative unnecessary remote repair, missed available item, incomplete observation, and quorum-intersection storage-loss campaigns. r[chaoscontrol.smr_storage_recovery.recovery_progress] r[chaoscontrol.smr_storage_recovery.validation]

## Evidence and closeout

- [x] [serial] Emit exact cohort identities, observations, oracle versions, and non-claims in receipts. r[chaoscontrol.smr_storage_recovery.evidence]
  - Evidence: `identify_storage_recovery_receipt`/`validate_storage_recovery_receipt` bind candidate, guest, kernel, filesystem, device, workload, schedule, oracle version, participants, safety findings, and non-claims; positive and drift/overclaim denials recorded.
- [ ] [parallel] Run Redb and Molten candidate campaigns for local-only recovery, peer repair, global absence, and incomplete observation through the saved-evidence shell. r[chaoscontrol.smr_storage_recovery.recovery_progress] r[chaoscontrol.smr_storage_recovery.validation]
- [ ] [serial] Run focused tests, formatting, Clippy, Octet, Cairn gates, and relevant KVM or Nix checks. r[chaoscontrol.storage_faults.validation] r[chaoscontrol.smr_storage_recovery.validation]

## Verification Coverage

- `Scenario: Valid guest flush completes` -> virtio planner and shell tests
- `Scenario: Flush returns an immediate error` -> flush fault-state test
- `Scenario: Application restarts with cache retained` -> restart-profile and stale-cache tests
- `Scenario: Failed page is rewritten` -> failed-clean and rewrite test
- `Scenario: One Raft node loses stable state` -> persistent-node campaign (open)
- `Scenario: Commitment remains unknown` -> fail-closed oracle test (open)
- `Scenario: Receipt cohort changes` -> identity rejection test (open)
