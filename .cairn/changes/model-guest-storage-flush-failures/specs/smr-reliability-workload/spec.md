# SMR Reliability Workload Delta

## ADDED Requirements

### Requirement: Persistent per-node consensus disks
r[chaoscontrol.smr_storage_recovery.persistent_nodes] The Raft storage-recovery workload MUST give each node an independent deterministic disk for term, vote, log, snapshot, and application state.

#### Scenario: Node process restarts
- GIVEN one node has durable and volatile consensus state
- WHEN its process restarts under a selected cache profile
- THEN only state admitted by that profile remains available

#### Scenario: One node disk is corrupted
- GIVEN a committed history and one scheduled disk corruption
- WHEN the node reopens storage
- THEN the workload reports the faulty local state before voting or repair

### Requirement: Protocol-aware storage oracles
r[chaoscontrol.smr_storage_recovery.protocol_oracles] The workload MUST reject acknowledged-data loss, committed-entry truncation, unsafe post-repair voting, conflicting committed histories, and progress that assumes unknown commitment.

#### Scenario: Commitment remains unknown
- GIVEN incomplete peer evidence for a faulty entry
- WHEN recovery requests an oracle decision
- THEN the oracle accepts a wait result and rejects unsupported truncation

#### Scenario: Repaired node votes immediately
- GIVEN local database repair completed without peer validation
- WHEN the node attempts to vote
- THEN the oracle reports a safety violation

### Requirement: Recovery progress is participant-scoped
r[chaoscontrol.smr_storage_recovery.recovery_progress] The storage-recovery workload MUST evaluate each affected participant with explicit local sufficiency, admitted peer, disruptive-fault, source-sequence, loss, final-drain, and finite virtual-horizon facts. The result MUST be pass, fail, not-evaluated, or incomplete. The oracle MUST reject unnecessary remote repair when admitted local durable state is sufficient. It MUST report global unavailability only when complete observations show that every permitted source lacks the exact required item.

#### Scenario: Local state can complete recovery
- GIVEN one participant has every required admitted durable item locally
- WHEN it starts remote repair
- THEN the oracle reports an unnecessary-repair failure.

#### Scenario: A peer has the missing committed item
- GIVEN a stable recovery window and one admitted peer with the exact missing committed item
- WHEN the affected participant does not repair it within the declared virtual horizon
- THEN the oracle reports a participant liveness failure.

#### Scenario: All permitted sources lack the item
- GIVEN complete lossless observations show that every permitted source lacks the exact required item
- WHEN recovery classification runs
- THEN the oracle can report bounded global unavailability.

#### Scenario: Peer observation has a gap
- GIVEN a required source sequence, loss counter, or final-drain fact is incomplete
- WHEN recovery classification runs
- THEN the result is incomplete and cannot become pass or global absence.

### Requirement: Storage-recovery evidence
r[chaoscontrol.smr_storage_recovery.evidence] Campaign receipts MUST bind exact candidate, guest, kernel, filesystem, device, fault, schedule, workload, and oracle identities with explicit non-claims.

#### Scenario: Receipt cohort changes
- GIVEN a saved receipt and a different filesystem or candidate identity
- WHEN evidence validation runs
- THEN validation rejects the stale receipt

### Requirement: Persistent consensus validation
r[chaoscontrol.smr_storage_recovery.validation] The workload MUST include positive peer-repair campaigns and negative storage-loss, lagging-replica, unavailable-peer, corruption, and unsafe-election campaigns.

#### Scenario: Quorum-intersection node loses stable state
- GIVEN one faulty quorum-intersection node, one unavailable correct node, and one lagging node
- WHEN recovery and election execute
- THEN the oracle rejects committed-history loss and unsafe rejoin
