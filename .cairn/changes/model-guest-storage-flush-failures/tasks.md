# Tasks: Model guest storage flush failures

## Virtio flush model

- [ ] [serial] Record the current block, virtio, fault, guest, and evidence baselines. r[chaoscontrol.storage_faults.virtio_flush]
- [ ] [serial] Add pure `VIRTIO_BLK_T_FLUSH` descriptor admission and transfer-budget rules. r[chaoscontrol.storage_faults.virtio_flush]
- [ ] [serial] Add feature negotiation and shell execution for supported flush requests. r[chaoscontrol.storage_faults.virtio_flush]
- [ ] [parallel] Add valid and malformed flush descriptor, feature, status, and overflow tests. r[chaoscontrol.storage_faults.validation]

## Fault and cache behavior

- [ ] [serial] Add deterministic success, immediate error, delayed error, and lying-flush outcomes bound to one attempt. r[chaoscontrol.storage_faults.flush_faults]
- [ ] [serial] Model application restart with cache retained, explicit cache eviction, and guest reboot as distinct profiles. r[chaoscontrol.storage_faults.cache_recovery_modes]
- [ ] [serial] Preserve failed-clean-page behavior until a new guest write occurs. r[chaoscontrol.storage_faults.cache_recovery_modes]
- [ ] [parallel] Add positive rewrite and negative retry-without-rewrite, stale-cache, partial-write, corruption, and full-disk tests. r[chaoscontrol.storage_faults.validation]

## Persistent consensus workload

- [ ] [serial] Add one persistent deterministic block device per Raft node and bind term, vote, log, snapshot, and application state. r[chaoscontrol.smr_storage_recovery.persistent_nodes]
- [ ] [serial] Add restart, lagging-replica, unavailable-peer, corruption, and local-repair schedules. r[chaoscontrol.smr_storage_recovery.persistent_nodes]
- [ ] [depends:protocol-observation-cohorts] Add oracles for acknowledged-data loss, committed truncation, unsafe voting, conflicting history, and unknown-commitment progress. r[chaoscontrol.smr_storage_recovery.protocol_oracles]
- [ ] [depends:protocol-observation-cohorts] Add participant-scoped recovery progress for local sufficiency, peer-available repair, complete global absence, incomplete observation, and finite virtual horizons. r[chaoscontrol.smr_storage_recovery.recovery_progress]
- [ ] [parallel] Add positive local-only and peer repair plus negative unnecessary remote repair, missed available item, incomplete observation, and quorum-intersection storage-loss campaigns. r[chaoscontrol.smr_storage_recovery.recovery_progress] r[chaoscontrol.smr_storage_recovery.validation]

## Evidence and closeout

- [ ] [serial] Emit exact cohort identities, observations, oracle versions, and non-claims in receipts. r[chaoscontrol.smr_storage_recovery.evidence]
- [ ] [parallel] Run Redb and Molten candidate campaigns for local-only recovery, peer repair, global absence, and incomplete observation through the saved-evidence shell. r[chaoscontrol.smr_storage_recovery.recovery_progress] r[chaoscontrol.smr_storage_recovery.validation]
- [ ] [serial] Run focused tests, formatting, Clippy, Octet, Cairn gates, and relevant KVM or Nix checks. r[chaoscontrol.storage_faults.validation] r[chaoscontrol.smr_storage_recovery.validation]

## Verification Coverage

- `Scenario: Valid guest flush completes` -> virtio parser and execution tests
- `Scenario: Flush returns an immediate error` -> fault-state test
- `Scenario: Application restarts with cache retained` -> stale-cache test
- `Scenario: Failed page is rewritten` -> recovery test
- `Scenario: One Raft node loses stable state` -> persistent-node campaign
- `Scenario: Commitment remains unknown` -> fail-closed oracle test
- `Scenario: Receipt cohort changes` -> identity rejection test
